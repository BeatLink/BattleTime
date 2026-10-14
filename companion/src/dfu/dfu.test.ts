import { afterEach, describe, expect, it, jest } from '@jest/globals';

import { zipSync } from 'fflate';

import { nextAddress, readU32le, u32le } from './bytes';
import {
  CURRENT_TIME,
  encodeCurrentTime,
  encodeLocalTimeInformation,
  LOCAL_TIME_INFORMATION,
  setWatchTime,
} from '@/protocol/neotime';
import { flashFirmware, flashRecovery, type FlashStep } from './flash';
import { LegacyDfuController, parseNotification } from './legacy';
import type { DfuLink } from './link';
import { MockLegacyDfuTarget } from './mock-legacy-target';
import { FirmwarePackageError, readFirmwarePackage } from './package';
import { LEGACY_CONTROL_POINT } from './uuids';

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
  it('reads the application the manifest names', () => {
    const firmware = readFirmwarePackage(firmwareZip(pattern(20), pattern(300)), 'firmware.zip');
    expect(firmware.name).toBe('firmware.zip');
    expect(firmware.image.part).toBe('application');
    expect(firmware.image.init).toEqual(pattern(20));
    expect(firmware.image.image).toEqual(pattern(300));
  });

  it('refuses a package with a part other than the application', () => {
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
    expect(() => readFirmwarePackage(zip)).toThrow(/only an application image, and this package holds softdevice/);
  });

  it('refuses a manifest that is not JSON, or names a file the zip lacks', () => {
    const broken = zipSync({ 'manifest.json': new TextEncoder().encode('{') });
    expect(() => readFirmwarePackage(broken)).toThrow(/not valid JSON/);
    const missing = zipSync({
      'manifest.json': new TextEncoder().encode(
        JSON.stringify({ manifest: { application: { bin_file: 'app.bin', dat_file: 'app.dat' } } }),
      ),
      'app.dat': pattern(8),
    });
    expect(() => readFirmwarePackage(missing)).toThrow(/names app.bin but does not contain it/);
  });

  it('falls back to a bare pair of files', () => {
    const zip = zipSync({ 'app.dat': pattern(8), 'app.bin': pattern(64) });
    expect(readFirmwarePackage(zip).image.image).toEqual(pattern(64));
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
    expect(target.timeZoneWrites).toHaveLength(1);
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

describe('sending a recovery firmware', () => {
  it('sends the image with NeoTime\'s recovery image type, to the running firmware', async () => {
    const image = pattern(4000);
    const target = new MockLegacyDfuTarget();
    const steps: FlashStep[] = [];

    await flashRecovery(target, readFirmwarePackage(firmwareZip(pattern(14), image)), {
      onStep: (step) => steps.push(step),
    });

    expect(target.imageTypes).toEqual([0x08]);
    expect(target.received.image).toEqual(image);
    expect(target.finished).toBe(true);
    expect(steps.at(-1)).toEqual({ step: 'finished', confirmed: false });
  });

  it('writes the time once the restarted watch is back', async () => {
    const target = new MockLegacyDfuTarget();
    await flashRecovery(target, readFirmwarePackage(firmwareZip(pattern(14), pattern(64))), {
      confirm: (link) => setWatchTime(link),
    });
    expect(target.timeWrites).toHaveLength(1);
  });

  it('refuses a watch that is not running NeoTime', async () => {
    const target = new MockLegacyDfuTarget('AA:BB:CC:DD:EE:00', 'wasp-os');
    await expect(
      flashRecovery(target, readFirmwarePackage(firmwareZip(pattern(14), pattern(64)))),
    ).rejects.toThrow(/Only a watch running NeoTime/);
    expect(target.imageTypes).toEqual([]);
  });

  it('names the procedure and the reason when the watch refuses one', async () => {
    // A watch that answers every command with "not supported".
    let notify: (value: Uint8Array) => void = () => {};
    const link = {
      subscribe: async (_service: string, _characteristic: string, onValue: (value: Uint8Array) => void) => {
        notify = onValue;
        return () => {};
      },
      write: async (_service: string, characteristic: string, data: Uint8Array) => {
        if (characteristic === LEGACY_CONTROL_POINT) {
          notify(Uint8Array.from([16, data[0], 3]));
        }
      },
    } as unknown as DfuLink;
    const { image } = readFirmwarePackage(firmwareZip(pattern(14), pattern(64)));
    await expect(new LegacyDfuController(link).run(image)).rejects.toThrow('The bootloader rejected start: not supported.');
  });

  it('leaves an ordinary update on the application image type', async () => {
    const target = new MockLegacyDfuTarget();
    await flashFirmware(target, readFirmwarePackage(firmwareZip(pattern(14), pattern(64))));
    expect(target.imageTypes).toEqual([0x04]);
  });
});

describe('sending the reloader to a watch still on wasp-os', () => {
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

describe('the Local Time Information value', () => {
  afterEach(() => {
    jest.restoreAllMocks();
  });

  // The value on 1 January or 1 July, in a zone this many minutes ahead of UTC from October to March and from April to September.
  function inZone(january: number, july: number, month: 0 | 6): number[] {
    jest
      .spyOn(Date.prototype, 'getTimezoneOffset')
      .mockImplementation(function (this: Date) {
        return -(this.getMonth() >= 3 && this.getMonth() <= 8 ? july : january);
      });
    return Array.from(encodeLocalTimeInformation(new Date(2026, month, 1)));
  }

  it('is UTC with no daylight saving in UTC', () => {
    expect(inZone(0, 0, 6)).toEqual([0, 0]);
  });

  it('splits a summer offset into the standard zone and an hour of daylight saving', () => {
    expect(inZone(60, 120, 6)).toEqual([4, 4]);
    expect(inZone(60, 120, 0)).toEqual([4, 0]);
  });

  it('carries a zone west of UTC as a negative byte', () => {
    expect(inZone(-300, -240, 6)).toEqual([0x100 - 20, 4]);
    expect(inZone(-300, -300, 6)).toEqual([0x100 - 20, 0]);
  });

  it('keeps quarter-hour zones and half-hour daylight saving', () => {
    expect(inZone(345, 345, 6)).toEqual([23, 0]);
    expect(inZone(630, 660, 6)).toEqual([42, 2]);
  });

  it('counts the southern summer as daylight saving too', () => {
    expect(inZone(780, 720, 0)).toEqual([48, 4]);
  });

  it('puts an offset change daylight saving cannot express into the zone', () => {
    expect(inZone(0, 180, 6)).toEqual([12, 0]);
  });

  it('is written before the time, and skipped on firmware that does not serve it', async () => {
    // A watch that records what is written to it, serving Local Time Information only when told to.
    async function writesTo(servesZone: boolean): Promise<string[]> {
      const writes: string[] = [];
      const link = {
        hasCharacteristic: async (_service: string, characteristic: string) =>
          characteristic === CURRENT_TIME || servesZone,
        write: async (_service: string, characteristic: string) => {
          writes.push(characteristic);
        },
      } as unknown as DfuLink;
      await setWatchTime(link);
      return writes;
    }
    expect(await writesTo(true)).toEqual([LOCAL_TIME_INFORMATION, CURRENT_TIME]);
    expect(await writesTo(false)).toEqual([CURRENT_TIME]);

    const target = new MockLegacyDfuTarget();
    await setWatchTime(target, new Date(Date.UTC(2026, 6, 1)));
    expect(target.timeZoneWrites).toHaveLength(1);
    expect(target.timeWrites).toHaveLength(1);
  });
});
