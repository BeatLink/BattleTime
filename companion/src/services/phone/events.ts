import type { PhoneEvent } from './types';

const CALL_STATES = ['incoming', 'outgoing', 'start', 'end'] as const;

type Raw = Record<string, unknown>;

const str = (raw: Raw, key: string): string | undefined =>
  typeof raw[key] === 'string' ? (raw[key] as string) : undefined;

const num = (raw: Raw, key: string): number | undefined =>
  typeof raw[key] === 'number' && Number.isFinite(raw[key]) ? (raw[key] as number) : undefined;

// Copies only the fields that are present, so optional ones stay absent rather than undefined.
function optional<T extends object>(fields: T): Partial<T> {
  return Object.fromEntries(Object.entries(fields).filter(([, value]) => value !== undefined)) as Partial<T>;
}

// Checks an event from the native module and returns it typed, or null if it is not one this app knows.
export function toPhoneEvent(value: unknown): PhoneEvent | null {
  if (!value || typeof value !== 'object') {
    return null;
  }
  const raw = value as Raw;
  switch (raw.type) {
    case 'notification': {
      const id = num(raw, 'id');
      if (id === undefined) return null;
      return {
        type: 'notification',
        id,
        app: str(raw, 'app') ?? '',
        title: str(raw, 'title') ?? '',
        body: str(raw, 'body') ?? '',
      };
    }
    case 'notificationRemoved': {
      const id = num(raw, 'id');
      return id === undefined ? null : { type: 'notificationRemoved', id };
    }
    case 'call': {
      const state = CALL_STATES.find((s) => s === raw.state);
      if (!state) return null;
      return { type: 'call', state, ...optional({ name: str(raw, 'name'), number: str(raw, 'number') }) };
    }
    case 'media':
      return {
        type: 'media',
        playing: raw.playing === true,
        ...optional({
          artist: str(raw, 'artist'),
          album: str(raw, 'album'),
          track: str(raw, 'track'),
          durationSec: num(raw, 'durationSec'),
          positionSec: num(raw, 'positionSec'),
        }),
      };
    default:
      return null;
  }
}
