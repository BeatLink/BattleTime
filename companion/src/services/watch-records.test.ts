import { describe, expect, it } from '@jest/globals';

import { MockGadgetbridge } from '@/ble/mock-gadgetbridge';
import { MockLegacyDfuTarget } from '@/dfu/mock-legacy-target';
import { decodeAppState, appStateJson } from '@/protocol/app-state';
import { FileService } from '@/protocol/file-service';
import { decodeWatchLine, encodeForWatch, type PhoneToWatchMessage } from '@/protocol/gadgetbridge';
import { ReplyRouter } from '@/protocol/requests';
import { fileFromSlots, readStepHistory, stepDayPath } from '@/protocol/steps';

import type { SyncBridge, SyncReport } from './sync';
import { changesAlarms, changesSteps, fileRecords, readAlarms, syncedRecords, watchRecords, writeAlarms } from './watch-records';

const TODAY = new Date(2026, 9, 14, 12, 0);

// The mock watch with its sample records, its file service, and the lines the companion sends it.
async function mockWatch() {
  const watch = new MockLegacyDfuTarget('AA:BB:CC:DD:EE:00', 'battletime', true);
  watch.fileService.addSampleRecords(TODAY);
  const service = await FileService.open(watch);
  const mock = new MockGadgetbridge();
  const sent: PhoneToWatchMessage[] = [];
  const send = async (message: PhoneToWatchMessage) => {
    sent.push(message);
    for (const line of mock.handle(encodeForWatch(message)) ?? []) {
      const decoded = decodeWatchLine(line);
      if (decoded) requests.push(decoded);
    }
  };
  const requests: ReplyRouter = new ReplyRouter(send, 1000);
  const records = fileRecords({ files: async () => service, send, requests });
  return { watch, records, sent };
}

// A sync bridge holding the given sources, as the desktop app's core gives them.
function fakeBridge(sources: Record<string, unknown>) {
  const writes: { id: string; changes: object }[] = [];
  const listeners: ((report: SyncReport) => void)[] = [];
  const bridge: SyncBridge = {
    settings: async () => ({ ok: true, value: { intervalSeconds: 300 } }),
    setSettings: async () => ({ ok: true }),
    now: async () => ({ ok: true }),
    read: async (source) => ({ ok: true, value: sources[source] ?? null }),
    list: async (prefix) => ({ ok: true, value: Object.keys(sources).filter((source) => source.startsWith(prefix)) }),
    writeAppState: async (id, changes) => {
      writes.push({ id, changes });
      const app = sources[`apps/${id}`] as { state: Record<string, unknown> };
      app.state = { ...app.state, ...changes };
      return { ok: true, value: app.state };
    },
    writeSettings: async () => ({ ok: false, error: 'not here' }),
    onReport: (listener) => {
      listeners.push(listener);
      return () => undefined;
    },
  };
  return { bridge, writes };
}

