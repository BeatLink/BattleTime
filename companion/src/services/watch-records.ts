// The watch's records that the Steps and Alarms screens and the command line show: step logs and apps' saved state.
//
// With the Rust core's sync (the Tauri app) they come from its copy and changes go through it; without
// it (the Expo phone app, Electron, the command line) they are read and written with the file service.

import { alarmsFromState, alarmStateChanges, type WatchAlarm } from '@/protocol/alarms';
import { readAppStateFile, writeAppStateFile, type AppStateJson } from '@/protocol/app-state';
import { FileMissingError, type FileService } from '@/protocol/file-service';
import type { PhoneToWatchMessage } from '@/protocol/gadgetbridge';
import { readWatchSettings, type RequestChannel } from '@/protocol/requests';
import {
  mergeSlots,
  slotsFromBlock,
  slotsFromFile,
  stepBlockPath,
  stepDayPath,
  stepDaySource,
  type StepDayReader,
} from '@/protocol/steps';

import type { SyncBridge } from './sync';

// ============================================================================
// Where the records come from
// ============================================================================

export type WatchRecords = {
  // Whether these come from the sync's copy, which is there even while the watch is away.
  synced: boolean;
  readStepDay: StepDayReader;
  // The watch's step goal, or null when it has none or it is not known yet.
  readStepGoal(): Promise<number | null>;
  // The app's state as JSON, or null when it has never saved any.
  readAppState(id: string): Promise<AppStateJson | null>;
  // Changes only the values named, and returns the state as it now is.
  writeAppState(id: string, changes: AppStateJson): Promise<AppStateJson>;
};

function valueOf<Value>(result: { ok: boolean; error?: string; value?: Value }): Value {
  if (!result.ok) {
    throw new Error(result.error ?? 'The desktop app could not reach its sync.');
  }
  return result.value as Value;
}

export function syncedRecords(bridge: SyncBridge): WatchRecords {
  return {
    synced: true,
    async readStepDay(date) {
      const day = valueOf(await bridge.read(stepDaySource(date))) as { slots?: number[] } | null;
      return Array.isArray(day?.slots) ? day.slots : null;
    },
    async readStepGoal() {
      const settings = valueOf(await bridge.read('settings')) as { step_goal?: number } | null;
      return settings?.step_goal || null;
    },
    async readAppState(id) {
      const app = valueOf(await bridge.read(`apps/${id}`)) as { state?: AppStateJson | null } | null;
      return app?.state ?? null;
    },
    async writeAppState(id, changes) {
      // The core writes the file and then sends the watch {"t":"state"} itself.
      return valueOf(await bridge.writeAppState(id, changes)) as AppStateJson;
    },
  };
}

// What reading the watch directly needs: its file service, and its message channel for settings and state news.
export type WatchLink = {
  files: () => Promise<FileService>;
  send: (message: PhoneToWatchMessage) => Promise<void>;
  requests: RequestChannel;
};

export function fileRecords({ files, send, requests }: WatchLink): WatchRecords {
  return {
    synced: false,
    async readStepDay(date) {
      const service = await files();
      const ownFile = await readIfThere(service, stepDayPath(date));
      const block = await readIfThere(service, stepBlockPath(date));
      return mergeSlots(ownFile && slotsFromFile(ownFile), block ? slotsFromBlock(block, date) : []);
    },
    async readStepGoal() {
      return (await readWatchSettings(requests)).step_goal || null;
    },
    async readAppState(id) {
      return readAppStateFile(await files(), id);
    },
    async writeAppState(id, changes) {
      const state = await writeAppStateFile(await files(), id, changes);
      await send({ t: 'state', app: id });
      return state;
    },
  };
}

// A file's bytes, or null when the watch has no such file.
async function readIfThere(service: FileService, path: string): Promise<Uint8Array | null> {
  try {
    return await service.read(path);
  } catch (error) {
    if (error instanceof FileMissingError) {
      return null;
    }
    throw error;
  }
}

// The sync's copy when the build has one, the watch's files otherwise.
export function watchRecords(bridge: SyncBridge | null, link: WatchLink): WatchRecords {
  return bridge ? syncedRecords(bridge) : fileRecords(link);
}

// ============================================================================
// Alarms
// ============================================================================

export const ALARM_APP = 'alarm';

export async function readAlarms(records: WatchRecords): Promise<WatchAlarm[]> {
  return alarmsFromState(await records.readAppState(ALARM_APP));
}

// Reads the state again first, so the places past the last alarm and the app's own keys stay as they are.
export async function writeAlarms(records: WatchRecords, alarms: WatchAlarm[]): Promise<WatchAlarm[]> {
  const changes = alarmStateChanges(await records.readAppState(ALARM_APP), alarms);
  return alarmsFromState(await records.writeAppState(ALARM_APP, changes));
}

// Whether a sync report changed what a screen shows: the alarm app's state, or any day of steps.
export function changesAlarms(changed: string[]): boolean {
  return changed.includes(`apps/${ALARM_APP}`);
}

export function changesSteps(changed: string[]): boolean {
  return changed.some((source) => source.startsWith('steps/'));
}
