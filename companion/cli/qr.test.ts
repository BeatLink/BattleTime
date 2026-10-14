import { describe, expect, it, jest } from '@jest/globals';

import { MockLegacyDfuTarget } from '@/dfu/mock-legacy-target';
import { FileService } from '@/protocol/file-service';
import { qrBitmap, qrCodePath, QR_LIST_PATH } from '@/protocol/qr-code';

import type { WatchSession } from './ble';
import { parseQrArguments, qr } from './commands/qr';

// What the qr command needs from a session: the file service on one watch.
class FakeSession extends MockLegacyDfuTarget {
  constructor() {
    super('AA:BB:CC:DD:EE:00', 'neotime', true);
  }
  private filesOpened: Promise<FileService> | null = null;
  files() {
    this.filesOpened ??= FileService.open(this);
    return this.filesOpened;
  }
}

// Runs the command with the arguments given and returns what it printed and warned.
async function run(session: FakeSession, ...args: string[]): Promise<{ said: string[]; warned: string[] }> {
  const said: string[] = [];
  const warned: string[] = [];
  const out = jest.spyOn(process.stdout, 'write').mockImplementation((text) => {
    said.push(String(text).trimEnd());
    return true;
  });
  const err = jest.spyOn(process.stderr, 'write').mockImplementation((text) => {
    warned.push(String(text).trimEnd());
    return true;
  });
  try {
    await qr(session as unknown as WatchSession, args);
  } finally {
    out.mockRestore();
    err.mockRestore();
  }
  return { said, warned };
}

// ============================================================================
// Arguments
// ============================================================================

describe('parseQrArguments', () => {
  it('reads text, a caption and a level', () => {
    expect(parseQrArguments(['https://example.org', '--label', 'Site', '--level', 'h'])).toEqual({
      kind: 'add',
      text: 'https://example.org',
      label: 'Site',
      level: 'H',
    });
    expect(parseQrArguments(['two', 'words'])).toEqual({
      kind: 'add',
      text: 'two words',
      label: 'two words',
      level: 'M',
    });
    expect(parseQrArguments(['https://example.org/a'])).toMatchObject({ label: 'example.org/a' });
  });

  it('builds Wi-Fi text, guessing the security from the password', () => {
    expect(parseQrArguments(['wifi', 'Home', 'secret'])).toMatchObject({
      text: 'WIFI:T:WPA;S:Home;P:secret;H:false;;',
      label: 'Home',
    });
    expect(parseQrArguments(['wifi', 'Cafe'])).toMatchObject({ text: 'WIFI:T:nopass;S:Cafe;P:;H:false;;' });
    expect(parseQrArguments(['wifi', 'Old', 'abc', '--security', 'wep', '--hidden'])).toMatchObject({
      text: 'WIFI:T:WEP;S:Old;P:abc;H:true;;',
    });
  });

  it('reads the list commands, taking other words as text', () => {
    expect(parseQrArguments(['list'])).toEqual({ kind: 'list' });
    expect(parseQrArguments(['remove', 'all'])).toEqual({ kind: 'remove-all' });
    expect(parseQrArguments(['remove', '2'])).toEqual({ kind: 'remove', number: 2 });
    expect(parseQrArguments(['rename', '1', 'New', 'name'])).toEqual({ kind: 'rename', number: 1, label: 'New name' });
    expect(parseQrArguments(['move', '3', 'up'])).toEqual({ kind: 'move', number: 3, step: -1 });
    expect(parseQrArguments(['move', '3', 'down'])).toEqual({ kind: 'move', number: 3, step: 1 });
    expect(parseQrArguments(['remove', 'me'])).toMatchObject({ kind: 'add', text: 'remove me' });
    expect(parseQrArguments(['move', '0', 'up'])).toMatchObject({ kind: 'add', text: 'move 0 up' });
  });

  it('refuses what it cannot use', () => {
    expect(() => parseQrArguments([])).toThrow('Give the text');
    expect(() => parseQrArguments(['wifi'])).toThrow('network name');
    expect(() => parseQrArguments(['x', '--level', 'Z'])).toThrow('not Z');
    expect(() => parseQrArguments(['x', '--level'])).toThrow('not nothing');
    expect(() => parseQrArguments(['wifi', 'a', '--security', 'WPA3'])).toThrow('not WPA3');
    expect(() => parseQrArguments(['wifi', 'a', '--security'])).toThrow('not nothing');
  });
});

// ============================================================================
// The command
// ============================================================================

describe('the qr command', () => {
  it('adds codes, lists, renames, moves and removes them', async () => {
    const session = new FakeSession();
    const files = session.fileService.files;

    const { said, warned } = await run(session, 'HELLO', '--label', 'Greeting');
    await run(session, 'wifi', 'Home', 'secret');

    expect(said).toEqual(['Added a 21x21 code (level M) as number 1, "Greeting".']);
    expect(warned).toEqual([]);
    expect(files.get(qrCodePath(0))).toEqual(qrBitmap('HELLO'));
    expect((await run(session, 'list')).said).toEqual(['1. Greeting', '2. Home']);
    expect((await run(session, 'rename', '2', 'Home Wi-Fi')).said).toEqual(['1. Greeting', '2. Home Wi-Fi']);
    expect((await run(session, 'move', '2', 'up')).said).toEqual(['1. Home Wi-Fi', '2. Greeting']);
    expect((await run(session, 'remove', '2')).said).toEqual(['1. Home Wi-Fi']);
    expect(files.has(qrCodePath(0))).toBe(false);
    await expect(run(session, 'remove', '2')).rejects.toThrow('has 1 code, so no number 2');

    expect((await run(session, 'remove', 'all')).said).toEqual(['Removed every code from the watch.']);
    expect(files.has(QR_LIST_PATH)).toBe(false);
    expect((await run(session, 'list')).said).toEqual(['The watch has no QR codes.']);
  });

  it('warns about a dense code but still sends it', async () => {
    const session = new FakeSession();

    const { said, warned } = await run(session, 'x'.repeat(400));

    expect(warned[0]).toContain('may not scan');
    expect(said[0]).toBe(`Added a 77x77 code (level M) as number 1, "${'x'.repeat(24)}".`);
  });

  it('refuses a code too big for the watch and sends nothing', async () => {
    const session = new FakeSession();

    await expect(run(session, 'x'.repeat(1000))).rejects.toThrow('the watch takes up to 120');
    expect(session.fileService.files.has(QR_LIST_PATH)).toBe(false);
  });
});
