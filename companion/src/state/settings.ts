import AsyncStorage from '@react-native-async-storage/async-storage';

import type { Place } from '@/services/weather';

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
  // Whether watch apps may fetch URLs through the phone, and from which hosts.
  httpEnabled: boolean;
  httpAllowedHosts: string;
  // Where firmware builds are listed from, and the token GitHub wants for Actions builds.
  githubRepo: string;
  githubToken: string;
  firmwareBoard: string;
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
  httpEnabled: false,
  httpAllowedHosts: '',
  githubRepo: 'wasp-os/wasp-os',
  githubToken: '',
  firmwareBoard: 'pinetime',
};

const STORAGE_KEY = 'waspos.settings.v1';

export async function loadSettings(): Promise<Settings> {
  try {
    const raw = await AsyncStorage.getItem(STORAGE_KEY);
    return raw ? { ...defaultSettings, ...JSON.parse(raw) } : defaultSettings;
  } catch {
    return defaultSettings;
  }
}

export async function saveSettings(settings: Settings): Promise<void> {
  await AsyncStorage.setItem(STORAGE_KEY, JSON.stringify(settings));
}
