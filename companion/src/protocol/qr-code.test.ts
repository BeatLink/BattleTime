import { describe, expect, it } from '@jest/globals';

import { MockLegacyDfuTarget } from '@/dfu/mock-legacy-target';

import { FileMissingError, FileService, FileServiceError } from './file-service';
import {
  bitmapModule,
  cleanQrLabel,
  darkRuns,
  escapeWifiField,
  packQrModules,
  qrBitmap,
  qrDensityWarning,
  QrCodeError,
  qrModules,
  addQrCode,
  MOST_QR_CODES,
  moveQrCode,
  packQrList,
  parseQrList,
  qrCodePath,
  QR_LIST_PATH,
  readQrList,
  removeAllQrCodes,
  removeQrCode,
  renameQrCode,
  wifiText,
  type QrFiles,
} from './qr-code';

// ============================================================================
// A watch's files in memory
// ============================================================================

class FakeFiles implements QrFiles {
  readonly files = new Map<string, Uint8Array>();
  failRemove = false;
  async read(path: string): Promise<Uint8Array> {
    const contents = this.files.get(path);
    if (!contents) {
      throw new FileMissingError(`${path} does not exist.`);
    }
    return contents;
  }

  async write(path: string, contents: Uint8Array): Promise<void> {
    this.files.set(path, contents);
  }

  async remove(path: string): Promise<void> {
    if (this.failRemove) {
      throw new FileServiceError(`${path}: the watch could not do that.`);
    }
    if (!this.files.delete(path)) {
      throw new FileMissingError(`${path} does not exist.`);
    }
  }
}
// ============================================================================
// Encoding
// ============================================================================

describe('qrBitmap', () => {
  it('packs a 21-module code into three bytes a row, leftmost module in the top bit', () => {
    const bitmap = qrBitmap('HELLO', 'M');

    expect(bitmap[0]).toBe(21);
    expect(bitmap[1]).toBe(21);
    expect(bitmap.length).toBe(2 + 21 * 3);
    // The top row is the two finder patterns' edges, "#######...###.#######", padded with zeros.
    expect([...bitmap.subarray(2, 5)]).toEqual([0xfe, 0x3b, 0xf8]);
  });

  it('matches the encoder module for module', () => {
    const modules = qrModules('https://example.org', 'Q');
    const bitmap = packQrModules(modules);

    expect(bitmap[0]).toBe(modules.size);
    for (let row = 0; row < modules.size; row++) {
      for (let column = 0; column < modules.size; column++) {
        expect(bitmapModule(bitmap, row, column)).toBe(modules.data[row * modules.size + column] === 1);
      }
    }
  });

  it('packs a width that is a multiple of eight without a spare byte', () => {
    const data = new Uint8Array(64);
    data[0] = 1;
    data[7] = 1;
    data[63] = 1;

    const bitmap = packQrModules({ size: 8, data });

    expect([...bitmap]).toEqual([8, 8, 0x81, 0, 0, 0, 0, 0, 0, 0x01]);
  });

  it('uses error correction M by default and grows with the level', () => {
    expect(qrBitmap('HELLO')).toEqual(qrBitmap('HELLO', 'M'));
    expect(qrBitmap('x'.repeat(100), 'H')[0]).toBeGreaterThan(qrBitmap('x'.repeat(100), 'L')[0]);
  });

  it('refuses empty text, a code over 120 modules and text too long for any code', () => {
    expect(() => qrBitmap('')).toThrow(QrCodeError);
    expect(() => qrBitmap('x'.repeat(1000), 'M')).toThrow('121 modules wide; the watch takes up to 120');
    expect(() => qrBitmap('x'.repeat(5000), 'L')).toThrow('too long for a QR code');
  });
});

