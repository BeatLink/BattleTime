import { describe, expect, it } from '@jest/globals';

import { MockLegacyDfuTarget } from '@/dfu/mock-legacy-target';
import { testImage } from '@/test-support/app-images';

import { AppImageError, readImageHeader } from './app-image';
import { FileService } from './file-service';
import { MOCK_ROOM } from './mock-packages-service';
import {
  appDetail,
  appImagePath,
  installImage,
  memoryText,
  PackagesClient,
  PackagesError,
  REFUSALS,
  forgetTemporaryApp,
  runTemporaryApp,
  TEMPORARY_DATA_LIMIT,
  temporaryAppPath,
  type FileWriter,
} from './packages';

function watch() {
  return new MockLegacyDfuTarget('AA:BB:CC:DD:EE:00', 'neotime', true);
}

// A file writer that keeps what it was given, as the file transfer service would on the watch.
function writerFor(target: MockLegacyDfuTarget): FileWriter & { folders: string[] } {
  const folders: string[] = [];
  return {
    folders,
    write: async (path, data, onProgress) => {
      target.fileService.files.set(path, data);
      onProgress?.({ file: path, sent: data.length, total: data.length });
    },
    mkdir: async (path) => void folders.push(path),
  };
}

describe('the image header', () => {
  it('reads the kind, id and name', () => {
    expect(readImageHeader(testImage('timer', 'Timer'))).toMatchObject({ kind: 'app', id: 'timer', name: 'Timer' });
    expect(readImageHeader(testImage('digital', 'Digital', 'face')).kind).toBe('face');
    expect(readImageHeader(testImage('shared', 'Shared library', 'library')).kind).toBe('library');
  });

  it('refuses what is not an image', () => {
    expect(() => readImageHeader(new Uint8Array(4))).toThrow(AppImageError);
    const wrongVersion = testImage('timer', 'Timer');
    wrongVersion[4] = 9;
    expect(() => readImageHeader(wrongVersion)).toThrow('another version');
    const unknownKind = testImage('timer', 'Timer');
    unknownKind[6] = 7;
    expect(() => readImageHeader(unknownKind)).toThrow('kind');
    const noStrings = testImage('timer', 'Timer');
    noStrings[20] = 5;
    expect(() => readImageHeader(noStrings)).toThrow('no strings');
    const badIndex = testImage('timer', 'Timer');
    badIndex[18] = 9;
    expect(() => readImageHeader(badIndex)).toThrow('does not have');
    expect(() => readImageHeader(testImage('timer', 'Timer').subarray(0, 40))).toThrow('cut short');
  });
});

