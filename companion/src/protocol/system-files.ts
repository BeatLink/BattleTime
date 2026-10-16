// The files every watch needs that are not apps, such as each accelerometer's configuration and the shared library's
// icons, which the firmware no longer carries in its shared library. The companion puts any a watch lacks; core/src/sync/system_files.rs does the
// same for the app, and the two must agree.

import { base64ToBytes } from '@/ble/encoding';
import { SYSTEM_FILES, type BundledSystemFile } from '@/bundles/system-files';

import type { WatchFile } from './file-service';

export type SystemFileWriter = {
  list(dir: string): Promise<WatchFile[]>;
  write(path: string, data: Uint8Array): Promise<void>;
};

// Writes each system file the watch lacks or holds at another size, giving the paths it wrote.
export async function putMissingSystemFiles(
  files: SystemFileWriter,
  bundle: BundledSystemFile[] = SYSTEM_FILES,
): Promise<string[]> {
  const written: string[] = [];
  for (const file of bundle) {
    const contents = base64ToBytes(file.base64);
    const slash = file.path.lastIndexOf('/');
    const folder = file.path.slice(0, slash) || '/';
    const name = file.path.slice(slash + 1);
    // A folder the watch has never had lists as missing, which means the file is too.
    const entries = await files.list(folder).catch(() => [] as WatchFile[]);
    const held = entries.some((entry) => entry.name === name && !entry.directory && entry.size === contents.length);
    if (!held) {
      await files.write(file.path, contents);
      written.push(file.path);
    }
  }
  return written;
}