describe('darkRuns', () => {
  it('joins dark modules side by side in a row, including one that reaches the edge', () => {
    const data = new Uint8Array(9);
    data.set([1, 1, 0, 0, 1, 1, 1, 0, 1]);

    expect(darkRuns(packQrModules({ size: 3, data }))).toEqual([
      { row: 0, column: 0, length: 2 },
      { row: 1, column: 1, length: 2 },
      { row: 2, column: 0, length: 1 },
      { row: 2, column: 2, length: 1 },
    ]);
  });

  it('starts the top row of a code with the finder pattern', () => {
    expect(darkRuns(qrBitmap('HELLO'))[0]).toEqual({ row: 0, column: 0, length: 7 });
  });
});

describe('qrDensityWarning', () => {
  it('warns only above 74 modules', () => {
    expect(qrDensityWarning(qrBitmap('HELLO'))).toBeNull();
    expect(qrDensityWarning(qrBitmap('x'.repeat(400), 'M'))).toContain('77 modules wide');
  });
});

// ============================================================================
// The caption
// ============================================================================

describe('cleanQrLabel', () => {
  it('keeps printable ASCII and drops accents', () => {
    expect(cleanQrLabel('Guest Wi-Fi')).toBe('Guest Wi-Fi');
    expect(cleanQrLabel('Café crème')).toBe('Cafe creme');
  });

  it('replaces other characters, folds line breaks into spaces and cuts it to 24 bytes', () => {
    expect(cleanQrLabel('Tea☕ time')).toBe('Tea? time');
    expect(cleanQrLabel(' line one\r\nline two ')).toBe('line one line two');
    expect(cleanQrLabel('a'.repeat(30))).toBe('a'.repeat(24));
    expect(cleanQrLabel('x'.repeat(23) + ' y')).toBe('x'.repeat(23));
  });
});

// ============================================================================
// Wi-Fi codes
// ============================================================================

describe('wifiText', () => {
  it('escapes the characters the format uses', () => {
    expect(escapeWifiField('a\\b;c,d:e"f')).toBe('a\\\\b\\;c\\,d\\:e\\"f');
  });

  it('builds the join text for each kind of network', () => {
    expect(wifiText({ ssid: 'Home', password: 'pa;ss', security: 'WPA', hidden: false })).toBe(
      'WIFI:T:WPA;S:Home;P:pa\\;ss;H:false;;',
    );
    expect(wifiText({ ssid: 'Old:Net', password: 'abc', security: 'WEP', hidden: true })).toBe(
      'WIFI:T:WEP;S:Old\\:Net;P:abc;H:true;;',
    );
    expect(wifiText({ ssid: 'Cafe', password: 'ignored', security: 'nopass', hidden: false })).toBe(
      'WIFI:T:nopass;S:Cafe;P:;H:false;;',
    );
  });

  it('refuses a network with no name', () => {
    expect(() => wifiText({ ssid: '', password: '', security: 'nopass', hidden: false })).toThrow(QrCodeError);
  });
});
// ============================================================================
// The list
// ============================================================================

describe('packQrList', () => {
  it('writes each slot, caption length and caption padded to 24 bytes', () => {
    const file = packQrList([
      { slot: 3, label: 'Café' },
      { slot: 0, label: 'Home' },
    ]);

    expect(file.length).toBe(2 + 2 * 26);
    expect([...file.subarray(0, 8)]).toEqual([1, 2, 3, 4, 0x43, 0x61, 0x66, 0x65]);
    expect(file[2 + 26]).toBe(0);
    expect(parseQrList(file)).toEqual([
      { slot: 3, label: 'Cafe' },
      { slot: 0, label: 'Home' },
    ]);
  });

  it('refuses more codes than the watch keeps', () => {
    const entries = Array.from({ length: MOST_QR_CODES + 1 }, (_, slot) => ({ slot, label: 'A' }));
    expect(() => packQrList(entries)).toThrow(QrCodeError);
  });

  it('reads nothing from a file in another format or cut short', () => {
    expect(parseQrList(new Uint8Array([2, 1]))).toEqual([]);
    expect(parseQrList(new Uint8Array([1, 1, 0, 3]))).toEqual([]);
  });
});