describe('the packages client', () => {
  it('installs an image sent as a file, lists it, hides it and removes it', async () => {
    const target = watch();
    const client = await PackagesClient.open(target);
    const writer = writerFor(target);

    const header = await installImage(writer, client, testImage('timer', 'Timer'));

    expect(header.id).toBe('timer');
    expect(writer.folders).toEqual(['/apps/timer']);
    const [listed] = await client.apps();
    expect(listed).toMatchObject({ id: 'timer', name: 'Timer', kind: 'app', enabled: true, bytes: header.bytes });
    expect(listed.need).toBeGreaterThan(header.bytes);
    await client.enable('timer', false);
    expect((await client.apps())[0].enabled).toBe(false);
    await client.enable('timer', true);
    await client.remove('timer');
    expect(await client.apps()).toEqual([]);
    client.close();
  });

  it('never sends the shared library, which ships inside the firmware', async () => {
    const target = watch();
    const client = await PackagesClient.open(target);
    const library = testImage('shared', 'Shared library', 'library');

    await expect(installImage(writerFor(target), client, library)).rejects.toThrow('ships inside the firmware');

    expect(target.fileService.files.has('/resources/shared.ntlib')).toBe(false);
  });

  it('passes on what the watch refuses', async () => {
    const target = watch();
    const client = await PackagesClient.open(target);
    await expect(client.install('nothing')).rejects.toThrow('no such app');
    target.fileService.files.set('/apps/timer/app.ntapp', testImage('dice', 'Dice'));
    await expect(client.install('timer')).rejects.toThrow("folder's name");
    target.fileService.files.set('/apps/broken/app.ntapp', new Uint8Array(8));
    await expect(client.install('broken')).rejects.toThrow('app checks');
    target.fileService.files.set('/apps/shared/app.ntapp', testImage('shared', 'Shared', 'library'));
    await expect(client.install('shared')).rejects.toThrow('ships inside the firmware');
  });

  it('gives each app\'s memory and the most an app may have', async () => {
    const target = watch();
    const client = await PackagesClient.open(target);
    const writer = writerFor(target);
    await installImage(writer, client, testImage('timer', 'Timer'));

    const listing = await client.listing();

    expect(listing.room).toBe(MOCK_ROOM);
    expect(appDetail(listing.apps[0])).toMatch(/^App · [0-9.]+ kB · needs [0-9.]+ kB$/);
    expect(appDetail({ ...listing.apps[0], kind: 'face', enabled: false, need: null })).toMatch(/^Watch face, hidden · [0-9.]+ kB$/);
    expect(await client.enable('timer', false)).toEqual({ need: listing.apps[0].need, room: MOCK_ROOM });
    expect(memoryText(33792)).toBe('33.0 kB');
  });

  it('passes on the watch\'s refusals with their number and the memory', async () => {
    const target = watch();
    const client = await PackagesClient.open(target);
    const writer = writerFor(target);
    await installImage(writer, client, testImage('launcher', 'Apps'));
    await installImage(writer, client, testImage('digital', 'Digital', 'face'));
    const timer = testImage('huge', 'Huge');
    const huge = new Uint8Array(timer.length + 30000);
    huge.set(timer);

    const tooLarge = await installImage(writer, client, huge).catch((error: PackagesError) => error);
    const launcher = await client.enable('launcher', false).catch((error: PackagesError) => error);
    const face = await client.enable('digital', false).catch((error: PackagesError) => error);

    expect(tooLarge).toMatchObject({ why: 1, room: MOCK_ROOM, need: huge.length + 8000 });
    expect((tooLarge as PackagesError).message).toBe(`${REFUSALS[1]} (needs ${memoryText(huge.length + 8000)} of 33.0 kB)`);
    expect((await client.apps()).find((app) => app.id === 'huge')?.enabled).toBe(false);
    await expect(client.enable('huge', true)).rejects.toMatchObject({ why: 1 });
    expect(launcher).toMatchObject({ why: 3, need: null, message: 'the launcher stays on' });
    expect(face).toMatchObject({ why: 4, message: 'the last watch face stays on' });
  });

  it('refuses a bad id before sending anything', async () => {
    const client = await PackagesClient.open(watch());
    for (const action of [() => client.install('../x'), () => client.remove(''), () => client.enable('a b', true)]) {
      await expect(action()).rejects.toThrow(PackagesError);
    }
    expect(() => appImagePath('x'.repeat(33))).toThrow('1 to 32');
  });

  it('ignores lines meant for the file service on the same characteristic', async () => {
    const target = watch();
    const client = await PackagesClient.open(target);
    const files = await FileService.open(target);
    expect((await files.list('/')).length).toBeGreaterThan(0);
    expect(await client.apps()).toEqual([]);
  });
});

// ============================================================================
// Temporary apps
// ============================================================================

// A test image whose header carries the given CRC, as a compiled image's does.
function imageWithCrc(id: string, crc: number, padding = 0): Uint8Array {
  const image = testImage(id, id, 'app', padding);
  new DataView(image.buffer).setUint32(12, crc, true);
  return image;
}

// The file service over the mock watch, counting the writes to each path.
async function countingFiles(target: MockLegacyDfuTarget) {
  const service = await FileService.open(target);
  const writes: string[] = [];
  const files: FileWriter = {
    write: (path, data, onProgress) => {
      writes.push(path);
      return service.write(path, data, onProgress);
    },
    mkdir: (path) => service.mkdir(path),
    readStart: (path, length) => service.readStart(path, length),
    list: (dir) => service.list(dir),
    remove: (path) => service.remove(path),
  };
  return { files, writes, service };
}

