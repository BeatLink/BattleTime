import { describe, expect, it, jest } from '@jest/globals';

import { MockLegacyDfuTarget } from '@/dfu/mock-legacy-target';
import { appStateJson, decodeAppState } from '@/protocol/app-state';
import { FileService } from '@/protocol/file-service';
import { ReplyRouter } from '@/protocol/requests';

import type { WatchSession } from './ble';
import { alarms, parseAlarm, steps } from './commands/watch';

// What the alarms and steps commands need from a session: the file service, and a channel for the lines sent.
class FakeSession extends MockLegacyDfuTarget {
  readonly lines: string[] = [];
  readonly requests = new ReplyRouter(undefined, 10);
  private filesOpened: Promise<FileService> | null = null;
  constructor() {
    super('AA:BB:CC:DD:EE:00', 'neotime', true);
    this.fileService.addSampleRecords(new Date());
  }
  files() {
    this.filesOpened ??= FileService.open(this);
    return this.filesOpened;
  }
  async send(text: string) {
    this.lines.push(text);
  }
}

async function run(command: () => Promise<void>): Promise<string[]> {
  const said: string[] = [];
  const log = jest.spyOn(process.stdout, 'write').mockImplementation((text) => {
    said.push(String(text).trimEnd());
    return true;
  });
  try {
    await command();
  } finally {
    log.mockRestore();
  }
  return said;
}

describe('the alarms command', () => {
  it('reads the alarm app state from the watch files', async () => {
    const session = new FakeSession();
    expect(await run(() => alarms(session as unknown as WatchSession, []))).toEqual([
      '07:00  on   Weekdays',
      '09:30  off  Once',
    ]);
  });

  it('writes the alarms and tells the watch', async () => {
    const session = new FakeSession();
    const said = await run(() => alarms(session as unknown as WatchSession, ['6:05/67', '22:00/off']));
    expect(said).toEqual(['06:05  on   Weekends', '22:00  off  Once']);
    const state = appStateJson(decodeAppState(session.fileService.files.get('/apps/alarm/state')!));
    expect(state.alarm_count).toBe(2);
    expect(session.lines.join('')).toContain('"t":"state"');
    expect(await run(() => alarms(session as unknown as WatchSession, ['none']))).toEqual(['No alarms.']);
  });

  it('reads alarms given on the command line', () => {
    expect(parseAlarm('7:30/135')).toEqual({ hour: 7, minute: 30, days: 0b10101, enabled: true });
    expect(() => parseAlarm('25:00')).toThrow('HH:MM');
  });
});

describe('the steps command', () => {
  it('prints a bar for each day from the step logs', async () => {
    const said = await run(() => steps(new FakeSession() as unknown as WatchSession, 3));
    expect(said).toHaveLength(3);
  });
});
