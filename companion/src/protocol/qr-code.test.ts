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
  QR_CODE_PATH,
  QR_LABEL_PATH,
  removeQrCode,
  sendQrCode,
  wifiText,
  type QrFiles,
} from './qr-code';

// ============================================================================
// A watch's files in memory
// ============================================================================

class FakeFiles implements QrFiles {
  readonly files = new Map<string, Uint8Array>();
  failRemove = false;

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

const decode = (bytes: Uint8Array | undefined) => new TextDecoder().decode(bytes);

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
// Sending
// ============================================================================

describe('sendQrCode', () => {
  it('writes the code and its cleaned caption', async () => {
    const files = new FakeFiles();
    const bitmap = qrBitmap('HELLO');

    await sendQrCode(files, bitmap, 'Café');

    expect(files.files.get(QR_CODE_PATH)).toEqual(bitmap);
    expect(decode(files.files.get(QR_LABEL_PATH))).toBe('Cafe');
  });

  it('deletes the caption when there is none, also when none was there', async () => {
    const files = new FakeFiles();
    await sendQrCode(files, qrBitmap('HELLO'), 'Old');

    await sendQrCode(files, qrBitmap('HELLO'), '  ');
    await sendQrCode(files, qrBitmap('HELLO'), '');

    expect(files.files.has(QR_LABEL_PATH)).toBe(false);
    expect(files.files.has(QR_CODE_PATH)).toBe(true);
  });

  it('passes on other failures', async () => {
    const files = new FakeFiles();
    files.failRemove = true;

    await expect(sendQrCode(files, qrBitmap('HELLO'), '')).rejects.toThrow('could not do that');
    await expect(removeQrCode(files)).rejects.toThrow('could not do that');
  });

  it('removes both files, whichever are there', async () => {
    const files = new FakeFiles();
    await sendQrCode(files, qrBitmap('HELLO'), '');

    await removeQrCode(files);
    await removeQrCode(files);

    expect(files.files.size).toBe(0);
  });

  it('works over the file service, making the folder as it writes', async () => {
    const watch = new MockLegacyDfuTarget('AA:BB:CC:DD:EE:00', 'neotime', true);
    const service = await FileService.open(watch);
    const bitmap = qrBitmap(wifiText({ ssid: 'Home', password: 'secret', security: 'WPA', hidden: false }));

    await sendQrCode(service, bitmap, 'Home Wi-Fi');

    expect(await service.read(QR_CODE_PATH)).toEqual(bitmap);
    expect(decode(await service.read(QR_LABEL_PATH))).toBe('Home Wi-Fi');

    await removeQrCode(service);
    await expect(service.read(QR_CODE_PATH)).rejects.toBeInstanceOf(FileMissingError);
  });
});
