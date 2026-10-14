import { describe, expect, it } from '@jest/globals';

import { MockLegacyDfuTarget } from '@/dfu/mock-legacy-target';
import { FileService } from '@/protocol/file-service';
import { installImage, PackagesClient, type WatchApp } from '@/protocol/packages';
import { DEFAULT_QUICK_APPS, writeQuickApps } from '@/protocol/quick-apps';
import { testImage } from '@/test-support/app-images';

import {
  addQuickApp,
  canBeQuickApp,
  describeQuickApp,
  loadQuickApps,
  quickAppChoices,
  removeQuickApp,
} from './quick-apps-form';
import { moveItem } from './remote-form';

const app = (id: string, name: string, kind: WatchApp['kind'] = 'app'): WatchApp => ({
  id,
  name,
  kind,
  enabled: true,
  bytes: 1000,
  need: null,
  group: 'other',
});

const INSTALLED = [
  app('face', 'Clock', 'face'),
  app('launcher', 'Launcher'),
  app('steps', 'Steps'),
  app('timer', 'Timer'),
  app('heart', 'Heart rate'),
];

describe('choosing quick apps', () => {
  it('leaves out faces and the launcher', () => {
    expect(canBeQuickApp(app('timer', 'Timer'))).toBe(true);
    expect(canBeQuickApp(app('face', 'Clock', 'face'))).toBe(false);
    expect(canBeQuickApp(app('launcher', 'Launcher'))).toBe(false);
  });

  it('offers the installed apps not yet in the ring, in the launcher order', () => {
    expect(quickAppChoices(['steps'], INSTALLED).map((choice) => choice.id)).toEqual(['timer', 'heart']);
    expect(quickAppChoices(['steps', 'timer', 'heart'], INSTALLED)).toEqual([]);
  });

  it('adds an app at the end once only', () => {
    expect(addQuickApp(['steps'], 'timer')).toEqual(['steps', 'timer']);
    expect(addQuickApp(['steps', 'timer'], 'steps')).toEqual(['steps', 'timer']);
  });

  it('removes an entry by its place', () => {
    expect(removeQuickApp(['steps', 'timer', 'heart'], 1)).toEqual(['steps', 'heart']);
    expect(removeQuickApp(['steps'], 3)).toEqual(['steps']);
  });

  it('moves entries up and down within the ring', () => {
    expect(moveItem(['steps', 'timer', 'heart'], 2, -1)).toEqual(['steps', 'heart', 'timer']);
    expect(moveItem(['steps', 'timer', 'heart'], 0, -1)).toEqual(['steps', 'timer', 'heart']);
  });

  it('names an installed app and marks one the watch would skip', () => {
    expect(describeQuickApp('heart', INSTALLED)).toEqual({ name: 'Heart rate', installed: true });
    expect(describeQuickApp('music', INSTALLED)).toEqual({ name: 'music', installed: false });
  });
});

describe('loading the ring from the watch', () => {
  async function watchWith(...ids: string[]) {
    const watch = new MockLegacyDfuTarget('AA:BB:CC:DD:EE:00', 'neotime', true);
    const files = await FileService.open(watch);
    const packages = await PackagesClient.open(watch);
    for (const id of ids) {
      await installImage(files, packages, testImage(id, id.toUpperCase()));
    }
    return { files, packages };
  }

  it('opens on the default ring and the installed apps when the watch has no file', async () => {
    const { files, packages } = await watchWith('steps', 'timer');
    const loaded = await loadQuickApps(files, packages);
    expect(loaded.ring).toEqual([...DEFAULT_QUICK_APPS]);
    expect(loaded.fromFile).toBe(false);
    expect(loaded.apps.map((installed) => installed.id)).toEqual(['steps', 'timer']);
    expect(quickAppChoices(loaded.ring, loaded.apps).map((choice) => choice.id)).toEqual(['timer']);
  });

  it('opens on the ring the watch holds after a save', async () => {
    const { files, packages } = await watchWith('timer');
    await writeQuickApps(files, ['timer', 'heart']);
    const loaded = await loadQuickApps(files, packages);
    expect(loaded).toMatchObject({ ring: ['timer', 'heart'], fromFile: true });
  });
});
