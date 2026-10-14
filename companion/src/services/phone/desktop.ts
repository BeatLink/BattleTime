// The desktop app's stand-in for a phone: desktop notifications, media players and Do Not Disturb from the computer.
//
// tauri/src/desktop.rs reads them over D-Bus and passes raw facts through window.neotime.desktop;
// this file turns those into the same events the Android bridge sends.

import type { DeviceLocation, MediaCommand, PhoneBridge, PhoneEvent } from './types';

// ============================================================================
// What the bridge carries
// ============================================================================

type BridgeResult = { ok: boolean; error?: string; value?: unknown };

export type RawNotify = {
  kind: 'notify';
  sender: string;
  serial: number;
  appName: string;
  replacesId: number;
  summary: string;
  body: string;
  hints: Record<string, unknown>;
};

export type RawNotifyReply = { kind: 'notify-reply'; destination: string; replySerial: number; id: number };

export type RawClosed = { kind: 'closed'; id: number };

export type RawPlayer = {
  name: string;
  status: string;
  metadata: Record<string, unknown>;
  position: number | null;
  volume: number | null;
};

export type RawPlayers = { kind: 'players'; players: RawPlayer[] };

export type RawDesktopEvent = RawNotify | RawNotifyReply | RawClosed | RawPlayers;

export type DesktopDeviceBridge = {
  hasNotificationAccess(): Promise<BridgeResult>;
  isDoNotDisturb(): Promise<BridgeResult>;
  mediaCommand(player: string, command: MediaCommand): Promise<BridgeResult>;
  locate(): Promise<BridgeResult>;
  onEvent(listener: (event: RawDesktopEvent) => void): () => void;
};

export function desktopDeviceBridge(): DesktopDeviceBridge | null {
  return (globalThis as { neotime?: { desktop?: DesktopDeviceBridge } }).neotime?.desktop ?? null;
}

async function unwrap<T>(pending: Promise<BridgeResult>): Promise<T> {
  const result = await pending;
  if (!result.ok) {
    throw new Error(result.error ?? 'The desktop bridge failed.');
  }
  return result.value as T;
}

// ============================================================================
// Notifications
// ============================================================================

// The desktop app's own notifications never go back to the watch.
const OWN_APP_NAMES = new Set(['NeoTime Companion', 'neotime-companion']);

// Notifications waiting for the server to say which id it gave them; old ones are dropped past this many.
const PENDING_LIMIT = 100;

const ENTITIES: Record<string, string> = { amp: '&', lt: '<', gt: '>', quot: '"', apos: "'" };

// Desktop notification bodies may hold simple markup, which the watch would show as text.
export function stripMarkup(text: string): string {
  return text
    .replace(/<[^>]*>/g, '')
    .replace(/&(amp|lt|gt|quot|apos);/g, (_match, name: string) => ENTITIES[name])
    .trim();
}

function notificationEvent(id: number, raw: RawNotify): PhoneEvent {
  const desktopEntry = typeof raw.hints['desktop-entry'] === 'string' ? (raw.hints['desktop-entry'] as string) : '';
  return {
    type: 'notification',
    id,
    app: raw.appName || desktopEntry,
    title: stripMarkup(raw.summary),
    body: stripMarkup(raw.body),
  };
}

// ============================================================================
// Media players
// ============================================================================

const MICROSECONDS_PER_SECOND = 1_000_000;

function text(value: unknown): string | undefined {
  if (Array.isArray(value)) {
    const joined = value.filter((item) => typeof item === 'string').join(', ');
    return joined || undefined;
  }
  return typeof value === 'string' && value !== '' ? value : undefined;
}

function seconds(microseconds: unknown): number | undefined {
  return typeof microseconds === 'number' && Number.isFinite(microseconds) && microseconds >= 0
    ? Math.round(microseconds / MICROSECONDS_PER_SECOND)
    : undefined;
}

// Prefers a player that is playing, then the one already followed, then any that is paused.
export function choosePlayer(players: RawPlayer[], current: string | null): RawPlayer | null {
  return (
    players.find((player) => player.status === 'Playing') ??
    players.find((player) => player.name === current && player.status !== 'Stopped') ??
    players.find((player) => player.status === 'Paused') ??
    null
  );
}

