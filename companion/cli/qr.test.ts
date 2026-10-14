import { describe, expect, it, jest } from '@jest/globals';

import { MockLegacyDfuTarget } from '@/dfu/mock-legacy-target';
import { FileService } from '@/protocol/file-service';
import { qrBitmap, QR_CODE_PATH, QR_LABEL_PATH } from '@/protocol/qr-code';

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
      kind: 'send',
      text: 'https://example.org',
      label: 'Site',
      level: 'H',
    });
    expect(parseQrArguments(['two', 'words'])).toEqual({ kind: 'send', text: 'two words', label: '', level: 'M' });
  });

  it('builds Wi-Fi text, guessing the security from the password', () => {
    expect(parseQrArguments(['wifi', 'Home', 'secret'])).toMatchObject({ text: 'WIFI:T:WPA;S:Home;P:secret;H:false;;' });
    expect(parseQrArguments(['wifi', 'Cafe'])).toMatchObject({ text: 'WIFI:T:nopass;S:Cafe;P:;H:false;;' });
    expect(parseQrArguments(['wifi', 'Old', 'abc', '--security', 'wep', '--hidden'])).toMatchObject({
      text: 'WIFI:T:WEP;S:Old;P:abc;H:true;;',
    });
  });

  it('reads remove, and refuses what it cannot use', () => {
    expect(parseQrArguments(['remove'])).toEqual({ kind: 'remove' });
    expect(parseQrArguments(['remove', 'me'])).toMatchObject({ text: 'remove me' });
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
  it('sends a code with its caption, then removes it', async () => {
    const session = new FakeSession();

    const { said, warned } = await run(session, 'HELLO', '--label', 'Greeting');

    expect(said).toEqual([`Sent a 21x21 code (level M) to ${QR_CODE_PATH}, captioned "Greeting".`]);
    expect(warned).toEqual([]);
    expect(session.fileService.files.get(QR_CODE_PATH)).toEqual(qrBitmap('HELLO'));
    expect(new TextDecoder().decode(session.fileService.files.get(QR_LABEL_PATH))).toBe('Greeting');

    expect((await run(session, 'remove')).said).toEqual(['Removed the code from the watch.']);
    expect(session.fileService.files.has(QR_CODE_PATH)).toBe(false);
    expect(session.fileService.files.has(QR_LABEL_PATH)).toBe(false);
  });

  it('warns about a dense code but still sends it', async () => {
    const session = new FakeSession();

    const { said, warned } = await run(session, 'x'.repeat(400));

    expect(warned[0]).toContain('may not scan');
    expect(said[0]).toBe(`Sent a 77x77 code (level M) to ${QR_CODE_PATH}.`);
  });

  it('refuses a code too big for the watch and sends nothing', async () => {
    const session = new FakeSession();

    await expect(run(session, 'x'.repeat(1000))).rejects.toThrow('the watch takes up to 120');
    expect(session.fileService.files.has(QR_CODE_PATH)).toBe(false);
  });
});
