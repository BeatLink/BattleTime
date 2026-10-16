// The companion app's socket, so the command line tool shares the app's link to the watch instead of competing for it in BlueZ.
//
// Each line is one JSON object: requests {id, command, args} get {id, ok, value} or {id, ok: false, error},
// and the app's events arrive as {event, payload}. core/src/daemon/socket.rs answers it.

import { createConnection, type Socket } from 'node:net';
import { homedir } from 'node:os';
import { join } from 'node:path';

import type { WriteMode } from '@/dfu/link';

import type { WatchBleEvent } from '../node/ble';

export const SOCKET_NAME = 'battletime-companion.sock';

// The app's data folder, which holds the socket when the session has no runtime folder.
const APP_IDENTIFIER = 'com.battletime.companion';

// How long the app has to answer before the tool goes to BlueZ itself.
const ANSWER_TIMEOUT_MS = 1500;

export type Emit = <K extends keyof WatchBleEvent>(name: K, payload: WatchBleEvent[K]) => void;

// ============================================================================
// Where the socket is
// ============================================================================

export function socketPath(env: NodeJS.ProcessEnv = process.env): string {
  if (env.BATTLETIME_SOCKET) {
    return env.BATTLETIME_SOCKET;
  }
  if (env.XDG_RUNTIME_DIR) {
    return join(env.XDG_RUNTIME_DIR, SOCKET_NAME);
  }
  const data = env.XDG_DATA_HOME || join(homedir(), '.local', 'share');
  return join(data, APP_IDENTIFIER, SOCKET_NAME);
}

// ============================================================================
// The connection
// ============================================================================

type Pending = { resolve: (value: unknown) => void; reject: (error: Error) => void };

export class DaemonClient {
  private nextId = 1;
  private readonly pending = new Map<number, Pending>();
  private buffered = '';
  private onEvent: (event: string, payload: unknown) => void = () => undefined;

  private constructor(private readonly socket: Socket) {
    socket.setEncoding('utf8');
    socket.on('data', (chunk: string) => this.take(chunk));
    socket.on('close', () => this.failAll(new Error('The companion app closed its connection.')));
    socket.on('error', () => undefined);
  }

  // Connects and asks for the daemon's state, or gives null when no app answers in time.
  static async open(path: string, timeoutMs = ANSWER_TIMEOUT_MS): Promise<DaemonClient | null> {
    const socket = await new Promise<Socket | null>((resolve) => {
      const attempt = createConnection(path);
      attempt.once('connect', () => resolve(attempt));
      attempt.once('error', () => resolve(null));
    });
    if (!socket) {
      return null;
    }
    const client = new DaemonClient(socket);
    const answered = await Promise.race([
      client.call('daemon_state').then(
        () => true,
        () => false,
      ),
      new Promise<boolean>((resolve) => setTimeout(() => resolve(false), timeoutMs).unref()),
    ]);
    if (!answered) {
      client.close();
      return null;
    }
    return client;
  }

  listen(onEvent: (event: string, payload: unknown) => void) {
    this.onEvent = onEvent;
  }

  call<T = unknown>(command: string, args: Record<string, unknown> = {}): Promise<T> {
    const id = this.nextId++;
    return new Promise<T>((resolve, reject) => {
      this.pending.set(id, { resolve: resolve as (value: unknown) => void, reject });
      this.socket.write(`${JSON.stringify({ id, command, args })}\n`);
    });
  }

  close() {
    this.socket.end();
    this.socket.destroy();
  }

  private take(chunk: string) {
    this.buffered += chunk;
    let end = this.buffered.indexOf('\n');
    while (end >= 0) {
      const line = this.buffered.slice(0, end).trim();
      this.buffered = this.buffered.slice(end + 1);
      if (line) {
        this.handle(line);
      }
      end = this.buffered.indexOf('\n');
    }
  }

  private handle(line: string) {
    let message: { id?: number; ok?: boolean; value?: unknown; error?: string; event?: string; payload?: unknown };
    try {
      message = JSON.parse(line);
    } catch {
      return;
    }
    if (typeof message.event === 'string') {
      this.onEvent(message.event, message.payload);
      return;
    }
    const waiting = typeof message.id === 'number' ? this.pending.get(message.id) : undefined;
    if (!waiting) {
      return;
    }
    this.pending.delete(message.id!);
    if (message.ok) {
      waiting.resolve(message.value);
    } else {
      waiting.reject(new Error(message.error ?? 'The companion app refused.'));
    }
  }

  private failAll(error: Error) {
    for (const waiting of this.pending.values()) waiting.reject(error);
    this.pending.clear();
  }
}

// ============================================================================
// The watch through the app
// ============================================================================

type DaemonState = { connection?: string; mode?: string; watch?: { id: string; name: string } | null };

// The same calls node/ble.js offers, answered by the app's link to the watch.
export class DaemonLink {
  constructor(
    private readonly client: DaemonClient,
    private readonly emit: Emit,
  ) {
    client.listen((event, payload) => {
      if (event !== 'ble:event') {
        return;
      }
      const { name, payload: inner } = payload as { name: keyof WatchBleEvent; payload: never };
      this.emit(name, inner);
    });
  }

  startScan(): Promise<void> {
    return this.client.call('ble_start_scan');
  }

  stopScan(): Promise<void> {
    return this.client.call('ble_stop_scan');
  }

  // Uses the app's link when it already has this watch, and otherwise asks the app to connect.
  async connect(id: string, _uart = true, name = ''): Promise<void> {
    let state = await this.client.call<DaemonState>('daemon_state');
    const already = state.connection === 'connected' && state.watch?.id === id;
    if (!already) {
      await this.client.call('daemon_connect', { id, name: name || (state.watch?.id === id ? state.watch.name : '') });
      state = await this.client.call<DaemonState>('daemon_state');
    }
    this.emit('mode', state.mode === 'bootloader' ? 'bootloader' : 'application');
    this.emit('state', 'connected');
  }

  reconnect(id: string): Promise<void> {
    return this.client.call('ble_reconnect', { id });
  }

  write(text: string): Promise<void> {
    return this.client.call('ble_write', { text });
  }

  writeBytes(data: Uint8Array | number[]): Promise<void> {
    return this.client.call('ble_write_bytes', { data: Array.from(data) });
  }

  gattHas(service: string, characteristic: string): Promise<boolean> {
    return this.client.call('ble_gatt_has', { service, characteristic });
  }

  gattRead(service: string, characteristic: string): Promise<number[]> {
    return this.client.call('ble_gatt_read', { service, characteristic });
  }

  gattWrite(service: string, characteristic: string, data: number[], mode: WriteMode): Promise<void> {
    return this.client.call('ble_gatt_write', { service, characteristic, data, mode });
  }

  gattSubscribe(service: string, characteristic: string): Promise<void> {
    return this.client.call('ble_gatt_subscribe', { service, characteristic });
  }

  gattUnsubscribe(service: string, characteristic: string): Promise<void> {
    return this.client.call('ble_gatt_unsubscribe', { service, characteristic });
  }

  // The app shares the file service with its own sync, so the tool takes a turn for each operation.
  async exclusive<T>(work: () => Promise<T>): Promise<T> {
    const lease = await this.client.call<number>('files_begin');
    try {
      return await work();
    } finally {
      await this.client.call('files_end', { lease }).catch(() => undefined);
    }
  }

  // Holds the app back from reconnecting while a firmware update moves the link.
  hold(held: boolean): Promise<void> {
    return this.client.call('daemon_hold', { held });
  }

  // Leaves the watch connected to the app; only the tool's own connection closes.
  async destroy(): Promise<void> {
    this.client.close();
  }
}
