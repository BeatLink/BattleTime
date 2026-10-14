// Sync: the desktop app's Rust core keeps a copy of every setting and record on the watch, read again
// on one interval that the whole companion shares. docs/sync.md describes it in full.
//
// The window reaches it through `window.neotime.sync` (tauri/src/bridge.js). Builds without it still
// use the interval for their own status reads.

// ============================================================================
// The interval
// ============================================================================

export const DEFAULT_SYNC_INTERVAL_SECONDS = 5 * 60;

export const SYNC_INTERVAL_CHOICES: { label: string; value: number }[] = [
  { label: '1 min', value: 60 },
  { label: '5 min', value: 5 * 60 },
  { label: '15 min', value: 15 * 60 },
  { label: '30 min', value: 30 * 60 },
  { label: '1 hour', value: 60 * 60 },
];

// A stored interval outside what the core accepts falls back to the default.
export function syncIntervalMs(seconds: number): number {
  const fine = Number.isInteger(seconds) && seconds >= 60 && seconds <= 24 * 60 * 60;
  return (fine ? seconds : DEFAULT_SYNC_INTERVAL_SECONDS) * 1000;
}

// ============================================================================
// What the desktop bridge carries
// ============================================================================

export type SyncReport = { changed: string[]; errors: string[]; finishedAt: number };

type BridgeResult<Value> = { ok: boolean; error?: string; value?: Value };

export type SyncBridge = {
  settings(): Promise<BridgeResult<{ intervalSeconds: number }>>;
  setSettings(settings: { intervalSeconds: number }): Promise<BridgeResult<void>>;
  now(): Promise<BridgeResult<void>>;
  read(source: string): Promise<BridgeResult<unknown>>;
  list(prefix: string): Promise<BridgeResult<string[]>>;
  writeAppState(id: string, changes: object): Promise<BridgeResult<unknown>>;
  writeSettings(changes: object): Promise<BridgeResult<unknown>>;
  onReport(listener: (report: SyncReport) => void): () => void;
};

export function syncBridge(): SyncBridge | null {
  return (globalThis as { neotime?: { sync?: SyncBridge } }).neotime?.sync ?? null;
}

function valueOf<Value>(result: BridgeResult<Value>): Value {
  if (!result.ok) {
    throw new Error(result.error ?? 'The desktop app could not reach its sync.');
  }
  return result.value as Value;
}

// Hands the companion's interval to the core, which keeps its own copy for when the window is closed.
export async function shareSyncInterval(bridge: SyncBridge, seconds: number): Promise<void> {
  const current = valueOf(await bridge.settings());
  if (current.intervalSeconds !== seconds) {
    valueOf(await bridge.setSettings({ intervalSeconds: seconds }));
  }
}
