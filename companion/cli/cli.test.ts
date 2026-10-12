import { describe, expect, it } from '@jest/globals';

import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { zipSync } from 'fflate';

import { MockLegacyDfuTarget } from '@/dfu/mock-legacy-target';

import type { WatchSession } from './ble';
import { flash } from './commands/flash';
import { recovery } from './commands/recovery';

// The flash command only needs the DFU link and a way to reset the watch, which is what a session gives it.
class FakeSession extends MockLegacyDfuTarget {
  readonly sent: string[] = [];

  async send(text: string) {
    this.sent.push(text);
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
  const path = join(mkdtempSync(join(tmpdir(), 'neotime-')), 'firmware.zip');
  writeFileSync(path, zip);
  return path;
}

describe('the flash command', () => {
  it('reads the package off disk, sends it to the watch and sets the time to keep it', async () => {
    const image = Uint8Array.from({ length: 2048 }, (_value, index) => index & 0xff);
    const session = new FakeSession();

    await flash(session as unknown as WatchSession, packageFile(image));

    expect(session.sent).toEqual([]);
    expect(session.received.image).toEqual(image);
    expect(session.timeWrites).toHaveLength(1);
  });

  it('restarts a watch still on wasp-os into its bootloader first', async () => {
    const image = Uint8Array.from({ length: 512 }, (_value, index) => index & 0xff);
    const session = new FakeSession('AA:BB:CC:DD:EE:00', 'wasp-os');

    await flash(session as unknown as WatchSession, packageFile(image));

    expect(session.sent[0]).toContain('enter_ota_dfu');
    expect(session.received.image).toEqual(image);
    // Waits out a real reset into the bootloader and another into the new firmware.
  }, 20000);

  it('refuses a file that is not a firmware package', async () => {
    const path = join(mkdtempSync(join(tmpdir(), 'neotime-')), 'not-firmware.zip');
    writeFileSync(path, 'hello');
    await expect(flash(new FakeSession() as unknown as WatchSession, path)).rejects.toThrow(
      /not a zip archive/,
    );
  });
});

describe('the recovery command', () => {
  it('sends the package to the recovery slot and sets the time once the watch is back', async () => {
    const image = Uint8Array.from({ length: 1500 }, (_value, index) => (index * 7) & 0xff);
    const session = new FakeSession();

    await recovery(session as unknown as WatchSession, packageFile(image));

    expect(session.imageTypes).toEqual([0x08]);
    expect(session.received.image).toEqual(image);
    expect(session.timeWrites).toHaveLength(1);
  });
});
