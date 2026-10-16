// The app's daemon, which keeps the watch linked and does the background work in the Rust core (core/src/daemon).
//
// The window reaches it through `window.battletime.daemon` (tauri/src/bridge.js). In a plain browser there is
// none, and the window does the work itself against the mock watch.

import type { ConnectionState, WatchMode } from '@/ble/transport';
import type { StatusReply } from '@/protocol/gadgetbridge';
import type { WatchCapabilities } from '@/protocol/battletime';
import type { PhoneEvent } from '@/services/phone';
import type { RemoteLogEntry } from '@/services/remote-actions';
import type { NoteStatus } from '@/state/note-sync';
import type { RemoteStatus } from '@/state/remote-sync';

// ============================================================================
// What the daemon reports
// ============================================================================

export type DaemonState = {
  connection: ConnectionState;
  mode: WatchMode;
  watch: { id: string; name: string } | null;
  capabilities: WatchCapabilities;
  status: StatusReply | null;
  batteryLevel: number | null;
  lastSynced: number | null;
  nowPlaying: Omit<Extract<PhoneEvent, { type: 'media' }>, 'type'> | null;
  findPhone: boolean;
  lastError: string | null;
  // True while the daemon is getting the remembered watch back on its own.
  reconnecting: boolean;
  features: { noteStatus?: NoteStatus; remoteStatus?: RemoteStatus; remoteLog?: RemoteLogEntry[] };
};

// ============================================================================
// What the bridge carries
// ============================================================================

type BridgeResult<Value> = { ok: boolean; error?: string; value?: Value };

export type DaemonBridge = {
  state(): Promise<BridgeResult<DaemonState>>;
  settings(): Promise<BridgeResult<Record<string, unknown>>>;
  updateSettings(patch: object): Promise<BridgeResult<Record<string, unknown>>>;
  importSettings(settings: object): Promise<BridgeResult<boolean>>;
  connect(id: string, name: string): Promise<BridgeResult<void>>;
  disconnect(): Promise<BridgeResult<void>>;
  hold(held: boolean): Promise<BridgeResult<void>>;
  refreshStatus(): Promise<BridgeResult<void>>;
  stopFindingPhone(): Promise<BridgeResult<void>>;
  sendWeather(): Promise<BridgeResult<void>>;
  syncNote(): Promise<BridgeResult<void>>;
  forgetNote(): Promise<BridgeResult<void>>;
  sendRemoteLayout(): Promise<BridgeResult<void>>;
  testRemoteButton(index: number): Promise<BridgeResult<void>>;
  clearRemoteLog(): Promise<BridgeResult<void>>;
  onState(listener: (state: DaemonState) => void): () => void;
  onSettings(listener: (settings: Record<string, unknown>) => void): () => void;
  onSent(listener: (line: string) => void): () => void;
};

export function daemonBridge(): DaemonBridge | null {
  return (globalThis as { battletime?: { daemon?: DaemonBridge } }).battletime?.daemon ?? null;
}

// The value of a bridge call, or its error thrown.
export async function daemonValue<Value>(pending: Promise<BridgeResult<Value>>): Promise<Value> {
  const result = await pending;
  if (!result.ok) {
    throw new Error(result.error ?? 'The app could not reach its daemon.');
  }
  return result.value as Value;
}
