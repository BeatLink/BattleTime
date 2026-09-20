import { describe, expect, it } from '@jest/globals';

import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { zipSync } from 'fflate';

import { MockDfuTarget } from '@/dfu/mock-target';

import type { WatchSession } from './ble';
import { flash } from './commands/flash';

// The flash command only needs the DFU link and a way to reset the watch,
// which is what a session gives it.
class FakeSession extends MockDfuTarget {
  readonly sent: string[] = [];

  async send(text: string) {
    this.sent.push(text);
    this.enterBootloader();
  }
}

function packageFile(image: Uint8Array): string {
  const zip = zipSync({
    'manifest.json': new TextEncoder().encode(
      JSON.stringify({ manifest: { application: { bin_file: 'a.bin', dat_file: 'a.dat' } } }),
    ),
    'a.dat': Uint8Array.from([1, 2, 3, 4]),
    'a.bin': image,
  });
  const path = join(mkdtempSync(join(tmpdir(), 'waspos-')), 'firmware.zip');
  writeFileSync(path, zip);
  return path;
}

describe('the flash command', () => {
  it('reads the package off disk and sends it to the watch', async () => {
    const image = Uint8Array.from({ length: 2048 }, (_value, index) => index & 0xff);
    const session = new FakeSession();

    await flash(session as unknown as WatchSession, packageFile(image));

    expect(session.sent[0]).toContain('enter_ota_dfu');
    expect(session.received.image).toEqual(image);
  });

  it('refuses a file that is not a firmware package', async () => {
    const path = join(mkdtempSync(join(tmpdir(), 'waspos-')), 'not-firmware.zip');
    writeFileSync(path, 'hello');
    await expect(flash(new FakeSession() as unknown as WatchSession, path)).rejects.toThrow(
      /not a zip archive/,
    );
  });
});