// ============================================================================
// Changing the codes on the watch
// ============================================================================

describe('the codes on the watch', () => {
  it('adds codes in the first free slots, at the end of the list', async () => {
    const files = new FakeFiles();
    const first = qrBitmap('ONE');
    const second = qrBitmap('TWO');

    await addQrCode(files, first, 'One');
    const entries = await addQrCode(files, second, 'Two');

    expect(entries).toEqual([
      { slot: 0, label: 'One' },
      { slot: 1, label: 'Two' },
    ]);
    expect(files.files.get(qrCodePath(0))).toEqual(first);
    expect(files.files.get(qrCodePath(1))).toEqual(second);
    expect(await readQrList(files)).toEqual(entries);
  });

  it('reuses a removed slot and refuses a ninth code', async () => {
    const files = new FakeFiles();
    for (let count = 0; count < MOST_QR_CODES; count++) {
      await addQrCode(files, qrBitmap('HELLO'), `Code ${count}`);
    }
    await expect(addQrCode(files, qrBitmap('HELLO'), 'More')).rejects.toThrow(QrCodeError);

    await removeQrCode(files, 2);
    const entries = await addQrCode(files, qrBitmap('HELLO'), 'Again');

    expect(entries.at(-1)).toEqual({ slot: 2, label: 'Again' });
    expect(entries).toHaveLength(MOST_QR_CODES);
  });

  it('refuses a code with no caption to list it by', async () => {
    const files = new FakeFiles();
    await expect(addQrCode(files, qrBitmap('HELLO'), '  ')).rejects.toThrow(QrCodeError);

    expect(files.files.size).toBe(0);
  });

  it('renames and moves codes by rewriting only the list', async () => {
    const files = new FakeFiles();
    await addQrCode(files, qrBitmap('ONE'), 'One');
    await addQrCode(files, qrBitmap('TWO'), 'Two');
    const first = files.files.get(qrCodePath(0));

    await renameQrCode(files, 0, 'First');
    const entries = await moveQrCode(files, 1, -1);

    expect(entries).toEqual([
      { slot: 1, label: 'Two' },
      { slot: 0, label: 'First' },
    ]);
    expect(await moveQrCode(files, 1, -1)).toEqual(entries);
    expect(files.files.get(qrCodePath(0))).toBe(first);
  });

  it('removes a code and its file, and every code at once', async () => {
    const files = new FakeFiles();
    await addQrCode(files, qrBitmap('ONE'), 'One');
    await addQrCode(files, qrBitmap('TWO'), 'Two');

    expect(await removeQrCode(files, 0)).toEqual([{ slot: 1, label: 'Two' }]);
    expect(files.files.has(qrCodePath(0))).toBe(false);

    await removeAllQrCodes(files);
    await removeAllQrCodes(files);

    expect(files.files.size).toBe(0);
    expect(await readQrList(files)).toEqual([]);
  });

  it('passes on failures other than a missing file', async () => {
    const files = new FakeFiles();
    files.failRemove = true;
    await addQrCode(files, qrBitmap('ONE'), 'One');

    await expect(removeQrCode(files, 0)).rejects.toThrow('could not do that');
    await expect(removeAllQrCodes(files)).rejects.toThrow('could not do that');
  });

  it('works over the file service, making the folder as it writes', async () => {
    const watch = new MockLegacyDfuTarget('AA:BB:CC:DD:EE:00', 'neotime', true);
    const service = await FileService.open(watch);
    const bitmap = qrBitmap(wifiText({ ssid: 'Home', password: 'secret', security: 'WPA', hidden: false }));

    await addQrCode(service, bitmap, 'Home Wi-Fi');

    expect(await service.read(qrCodePath(0))).toEqual(bitmap);
    expect(await readQrList(service)).toEqual([{ slot: 0, label: 'Home Wi-Fi' }]);

    await removeAllQrCodes(service);
    await expect(service.read(QR_LIST_PATH)).rejects.toBeInstanceOf(FileMissingError);
  });
});
