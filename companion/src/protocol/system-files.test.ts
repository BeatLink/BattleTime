import { bytesToBase64 } from '@/ble/encoding';
import { SYSTEM_FILES } from '@/bundles/system-files';

import type { WatchFile } from './file-service';
import { crc32 } from './crc32';
import { putMissingSystemFiles, SYSTEM_MANIFEST_PATH, type SystemFileWriter } from './system-files';

// A watch's files by path, listed by folder as the file service lists them.
function fakeWatch(held: Record<string, Uint8Array>): SystemFileWriter & { files: Record<string, Uint8Array> } {
  const files = { ...held };
  return {
    files,
    async list(dir: string): Promise<WatchFile[]> {
      const inside = Object.keys(files).filter((path) => path.startsWith(`${dir}/`));
      if (inside.length === 0) throw new Error(`${dir} is missing`);
      return inside.map((path) => ({ name: path.slice(dir.length + 1), path, size: files[path].length, directory: false }));
    },
    async read(path: string): Promise<Uint8Array> {
      if (!(path in files)) throw new Error(`${path} is missing`);
      return files[path];
    },
    async write(path: string, data: Uint8Array): Promise<void> {
      files[path] = data;
    },
  };
}

// The manifest a companion writes after putting these files.
function manifestOf(held: Record<string, Uint8Array>): Uint8Array {
  const lines = Object.entries(held).map(
    ([path, bytes]) => `${crc32(bytes).toString(16).padStart(8, '0')} ${bytes.length} ${path}\n`,
  );
  return new TextEncoder().encode(lines.join(''));
}

const bundle = [
  { path: '/system/accelerometer/bma421', base64: bytesToBase64(new Uint8Array([1, 2, 3])) },
  { path: '/system/accelerometer/bma425', base64: bytesToBase64(new Uint8Array([4, 5, 6, 7])) },
];

describe('putMissingSystemFiles', () => {
  it('bundles both accelerometer files, the icons and the native modules at their paths', () => {
    expect(SYSTEM_FILES.map((file) => file.path)).toEqual([
      '/system/accelerometer/bma421',
      '/system/accelerometer/bma425',
      '/system/font',
      '/system/icons',
      '/system/modules/p256',
      '/system/modules/test',
      '/system/modules/verifier',
      '/system/shell/notifications/app.ntapp',
      '/system/shell/quick-settings/app.ntapp',
    ]);
  });

  it('writes every file to a watch without them, then the manifest', async () => {
    const watch = fakeWatch({});
    expect(await putMissingSystemFiles(watch, bundle)).toEqual([bundle[0].path, bundle[1].path]);
    expect(Array.from(watch.files['/system/accelerometer/bma425'])).toEqual([4, 5, 6, 7]);
    expect(watch.files[SYSTEM_MANIFEST_PATH]).toEqual(
      manifestOf({
        '/system/accelerometer/bma421': new Uint8Array([1, 2, 3]),
        '/system/accelerometer/bma425': new Uint8Array([4, 5, 6, 7]),
      }),
    );
  });

  it('writes nothing to a watch that holds them as its manifest says', async () => {
    const held = {
      '/system/accelerometer/bma421': new Uint8Array([1, 2, 3]),
      '/system/accelerometer/bma425': new Uint8Array([4, 5, 6, 7]),
    };
    const watch = fakeWatch({ ...held, [SYSTEM_MANIFEST_PATH]: manifestOf(held) });
    expect(await putMissingSystemFiles(watch, bundle)).toEqual([]);
  });

  it('writes again a file whose contents changed at the same size', async () => {
    const old = {
      '/system/accelerometer/bma421': new Uint8Array([1, 2, 3]),
      '/system/accelerometer/bma425': new Uint8Array([4, 5, 6, 0]),
    };
    const watch = fakeWatch({ ...old, [SYSTEM_MANIFEST_PATH]: manifestOf(old) });
    expect(await putMissingSystemFiles(watch, bundle)).toEqual([bundle[1].path]);
    expect(await putMissingSystemFiles(watch, bundle)).toEqual([]);
  });

  it('writes again a file of another size', async () => {
    const held = {
      '/system/accelerometer/bma421': new Uint8Array([1, 2, 3]),
      '/system/accelerometer/bma425': new Uint8Array([4, 5, 6, 7]),
    };
    const watch = fakeWatch({ ...held, [SYSTEM_MANIFEST_PATH]: manifestOf(held) });
    watch.files['/system/accelerometer/bma425'] = new Uint8Array([9]);
    expect(await putMissingSystemFiles(watch, bundle)).toEqual([bundle[1].path]);
  });

  it('writes every file once to a watch without a manifest, as one from before it', async () => {
    const watch = fakeWatch({
      '/system/accelerometer/bma421': new Uint8Array([1, 2, 3]),
      '/system/accelerometer/bma425': new Uint8Array([4, 5, 6, 7]),
    });
    expect(await putMissingSystemFiles(watch, bundle)).toHaveLength(2);
    expect(await putMissingSystemFiles(watch, bundle)).toEqual([]);
  });
});