function mediaEvent(player: RawPlayer | null): Extract<PhoneEvent, { type: 'media' }> {
  if (!player) {
    return { type: 'media', playing: false };
  }
  const fields = {
    artist: text(player.metadata['xesam:artist']),
    album: text(player.metadata['xesam:album']),
    track: text(player.metadata['xesam:title']),
    durationSec: seconds(player.metadata['mpris:length']),
    positionSec: seconds(player.position),
  };
  const present = Object.fromEntries(Object.entries(fields).filter(([, value]) => value !== undefined));
  return { type: 'media', playing: player.status === 'Playing', ...present };
}

// ============================================================================
// Turning raw facts into phone events
// ============================================================================

// Remembers just enough to pair a notification with the id the server gave it, and to send media only when it changes.
export class DesktopEventTracker {
  private readonly pending = new Map<string, RawNotify>();
  private lastMedia: Extract<PhoneEvent, { type: 'media' }> | null = null;
  private lastMediaKey = '';
  currentPlayer: string | null = null;

  constructor(private readonly emit: (event: PhoneEvent) => void) {}

  // The last media state, so a late listener need not wait for the next change.
  get media() {
    return this.lastMedia;
  }

  handle(raw: RawDesktopEvent) {
    switch (raw.kind) {
      case 'notify':
        this.onNotify(raw);
        break;
      case 'notify-reply':
        this.onNotifyReply(raw);
        break;
      case 'closed':
        this.emit({ type: 'notificationRemoved', id: raw.id });
        break;
      case 'players':
        this.onPlayers(raw.players);
        break;
    }
  }

  private onNotify(raw: RawNotify) {
    if (OWN_APP_NAMES.has(raw.appName)) {
      return;
    }
    // A replacement keeps the id of the notification it replaces, so it needs no answer from the server.
    if (raw.replacesId > 0) {
      this.emit(notificationEvent(raw.replacesId, raw));
      return;
    }
    this.pending.set(`${raw.sender}/${raw.serial}`, raw);
    if (this.pending.size > PENDING_LIMIT) {
      const oldest = this.pending.keys().next().value as string;
      this.pending.delete(oldest);
    }
  }

  private onNotifyReply(reply: RawNotifyReply) {
    const key = `${reply.destination}/${reply.replySerial}`;
    const raw = this.pending.get(key);
    if (!raw) {
      return;
    }
    this.pending.delete(key);
    this.emit(notificationEvent(reply.id, raw));
  }

  private onPlayers(players: RawPlayer[]) {
    const player = choosePlayer(players, this.currentPlayer);
    this.currentPlayer = player?.name ?? null;
    const media = mediaEvent(player);
    const key = JSON.stringify(media);
    if (key === this.lastMediaKey) {
      return;
    }
    this.lastMediaKey = key;
    this.lastMedia = media;
    this.emit(media);
  }
}

// ============================================================================
// The bridge
// ============================================================================

const noCalls = () => Promise.reject(new Error('A computer has no calls to answer'));

export function createDesktopPhoneBridge(device: DesktopDeviceBridge): PhoneBridge {
  const listeners = new Set<(event: PhoneEvent) => void>();
  const tracker = new DesktopEventTracker((event) => {
    for (const listener of listeners) {
      listener(event);
    }
  });
  device.onEvent((raw) => tracker.handle(raw));

  return {
    available: true,
    kind: 'computer',
    supportsCalls: false,
    supportsLocation: true,
    hasNotificationAccess: () => unwrap<boolean>(device.hasNotificationAccess()),
    openNotificationAccessSettings: () =>
      Promise.reject(new Error('The desktop app reads notifications without any setting')),
    requestCallPermissions: async () => false,
    isDoNotDisturb: () => unwrap<boolean>(device.isDoNotDisturb()),
    subscribe(listener) {
      listeners.add(listener);
      if (tracker.media) {
        listener(tracker.media);
      }
      return () => listeners.delete(listener);
    },
    answerCall: noCalls,
    rejectCall: noCalls,
    silenceCall: noCalls,
    async mediaCommand(command) {
      if (!tracker.currentPlayer) {
        throw new Error('No media player is running.');
      }
      await unwrap(device.mediaCommand(tracker.currentPlayer, command));
    },
    // A desktop app keeps running in the background anyway.
    startKeepAlive: async () => {},
    stopKeepAlive: async () => {},
    async locate(): Promise<DeviceLocation> {
      const found = await unwrap<{ latitude: number; longitude: number; description: string }>(device.locate());
      return { latitude: found.latitude, longitude: found.longitude, name: found.description || 'Here' };
    },
  };
}
