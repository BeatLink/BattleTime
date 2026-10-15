import { describe, expect, it } from '@jest/globals';

import { MockLegacyDfuTarget } from '@/dfu/mock-legacy-target';

import { FileService, FileServiceError } from './file-service';
import {
  decodeQuickApps,
  DEFAULT_QUICK_APPS,
  encodeQuickApps,
  QUICK_APPS_PATH,
  QuickAppsError,
  readQuickApps,
  resetQuickApps,
  writeQuickApps,
  type QuickAppsFiles,
} from './quick-apps';

const text = (bytes: Uint8Array) => new TextDecoder().decode(bytes);
const bytes = (value: string) => new TextEncoder().encode(value);

const mockWatch = () => new MockLegacyDfuTarget('AA:BB:CC:DD:EE:00', 'battletime', true);

describe('the quick-app ring file', () => {
  it('lives in the watch settings folder', () => {
    expect(QUICK_APPS_PATH).toBe('/settings/quick_apps');
  });

  it('defaults to the ring the watch uses without the file', () => {
    expect(DEFAULT_QUICK_APPS).toEqual(['steps', 'stopwatch', 'heart', 'weather', 'music']);
  });

  it('writes one id a line, each ending in a newline', () => {
    expect(text(encodeQuickApps(['steps', 'stopwatch', 'heart']))).toBe('steps\nstopwatch\nheart\n');
    expect(encodeQuickApps([])).toHaveLength(0);
  });

  it('refuses an empty id or one with spaces or line breaks', () => {
    expect(() => encodeQuickApps(['steps', ''])).toThrow(QuickAppsError);
    expect(() => encodeQuickApps(['two words'])).toThrow('"two words" is not an app id');
    expect(() => encodeQuickApps(['a\nb'])).toThrow(QuickAppsError);
  });

  it('reads the ids back in order', () => {
    expect(decodeQuickApps(encodeQuickApps(['heart', 'steps']))).toEqual(['heart', 'steps']);
  });

  it('ignores blank lines, surrounding spaces and carriage returns as the watch does', () => {
    expect(decodeQuickApps(bytes('  steps \r\n\r\n\tstopwatch\n\nheart'))).toEqual(['steps', 'stopwatch', 'heart']);
    expect(decodeQuickApps(bytes(''))).toEqual([]);
    expect(decodeQuickApps(bytes('\n \r\n'))).toEqual([]);
  });
});

describe('the ring on the watch', () => {
  it('falls back to the default ring when the file is missing', async () => {
    const service = await FileService.open(mockWatch());
    const onWatch = await readQuickApps(service);
    expect(onWatch).toEqual({ ids: [...DEFAULT_QUICK_APPS], fromFile: false });
    onWatch.ids.push('changed');
    expect(DEFAULT_QUICK_APPS).toHaveLength(5);
  });

  it('writes the ring and reads it back from the mock watch', async () => {
    const watch = mockWatch();
    const service = await FileService.open(watch);
    await writeQuickApps(service, ['heart', 'stopwatch']);
    expect(text(watch.fileService.files.get(QUICK_APPS_PATH)!)).toBe('heart\nstopwatch\n');
    expect(await readQuickApps(service)).toEqual({ ids: ['heart', 'stopwatch'], fromFile: true });
  });

  it('reads an empty file as an empty ring, not the default', async () => {
    const watch = mockWatch();
    watch.fileService.files.set(QUICK_APPS_PATH, new Uint8Array(0));
    const service = await FileService.open(watch);
    expect(await readQuickApps(service)).toEqual({ ids: [], fromFile: true });
  });

  it('removes the file to go back to the default, and a file already gone is fine', async () => {
    const watch = mockWatch();
    const service = await FileService.open(watch);
    await writeQuickApps(service, ['heart']);
    await resetQuickApps(service);
    expect(watch.fileService.files.has(QUICK_APPS_PATH)).toBe(false);
    await resetQuickApps(service);
    expect((await readQuickApps(service)).fromFile).toBe(false);
  });

  it('passes on any other failure', async () => {
    const failing: QuickAppsFiles = {
      read: async () => {
        throw new FileServiceError('/settings/quick_apps: the watch could not do that.');
      },
      write: async () => {},
      remove: async () => {
        throw new FileServiceError('/settings/quick_apps: the watch could not do that.');
      },
    };
    await expect(readQuickApps(failing)).rejects.toThrow('could not do that');
    await expect(resetQuickApps(failing)).rejects.toThrow('could not do that');
  });

  it('does not write a ring with a malformed id', async () => {
    const watch = mockWatch();
    const service = await FileService.open(watch);
    await expect(writeQuickApps(service, ['bad id'])).rejects.toThrow(QuickAppsError);
    expect(watch.fileService.files.has(QUICK_APPS_PATH)).toBe(false);
  });
});
