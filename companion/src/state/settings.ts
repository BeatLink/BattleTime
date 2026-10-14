import type { NoteFont, NoteOnWatch } from '@/protocol/notes';
import { readRemoteButtons, type RemoteButton, type RemoteOnWatch } from '@/protocol/remote';
import type { NoteFileRef } from '@/services/note-file';
import { NOTE_SOURCES, type NoteSource } from '@/services/note-source';
import { readWebAccess, type WebAccess } from '@/services/web-request';
import { DEFAULT_SYNC_INTERVAL_SECONDS } from '@/services/sync';
import type { Place } from '@/services/weather';
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
  // How often everything on the watch is read again while it is connected.
  syncIntervalSeconds: number;
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
  githubRepo: 'BeatLink/NeoTime',
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
  syncIntervalSeconds: DEFAULT_SYNC_INTERVAL_SECONDS,
};

const STORAGE_KEY = 'neotime.settings.v1';

export async function loadSettings(storage: KeyValueStorage = appStorage): Promise<Settings> {
  try {
    const raw = await storage.getItem(STORAGE_KEY);
    if (!raw) {
      return defaultSettings;
    }
    const parsed = JSON.parse(raw);
    const stored = { ...defaultSettings, ...parsed };
    // Settings saved before there was a choice of source kept only a file.
    if (!(NOTE_SOURCES as readonly unknown[]).includes(parsed.noteSource)) {
      stored.noteSource = stored.noteFile ? 'file' : 'typed';
    }
    // Buttons are checked one by one, so a malformed one never reaches the watch or runs.
    return {
      ...stored,
      noteUrlAccess: readWebAccess(stored.noteUrlAccess),
      remoteButtons: readRemoteButtons(stored.remoteButtons),
    };
  } catch {
    return defaultSettings;
  }
}

export async function saveSettings(settings: Settings, storage: KeyValueStorage = appStorage): Promise<void> {
  await storage.setItem(STORAGE_KEY, JSON.stringify(settings));
}
