// Plugins: the phone-side halves of watch apps, which the desktop app installs and runs in its Rust core.
//
// The window lists and manages them through `window.neotime.plugins` (tauri/src/bridge.js), installs
// the watch apps each one carries, and shows each one's settings page in a sandboxed frame that talks
// to the plugin through messages. docs/plugins.md describes plugins in full.

// ============================================================================
// What the desktop bridge carries
// ============================================================================

export type PluginManifest = {
  id: string;
  name: string;
  version: string;
  description: string;
  watch: string[];
  main?: string | null;
  ui?: string | null;
  permissions: { messages: string[]; hosts: string[]; data?: string[] };
};

export type PluginStatus = { manifest: PluginManifest; enabled: boolean; running: boolean };

export type PluginLogLine = { id: string; line: string };

export type PluginUiMessage = { id: string; message: unknown };

type BridgeResult<Value> = { ok: boolean; error?: string; value?: Value };

export type PluginBridge = {
  list(): Promise<BridgeResult<PluginStatus[]>>;
  installPick(): Promise<BridgeResult<PluginStatus | null>>;
  install(data: number[]): Promise<BridgeResult<PluginStatus>>;
  remove(id: string): Promise<BridgeResult<void>>;
  setEnabled(id: string, enabled: boolean): Promise<BridgeResult<void>>;
  watchApps(id: string): Promise<BridgeResult<{ name: string; data: number[] }[]>>;
  uiMessage(id: string, message: unknown): Promise<BridgeResult<void>>;
  onUi(listener: (event: PluginUiMessage) => void): () => void;
  onLog(listener: (event: PluginLogLine) => void): () => void;
};

export function pluginBridge(): PluginBridge | null {
  return (globalThis as { neotime?: { plugins?: PluginBridge } }).neotime?.plugins ?? null;
}

function valueOf<Value>(result: BridgeResult<Value>): Value {
  if (!result.ok) {
    throw new Error(result.error ?? 'The desktop app could not reach its plugins.');
  }
  return result.value as Value;
}

// ============================================================================
// Managing plugins
// ============================================================================

// The bridge's calls with their results unwrapped, so a refusal arrives as a thrown error.
export class Plugins {
  constructor(private readonly bridge: PluginBridge) {}

  async list(): Promise<PluginStatus[]> {
    return valueOf(await this.bridge.list());
  }

  // Lets the user choose a .ntplugin file; null when they cancel.
  async installPick(): Promise<PluginStatus | null> {
    return valueOf(await this.bridge.installPick());
  }

  // Installs a .ntplugin file's contents, replacing any plugin with the same id.
  async install(data: Uint8Array): Promise<PluginStatus> {
    return valueOf(await this.bridge.install(Array.from(data)));
  }

  async remove(id: string): Promise<void> {
    valueOf(await this.bridge.remove(id));
  }

  async setEnabled(id: string, enabled: boolean): Promise<void> {
    valueOf(await this.bridge.setEnabled(id, enabled));
  }

  async watchAppImages(id: string): Promise<Uint8Array[]> {
    return valueOf(await this.bridge.watchApps(id)).map((app) => Uint8Array.from(app.data));
  }

  async uiMessage(id: string, message: unknown): Promise<void> {
    valueOf(await this.bridge.uiMessage(id, message));
  }
}

// Installs every watch app a plugin carries, one after another, and says how many went on.
export async function installPluginWatchApps(
  plugins: Plugins,
  id: string,
  install: (image: Uint8Array) => Promise<unknown>,
): Promise<number> {
  const images = await plugins.watchAppImages(id);
  for (const image of images) {
    await install(image);
  }
  return images.length;
}

// What a plugin may do, in words, for the user to read before relying on it.
export function describePermissions(manifest: PluginManifest): string[] {
  const words: Record<string, string> = {
    notify: 'Show notifications on the watch',
    call: 'Show calls on the watch',
    musicinfo: 'Show what is playing on the watch',
    musicstate: 'Show whether music is playing on the watch',
    weather: 'Send the weather to the watch',
    find: 'Ring the watch',
    vibrate: 'Buzz the watch',
  };
  const data: Record<string, string> = {
    settings: "Read and change the watch's settings",
    steps: "Read the watch's step history",
    logs: "Read the watch's logs",
    files: "Read the watch's settings files",
    apps: "Read every app's saved state",
  };
  return [
    ...manifest.permissions.messages.map((message) => words[message] ?? `Send ${message} messages to the watch`),
    ...manifest.permissions.hosts.map((host) => `Fetch from ${host}`),
    ...(manifest.permissions.data ?? []).map((kind) => data[kind] ?? `Read the watch's ${kind}`),
  ];
}

export function pluginStateText(status: PluginStatus): string {
  if (!status.enabled) {
    return 'Off';
  }
  if (!status.manifest.main) {
    return 'On, with no logic of its own';
  }
  return status.running ? 'Running' : 'Stopped by an error; see its log';
}

// ============================================================================
// The settings page
// ============================================================================

// The address the desktop app serves a plugin's settings page from.
export function pluginPageUrl(id: string): string {
  return `plugin://${encodeURIComponent(id)}/`;
}

// The page and the window wrap each message as { neotime: message }, so other messages are ignored.
export function messageFromPage(data: unknown): { message: unknown } | null {
  if (data && typeof data === 'object' && 'neotime' in data) {
    return { message: (data as { neotime: unknown }).neotime };
  }
  return null;
}

export function messageToPage(message: unknown): { neotime: unknown } {
  return { neotime: message };
}

// Keeps the newest log lines, at most `limit` of them.
export function appendLog(lines: PluginLogLine[], line: PluginLogLine, limit = 50): PluginLogLine[] {
  return [...lines, line].slice(-limit);
}