describe('temporary apps', () => {
  it('names the files in a temporary app\'s folder and refuses names that are not plain', () => {
    expect(temporaryAppPath('prompt', 'app.ntapp')).toBe('/temp/prompt/app.ntapp');
    expect(temporaryAppPath('prompt', 'text')).toBe('/temp/prompt/text');
    expect(temporaryAppPath('prompt', 'a-b_c.txt')).toBe('/temp/prompt/a-b_c.txt');
    for (const name of ['', '.', '..', 'a/b', 'x'.repeat(33), 'caf\u00e9']) {
      expect(() => temporaryAppPath('prompt', name)).toThrow(PackagesError);
    }
    expect(() => temporaryAppPath('../x', 'text')).toThrow('an app id');
  });

  it('sends the image and data the first time, opens the app, and keeps it out of the apps listing', async () => {
    const target = watch();
    const client = await PackagesClient.open(target);
    const { files, writes } = await countingFiles(target);
    const image = imageWithCrc('prompt', 0x12345678);
    const progress: string[] = [];

    const run = await runTemporaryApp(files, client, image, { text: 'Lunch?', raw: Uint8Array.from([7]) }, (step) =>
      progress.push(step.file),
    );

    expect(run).toMatchObject({ header: { id: 'prompt' }, uploaded: true, room: MOCK_ROOM });
    expect(run.need).toBeGreaterThan(image.length);
    expect(writes).toEqual(['/temp/prompt/app.ntapp', '/temp/prompt/text', '/temp/prompt/raw']);
    expect(progress).toContain('/temp/prompt/app.ntapp');
    expect(target.fileService.files.get('/temp/prompt/app.ntapp')).toEqual(image);
    expect(new TextDecoder().decode(target.fileService.files.get('/temp/prompt/text'))).toBe('Lunch?');
    expect(target.fileService.files.get('/temp/prompt/raw')).toEqual(Uint8Array.from([7]));
    expect(target.packagesService.openTemporaryApp).toBe('prompt');
    expect(await client.apps()).toEqual([]);
  });

  it('reuses the image the watch already has and only rewrites the data', async () => {
    const target = watch();
    const client = await PackagesClient.open(target);
    const { files, writes } = await countingFiles(target);
    const image = imageWithCrc('prompt', 0x12345678);
    await runTemporaryApp(files, client, image, { text: 'First' });
    writes.length = 0;

    const again = await runTemporaryApp(files, client, image, { text: 'Second' });

    expect(again.uploaded).toBe(false);
    expect(writes).toEqual(['/temp/prompt/text']);
    expect(new TextDecoder().decode(target.fileService.files.get('/temp/prompt/text'))).toBe('Second');
  });

  it('deletes data an earlier run left behind, so the app never reads it', async () => {
    const target = watch();
    const client = await PackagesClient.open(target);
    const { files } = await countingFiles(target);
    const image = imageWithCrc('prompt', 1);
    await runTemporaryApp(files, client, image, { text: 'First', extra: 'old' });
    target.fileService.folders.add('/temp/prompt/leftover');

    await runTemporaryApp(files, client, image, { text: 'Second' });

    const left = [...target.fileService.files.keys()].filter((path) => path.startsWith('/temp/prompt/')).sort();
    expect(left).toEqual(['/temp/prompt/app.ntapp', '/temp/prompt/text']);
    expect(target.fileService.folders.has('/temp/prompt/leftover')).toBe(false);
    // A writer that cannot list leaves the folder as it is.
    target.fileService.files.set('/temp/prompt/extra', Uint8Array.from([1]));
    await runTemporaryApp(writerFor(target), client, image, { text: 'Third' });
    expect(target.fileService.files.has('/temp/prompt/extra')).toBe(true);
  });

  it('sends the image again when the cached one differs or cannot be read', async () => {
    const target = watch();
    const client = await PackagesClient.open(target);
    const { files } = await countingFiles(target);
    await runTemporaryApp(files, client, imageWithCrc('prompt', 1));

    expect((await runTemporaryApp(files, client, imageWithCrc('prompt', 2))).uploaded).toBe(true);
    expect((await runTemporaryApp(files, client, imageWithCrc('prompt', 2, 10))).uploaded).toBe(true);
    // A writer that cannot read part of a file always sends the image.
    expect((await runTemporaryApp(writerFor(target), client, imageWithCrc('prompt', 2, 10))).uploaded).toBe(true);
    // A cached file shorter than a header is not the image.
    target.fileService.files.set('/temp/prompt/app.ntapp', Uint8Array.from([1, 2]));
    expect((await runTemporaryApp(files, client, imageWithCrc('prompt', 2, 10))).uploaded).toBe(true);
    // A read that fails sends the image too.
    const failing: FileWriter = { ...files, readStart: () => Promise.reject(new Error('gone')) };
    expect((await runTemporaryApp(failing, client, imageWithCrc('prompt', 2, 10))).uploaded).toBe(true);
  });

  it('refuses bad data file names, large data, faces and libraries before sending anything', async () => {
    const target = watch();
    const client = await PackagesClient.open(target);
    const { files, writes } = await countingFiles(target);
    const image = imageWithCrc('prompt', 1);

    await expect(runTemporaryApp(files, client, image, { 'app.ntapp': 'x' })).rejects.toThrow("is the app's image");
    await expect(runTemporaryApp(files, client, image, { 'a/b': 'x' })).rejects.toThrow('file name');
    await expect(runTemporaryApp(files, client, image, { ['x'.repeat(33)]: 'x' })).rejects.toThrow('file name');
    await expect(
      runTemporaryApp(files, client, image, { text: 'x'.repeat(TEMPORARY_DATA_LIMIT + 1) }),
    ).rejects.toThrow('text holds 4097 bytes; a temporary app\'s data file holds at most 4096.');
    await expect(runTemporaryApp(files, client, testImage('digital', 'Digital', 'face'))).rejects.toThrow('watch face');
    await expect(runTemporaryApp(files, client, testImage('shared', 'Shared', 'library'))).rejects.toThrow(
      'shared library',
    );
    expect(writes).toEqual([]);
    // Exactly the limit is fine.
    await runTemporaryApp(files, client, image, { text: new Uint8Array(TEMPORARY_DATA_LIMIT) });
  });

  it('passes on what the watch refuses, with the memory when that is the reason', async () => {
    const target = watch();
    const client = await PackagesClient.open(target);
    const { files } = await countingFiles(target);

    const refused = runTemporaryApp(files, client, imageWithCrc('huge', 1, 30000));
    await expect(refused).rejects.toMatchObject({ why: 1, room: MOCK_ROOM });
    await expect(refused).rejects.toThrow(REFUSALS[1]);
    expect(target.packagesService.openTemporaryApp).toBeNull();

    const missing = client.run('nothing');
    await expect(missing).rejects.toThrow('the image is missing or would not open');
    await expect(missing).rejects.toMatchObject({ why: null });
    target.fileService.files.set('/temp/other/app.ntapp', imageWithCrc('prompt', 1));
    await expect(client.run('other')).rejects.toThrow('would not open');
    target.fileService.files.set('/temp/broken/app.ntapp', new Uint8Array(4));
    await expect(client.run('broken')).rejects.toThrow('would not open');
    await expect(client.run('../x')).rejects.toThrow('an app id');
  });

  it('stops the open app, keeping its files, and forgets one app or all of them', async () => {
    const target = watch();
    const client = await PackagesClient.open(target);
    const { files, service } = await countingFiles(target);
    await runTemporaryApp(files, client, imageWithCrc('prompt', 1), { text: 'Hi' });

    await client.stop();
    await client.stop();
    expect(target.packagesService.openTemporaryApp).toBeNull();
    expect(target.fileService.files.has('/temp/prompt/app.ntapp')).toBe(true);

    await runTemporaryApp(files, client, imageWithCrc('other', 1));
    await forgetTemporaryApp(service, client, 'other');
    expect(target.packagesService.openTemporaryApp).toBeNull();
    expect(target.fileService.files.has('/temp/other/app.ntapp')).toBe(false);
    expect(target.fileService.folders.has('/temp/other')).toBe(false);
    expect(target.fileService.files.has('/temp/prompt/text')).toBe(true);
    // Forgetting what is not there is fine.
    await forgetTemporaryApp(service, client, 'other');

    await forgetTemporaryApp(service, client);
    expect([...target.fileService.files.keys()].some((path) => path.startsWith('/temp/'))).toBe(false);
    await forgetTemporaryApp(service, client);
    await expect(forgetTemporaryApp(service, client, '../x')).rejects.toThrow('an app id');
    await expect(
      forgetTemporaryApp({ remove: () => Promise.reject(new Error('the watch could not do that')) }, client, 'a'),
    ).rejects.toThrow('could not do that');
  });
});