describe('records from the watch files', () => {
  it('reads step logs, with a missing day as none', async () => {
    const { records } = await mockWatch();
    const history = await readStepHistory(records.readStepDay, 6, TODAY);
    expect(history.map((day) => day.slots !== null)).toEqual([true, true, true, true, false, true]);
    expect(history[0].total).toBeGreaterThan(0);
    // Today stops at noon.
    expect(history[0].slots?.length).toBeLessThanOrEqual(120);
    expect(await records.readStepGoal()).toBe(10000);
  });

  it('reads a day older firmware kept as a file together with its block', async () => {
    const { watch, records } = await mockWatch();
    const slots = new Array<number>(82).fill(0);
    slots[0] = 40;
    slots[81] = 7;
    const ownFile = fileFromSlots(slots);
    const withoutIt = await records.readStepDay(TODAY);
    watch.fileService.files.set(stepDayPath(TODAY), ownFile);

    const merged = await records.readStepDay(TODAY);

    expect(merged?.[0]).toBe((withoutIt?.[0] ?? 0) + 40);
    expect(merged?.[81]).toBe((withoutIt?.[81] ?? 0) + 7);
  });

  it('reads and writes the alarms, finds the next ring again, and tells the watch', async () => {
    const { watch, records, sent } = await mockWatch();
    expect(await readAlarms(records)).toEqual([
      { hour: 7, minute: 0, days: 0x1f, enabled: true },
      { hour: 9, minute: 30, days: 0, enabled: false },
    ]);
    const written = await writeAlarms(records, [{ hour: 6, minute: 45, days: 0x60, enabled: true }]);
    expect(written).toEqual([{ hour: 6, minute: 45, days: 0x60, enabled: true }]);
    const state = appStateJson(decodeAppState(watch.fileService.files.get('/apps/alarm/state')!));
    expect(state.alarm_count).toBe(1);
    expect((state.alarms as { hour: number[] }).hour.slice(0, 3)).toEqual([6, 9, 8]);
    // The app's next ring is found again for the one alarm left, which rings on a weekend morning.
    expect(state.next_ring_place).toBe(0);
    expect(new Date(state.next_ring_at as number).getHours()).toBe(6);
    expect([0, 6]).toContain(new Date(state.next_ring_at as number).getDay());
    expect(sent).toContainEqual({ t: 'state', app: 'alarm' });
    // The mock watch takes the news without answering, as the watch does.
    expect(new MockGadgetbridge().handle(encodeForWatch({ t: 'state', app: 'alarm' }))).toEqual([]);
  });

  it('says the app has saved nothing when its state file is missing', async () => {
    const { watch, records } = await mockWatch();
    watch.fileService.files.delete('/apps/alarm/state');
    expect(await records.readAppState('alarm')).toBeNull();
    await expect(readAlarms(records)).rejects.toThrow('open it on the watch once');
  });
});

describe('records from the sync', () => {
  const sources = () => ({
    'steps/2026/10-14': { size: 4, slots: [12, 30] },
    settings: { step_goal: 8000 },
    'apps/alarm': { hidden: false, state: { alarm_count: 1, alarms: { hour: [7, 8], minute: [0, 0], days: [0, 0], enabled: [true, false] } } },
  });

  it('reads the synced copy', async () => {
    const records = syncedRecords(fakeBridge(sources()).bridge);
    expect(records.synced).toBe(true);
    expect(await records.readStepDay(TODAY)).toEqual([12, 30]);
    expect(await records.readStepDay(new Date(2026, 9, 13))).toBeNull();
    expect(await records.readStepGoal()).toBe(8000);
    expect(await readAlarms(records)).toEqual([{ hour: 7, minute: 0, days: 0, enabled: true }]);
  });

  it('writes through the core, which tells the watch itself', async () => {
    const { bridge, writes } = fakeBridge(sources());
    const records = watchRecords(bridge, { files: () => Promise.reject(new Error('unused')), send: async () => {}, requests: new ReplyRouter() });
    const alarms = [
      { hour: 7, minute: 0, days: 0, enabled: false },
      { hour: 22, minute: 15, days: 0x7f, enabled: true },
    ];
    expect(await writeAlarms(records, alarms)).toEqual(alarms);
    expect(writes).toEqual([
      { id: 'alarm', changes: { alarm_count: 2, alarms: { hour: [7, 22], minute: [0, 15], days: [0, 0x7f], enabled: [false, true] } } },
    ]);
  });

  it('passes on a failed read', async () => {
    const { bridge } = fakeBridge({});
    bridge.read = async () => ({ ok: false, error: 'gone' });
    await expect(syncedRecords(bridge).readStepDay(TODAY)).rejects.toThrow('gone');
  });

  it('tells which reports change what', () => {
    expect(changesAlarms(['apps/alarm'])).toBe(true);
    expect(changesAlarms(['apps/timer'])).toBe(false);
    expect(changesSteps(['steps/2026/10-14'])).toBe(true);
    expect(changesSteps(['settings'])).toBe(false);
  });
});
