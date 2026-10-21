import type { NoteFont, NoteOnWatch } from '@/protocol/notes';
import { readRemoteButtons, type RemoteButton, type RemoteOnWatch } from '@/protocol/remote';
import type { NoteFileRef } from '@/services/note-file';
import { NOTE_SOURCES, type NoteSource } from '@/services/note-source';
import { readWebAccess, type WebAccess } from '@/services/web-request';
import { DEFAULT_SYNC_INTERVAL_SECONDS } from '@/services/sync';
import type { Place } from '@/services/weather';
import { DEFAULT_EMERGENCY_NUMBER, readSosContacts, type SosCall, type SosContact } from '@/state/sos';
import { appStorage } from '@/state/storage';
import type { KeyValueStorage } from '@/services/app-store';

// User settings that survive restarts, stored as one JSON blob.
export type Settings = {
  lastWatchId: string | null;
  lastWatchName: string | null;
  autoReconnect: boolean;
  forwardNotifications: boolean;
  forwardMusic: boolean;
  forwardWeather: boolean;
  forwardCalls: boolean;
  // Hold notifications back while the phone is in Do Not Disturb.
  respectDoNotDisturb: boolean;
  // The place weather reports are for, as the user typed it and as it was found.
  weatherQuery: string;
  weatherPlace: Place | null;
  // Report the weather where the device is, found by the computer's location service, instead of the place above.
  weatherFromDevice: boolean;
  // Whether watch apps may fetch URLs through the phone, and from which hosts.
  httpEnabled: boolean;
  httpAllowedHosts: string;
  // Where firmware builds are listed from, and the token GitHub wants for Actions builds.
  githubRepo: string;
  githubToken: string;
  firmwareBoard: string;
  // Where the watch's note comes from, each source's contents, the font it is drawn in, and what the watch was last sent.
  noteSource: NoteSource;
  noteText: string;
  noteFile: NoteFileRef | null;
  noteUrl: string;
  // The notes URL's headers, login and certificate choice.
  noteUrlAccess: WebAccess;
  noteFont: NoteFont;
  noteOnWatch: NoteOnWatch | null;
  // The watch's remote buttons, in order, and the layout the watch was last sent.
  remoteButtons: RemoteButton[];
  remoteOnWatch: RemoteOnWatch | null;
  // The SOS: whom it texts and calls, whether it calls them or the emergency number, and whether it texts and records.
  sosContacts: SosContact[];
  sosCall: SosCall;
  sosEmergencyNumber: string;
  sosTextLocation: boolean;
  sosRecord: boolean;
  // How often everything on the watch is read again while it is connected.
  syncIntervalSeconds: number;
  // On the desktop: keep the watch after the window closes, and start hidden with the computer.
  runInBackground: boolean;
  startWithComputer: boolean;
};

export const defaultSettings: Settings = {
  lastWatchId: null,
  lastWatchName: null,
  autoReconnect: true,
  forwardNotifications: true,
  forwardMusic: true,
  forwardWeather: false,
  forwardCalls: true,
  respectDoNotDisturb: true,
  weatherQuery: '',
  weatherPlace: null,
  weatherFromDevice: false,
  httpEnabled: false,
  httpAllowedHosts: '',
  githubRepo: 'BeatLink/BattleTime',
  githubToken: '',
  firmwareBoard: 'pinetime',
  noteSource: 'typed',
  noteText: '',
  noteFile: null,
  noteUrl: '',
  noteUrlAccess: {},
  noteFont: 'normal',
  noteOnWatch: null,
  remoteButtons: [],
  remoteOnWatch: null,
  sosContacts: [],
  sosCall: 'contacts',
  sosEmergencyNumber: DEFAULT_EMERGENCY_NUMBER,
  sosTextLocation: true,
  sosRecord: true,
  syncIntervalSeconds: DEFAULT_SYNC_INTERVAL_SECONDS,
  runInBackground: true,
  startWithComputer: false,
};

const STORAGE_KEY = 'battletime.settings.v1';

// Settings as stored, with every key checked and the defaults filled in.
export function readSettings(parsed: unknown): Settings {
  if (!parsed || typeof parsed !== 'object') {
    return defaultSettings;
  }
  const raw = parsed as Partial<Settings>;
  const stored = { ...defaultSettings, ...raw };
  // Settings saved before there was a choice of source kept only a file.
  if (!(NOTE_SOURCES as readonly unknown[]).includes(raw.noteSource)) {
    stored.noteSource = stored.noteFile ? 'file' : 'typed';
  }
  // Buttons are checked one by one, so a malformed one never reaches the watch or runs.
  return {
    ...stored,
    noteUrlAccess: readWebAccess(stored.noteUrlAccess),
    remoteButtons: readRemoteButtons(stored.remoteButtons),
    sosContacts: readSosContacts(stored.sosContacts),
    sosCall: stored.sosCall === 'emergency' ? 'emergency' : 'contacts',
  };
}

export async function loadSettings(storage: KeyValueStorage = appStorage): Promise<Settings> {
  try {
    const raw = await storage.getItem(STORAGE_KEY);
    return raw ? readSettings(JSON.parse(raw)) : defaultSettings;
  } catch {
    return defaultSettings;
  }
}

export async function saveSettings(settings: Settings, storage: KeyValueStorage = appStorage): Promise<void> {
  await storage.setItem(STORAGE_KEY, JSON.stringify(settings));
}
