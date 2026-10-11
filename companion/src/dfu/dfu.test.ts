import { describe, expect, it } from '@jest/globals';

import { zipSync } from 'fflate';

import { nextAddress, readU32le, u32le } from './bytes';
import { encodeCurrentTime, setWatchTime } from '@/protocol/neotime';
import { flashFirmware, type FlashStep } from './flash';
import { parseNotification } from './legacy';
import { MockLegacyDfuTarget } from './mock-legacy-target';
import { FirmwarePackageError, readFirmwarePackage } from './package';

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
    const firmware = readFirmwarePackage(firmwareZip(pattern(20), pattern(300)), 'firmware.zip');
    expect(firmware.name).toBe('firmware.zip');
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

});

describe('flashing a NeoTime watch', () => {
  it('sends the image straight to the running firmware', async () => {
    const init = pattern(141);
    const image = pattern(9000);
    const target = new MockLegacyDfuTarget();
    const steps: FlashStep[] = [];

    await flashFirmware(target, readFirmwarePackage(firmwareZip(init, image)), {
      onStep: (step) => steps.push(step),
    });

    expect(target.received.init).toEqual(init);
    expect(target.received.image).toEqual(image);
    expect(target.finished).toBe(true);
    expect(steps.map((step) => step.step)).not.toContain('resetting');
    expect(steps.at(-1)).toEqual({ step: 'finished', confirmed: false });
  });

  it('writes the time once the new firmware is back, so the watch keeps it', async () => {
    const target = new MockLegacyDfuTarget();
    const steps: FlashStep[] = [];
    await flashFirmware(target, readFirmwarePackage(firmwareZip(pattern(16), pattern(64))), {
      confirm: (link) => setWatchTime(link),
      onStep: (step) => steps.push(step),
    });
    expect(target.timeWrites).toHaveLength(1);
    expect(steps.at(-1)).toEqual({ step: 'finished', confirmed: true });
  });

  it('reports progress that ends at the image size', async () => {
    const image = pattern(5000);
    const sent: number[] = [];
    await flashFirmware(new MockLegacyDfuTarget(), readFirmwarePackage(firmwareZip(pattern(64), image)), {
      onStep: (step) => {
        if (step.step === 'progress' && step.progress.phase === 'sending') {
          sent.push(step.progress.sent);
        }
      },
    });
    expect(sent.at(-1)).toBe(image.length);
    expect(sent.every((value, index) => index === 0 || value >= sent[index - 1])).toBe(true);
  });
});

describe('flashing a watch still on wasp-os', () => {
  it('resets it into its bootloader first, then sends the image', async () => {
    const image = pattern(3333);
    const target = new MockLegacyDfuTarget('AA:BB:CC:DD:EE:00', 'wasp-os');
    let reset = false;

    await flashFirmware(target, readFirmwarePackage(firmwareZip(pattern(64), image)), {
      enterDfu: async () => {
        reset = true;
      },
    });

    expect(reset).toBe(true);
    expect(target.received.image).toEqual(image);
    expect(target.finished).toBe(true);
  });

  it('refuses when there is no way into a bootloader', async () => {
    const target = new MockLegacyDfuTarget('AA:BB:CC:DD:EE:00', 'wasp-os');
    await expect(
      flashFirmware(target, readFirmwarePackage(firmwareZip(pattern(16), pattern(64)))),
    ).rejects.toThrow(/no way into a bootloader/);
  });
});

describe('the Current Time value', () => {
  it('holds the local date and time, Monday as weekday 1', () => {
    const value = encodeCurrentTime(new Date(2026, 9, 12, 13, 45, 30, 500));
    expect(Array.from(value)).toEqual([0xea, 0x07, 10, 12, 13, 45, 30, 1, 128, 1]);
  });

  it('gives Sunday weekday 7', () => {
    expect(encodeCurrentTime(new Date(2026, 9, 11, 0, 0, 0))[7]).toBe(7);
  });
});
