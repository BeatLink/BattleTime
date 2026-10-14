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
