import { bytesToBase64 } from '@/ble/encoding';
import { SYSTEM_FILES } from '@/bundles/system-files';

import type { WatchFile } from './file-service';
import { putMissingSystemFiles, type SystemFileWriter } from './system-files';

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
    async write(path: string, data: Uint8Array): Promise<void> {
      files[path] = data;
    },
  };
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

  it('writes every file to a watch without them', async () => {
    const watch = fakeWatch({});
    expect(await putMissingSystemFiles(watch, bundle)).toEqual([bundle[0].path, bundle[1].path]);
    expect(Array.from(watch.files['/system/accelerometer/bma425'])).toEqual([4, 5, 6, 7]);
  });

  it('writes nothing to a watch that holds them', async () => {
    const watch = fakeWatch({
      '/system/accelerometer/bma421': new Uint8Array([1, 2, 3]),
      '/system/accelerometer/bma425': new Uint8Array([4, 5, 6, 7]),
    });
    expect(await putMissingSystemFiles(watch, bundle)).toEqual([]);
  });

  it('writes again a file of another size', async () => {
    const watch = fakeWatch({
      '/system/accelerometer/bma421': new Uint8Array([1, 2, 3]),
      '/system/accelerometer/bma425': new Uint8Array([9]),
    });
    expect(await putMissingSystemFiles(watch, bundle)).toEqual([bundle[1].path]);
  });
});
