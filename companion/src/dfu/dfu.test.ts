import { describe, expect, it } from '@jest/globals';

import { zipSync } from 'fflate';

import { crc32, nextAddress, readU32le, u32le } from './bytes';
import { flashFirmware, type FlashStep } from './flash';
import { parseNotification } from './legacy';
import { MockLegacyDfuTarget } from './mock-legacy-target';
import { MockDfuTarget } from './mock-target';
import { FirmwarePackageError, readFirmwarePackage } from './package';
import { parseResponse } from './secure';

function firmwareZip(init: Uint8Array, image: Uint8Array): Uint8Array {
  return zipSync({
    'manifest.json': new TextEncoder().encode(
      JSON.stringify({
        manifest: { application: { bin_file: 'app.bin', dat_file: 'app.dat' } },
      }),
    ),
    'app.dat': init,
    'app.bin': image,
  });
}

function pattern(length: number): Uint8Array {
  const out = new Uint8Array(length);
  for (let i = 0; i < length; i++) {
    out[i] = (i * 7 + 13) & 0xff;
  }
  return out;
}

describe('bytes', () => {
  it('round-trips a 32-bit value', () => {
    expect(readU32le(u32le(0xdeadbeef), 0)).toBe(0xdeadbeef);
  });

  it('matches the reference CRC-32', () => {
    expect(crc32(new TextEncoder().encode('123456789'))).toBe(0xcbf43926);
  });

  it('finds the bootloader address, carrying where it must', () => {
    expect(nextAddress('AA:BB:CC:DD:EE:01')).toBe('AA:BB:CC:DD:EE:02');
    expect(nextAddress('aa:bb:cc:dd:ee:ff')).toBe('AA:BB:CC:DD:EF:00');
  });

  it('rejects something that is not an address', () => {
    expect(() => nextAddress('mock-pinetime')).toThrow(/Bluetooth address/);
  });
});

describe('the firmware package', () => {
  it('reads the images the manifest names', () => {
    const firmware = readFirmwarePackage(firmwareZip(pattern(20), pattern(300)), 'micropython.zip');
    expect(firmware.name).toBe('micropython.zip');
    expect(firmware.images).toHaveLength(1);
    expect(firmware.images[0].part).toBe('application');
    expect(firmware.images[0].image).toEqual(pattern(300));
  });

  it('sends the softdevice before the application', () => {
    const zip = zipSync({
      'manifest.json': new TextEncoder().encode(
        JSON.stringify({
          manifest: {
            application: { bin_file: 'app.bin', dat_file: 'app.dat' },
            softdevice: { bin_file: 'sd.bin', dat_file: 'sd.dat' },
          },
        }),
      ),
      'app.dat': pattern(8),
      'app.bin': pattern(16),
      'sd.dat': pattern(8),
      'sd.bin': pattern(16),
    });
    expect(readFirmwarePackage(zip).images.map((image) => image.part)).toEqual([
      'softdevice',
      'application',
    ]);
  });

  it('falls back to a bare pair of files', () => {
    const zip = zipSync({ 'app.dat': pattern(8), 'app.bin': pattern(64) });
    expect(readFirmwarePackage(zip).images[0].image).toEqual(pattern(64));
  });

  it('refuses a file that is not a zip', () => {
    expect(() => readFirmwarePackage(pattern(40))).toThrow(FirmwarePackageError);
  });

  it('refuses a zip with no firmware in it', () => {
    expect(() => readFirmwarePackage(zipSync({ 'readme.txt': pattern(4) }))).toThrow(
      /no firmware image/,
    );
  });
});

describe('notification parsing', () => {
  it('reads a legacy packet receipt', () => {
    expect(parseNotification(Uint8Array.from([17, 0x40, 0x01, 0x00, 0x00]))).toEqual({
      kind: 'receipt',
      received: 0x140,
    });
  });

  it('reads a legacy response', () => {
    expect(parseNotification(Uint8Array.from([16, 3, 1]))).toEqual({
      kind: 'response',
      procedure: 3,
      result: 1,
    });
  });

  it('reads what a secure SELECT reports', () => {
    const value = new Uint8Array([0x60, 0x06, 0x01, ...u32le(4096), ...u32le(64), ...u32le(99)]);
    expect(parseResponse(value)).toEqual({
      procedure: 6,
      result: 1,
      maxSize: 4096,
      offset: 64,
      crc: 99,
    });
  });

  it('keeps a failure result without reading past it', () => {
    expect(parseResponse(Uint8Array.from([0x60, 0x01, 0x0a]))).toEqual({
      procedure: 1,
      result: 0x0a,
    });
  });
});

describe('flashing a watch', () => {
  it('jumps into the bootloader and sends the whole image', async () => {
    const init = pattern(141);
    const image = pattern(9000);
    const target = new MockDfuTarget();
    const steps: FlashStep[] = [];

    await flashFirmware(target, readFirmwarePackage(firmwareZip(init, image)), {
      onStep: (step) => steps.push(step),
    });

    expect(target.received.init).toEqual(init);
    expect(target.received.image).toEqual(image);
    expect(steps.map((step) => step.step)).toContain('resetting');
    expect(steps.at(-1)).toEqual({ step: 'finished' });
    expect(steps).toContainEqual({ step: 'detected', flavour: 'secure' });
  });

  it('reports progress that ends at the image size', async () => {
    const image = pattern(5000);
    const sent: number[] = [];
    await flashFirmware(new MockDfuTarget(), readFirmwarePackage(firmwareZip(pattern(64), image)), {
      onStep: (step) => {
        if (step.step === 'progress' && step.progress.phase === 'sending') {
          sent.push(step.progress.sent);
        }
      },
    });
    expect(sent.at(-1)).toBe(image.length);
    expect(sent.every((value, index) => index === 0 || value >= sent[index - 1])).toBe(true);
  });

  it('uses the reset command when one is given', async () => {
    const target = new MockDfuTarget();
    let reset = false;
    await flashFirmware(target, readFirmwarePackage(firmwareZip(pattern(16), pattern(64))), {
      enterDfu: async () => {
        reset = true;
        target.enterBootloader();
      },
    });
    expect(reset).toBe(true);
    expect(target.received.image).toEqual(pattern(64));
  });
});

describe('flashing a legacy bootloader', () => {
  it('sends the init packet and the image, then activates', async () => {
    const init = pattern(64);
    const image = pattern(3333);
    const target = new MockLegacyDfuTarget();

    await flashFirmware(target, readFirmwarePackage(firmwareZip(init, image)), {
      enterDfu: async () => target.enterBootloader(),
    });

    expect(target.received.init).toEqual(init);
    expect(target.received.image).toEqual(image);
    expect(target.finished).toBe(true);
  });

  it('reports the bootloader flavour it found', async () => {
    const target = new MockLegacyDfuTarget();
    const flavours: string[] = [];
    await flashFirmware(target, readFirmwarePackage(firmwareZip(pattern(20), pattern(100))), {
      enterDfu: async () => target.enterBootloader(),
      onStep: (step) => {
        if (step.step === 'detected') {
          flavours.push(step.flavour);
        }
      },
    });
    expect(flavours).toEqual(['legacy']);
  });
});
