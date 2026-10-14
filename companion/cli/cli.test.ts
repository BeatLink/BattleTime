import { describe, expect, it, jest } from '@jest/globals';

import { mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { zipSync } from 'fflate';

import { MockLegacyDfuTarget } from '@/dfu/mock-legacy-target';
import { FileService } from '@/protocol/file-service';
import { NOTE_FONT_PATH, NOTE_PATH } from '@/protocol/notes';

import type { WatchSession } from './ble';
import { debug } from './commands/debug';
import { flash } from './commands/flash';
import { recovery } from './commands/recovery';
import { diag } from './commands/send';
import { note } from './commands/watch';

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

describe('the note command', () => {
  // A session whose file service is the mock watch's.
  function noteSession() {
    const watch = new MockLegacyDfuTarget('AA:BB:CC:DD:EE:00', 'neotime', true);
    const service = FileService.open(watch);
    return { session: { deviceId: watch.deviceId, files: () => service } as unknown as WatchSession, service };
  }

  it('sends a text file and its font, and removes it again', async () => {
    const { session, service } = noteSession();
    const path = join(mkdtempSync(join(tmpdir(), 'neotime-')), 'list.txt');
    writeFileSync(path, 'Milk\nBread\n');

    await note(session, [path, 'large']);
    const files = await service;
    expect(new TextDecoder().decode(await files.read(NOTE_PATH))).toBe('Milk\nBread\n');
    expect(new TextDecoder().decode(await files.read(NOTE_FONT_PATH))).toBe('large');

    await note(session, ['remove']);
    await expect(files.read(NOTE_PATH)).rejects.toThrow();
  });

  it('sends typed text, and the text a URL answers with', async () => {
    const { session, service } = noteSession();
    const fetcher = (async () => new Response('From the web')) as unknown as typeof fetch;

    await note(session, ['text', 'Milk and eggs', 'small']);
    const files = await service;
    expect(new TextDecoder().decode(await files.read(NOTE_PATH))).toBe('Milk and eggs');
    expect(new TextDecoder().decode(await files.read(NOTE_FONT_PATH))).toBe('small');

    await note(session, ['url', 'https://example.org/note.txt'], fetcher);
    expect(new TextDecoder().decode(await files.read(NOTE_PATH))).toBe('From the web');
  });

  it('asks for a file and a known font', async () => {
    const { session } = noteSession();

    await expect(note(session, [])).rejects.toThrow('Give the text file');
    await expect(note(session, ['url'])).rejects.toThrow('Give the URL to show.');
    await expect(note(session, ['list.txt', 'huge'])).rejects.toThrow('The font is one of small, normal, medium, large');
  });
});

describe('the debug command', () => {
  it('saves the report as JSON and tells the watch it has the crash', async () => {
    const session = new FakeSession('AA:BB:CC:DD:EE:00', 'neotime', true);
    Object.assign(session, {
      packages: async () => ({ apps: async () => [] }),
      files: async () => ({ storage: async () => ({ free: 4096 }) }),
    });
    const path = join(mkdtempSync(join(tmpdir(), 'neotime-')), 'report.json');

    const written = await debug(session as unknown as WatchSession, path);

    const report = JSON.parse(readFileSync(written, 'utf8'));
    expect(written).toBe(path);
    expect(report.lastRestart.crash.kind).toBe('outOfMemory');
    expect(report.apps).toEqual([]);
    expect(report.freeStorageBytes).toBe(4096);
    expect(session.debugReportsSaved).toBe(1);
  });

  it('names the file after the time when none is given', async () => {
    const session = new FakeSession();
    const folder = mkdtempSync(join(tmpdir(), 'neotime-'));
    const before = process.cwd();
    process.chdir(folder);
    try {
      const written = await debug(session as unknown as WatchSession);
      expect(written).toMatch(/^neotime-debug-.*\.json$/);
      expect(readFileSync(join(folder, written), 'utf8')).toContain('"outOfMemory"');
    } finally {
      process.chdir(before);
    }
  });
});

describe('the diag command', () => {
  it('prints the counters, then the last restart and its crash', async () => {
    const printed: string[] = [];
    const write = jest.spyOn(process.stdout, 'write').mockImplementation((text) => {
      printed.push(String(text));
      return true;
    });
    try {
      await diag(new FakeSession() as unknown as WatchSession);
    } finally {
      write.mockRestore();
    }

    expect(printed[0]).toBe('Shell loops: 1234\n');
    expect(printed).toContain('Last restart: watchdog\n');
    expect(printed).toContain('NeoTime stopped: out of memory while alarm was open.\n');
  });
});
