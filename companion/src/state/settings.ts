import AsyncStorage from '@react-native-async-storage/async-storage';

// User settings that survive restarts, stored as one JSON blob.
export type Settings = {
  lastWatchId: string | null;
  lastWatchName: string | null;
  autoReconnect: boolean;
  forwardNotifications: boolean;
  forwardMusic: boolean;
  forwardWeather: boolean;
};

export const defaultSettings: Settings = {
  lastWatchId: null,
  lastWatchName: null,
  autoReconnect: true,
  forwardNotifications: true,
  forwardMusic: true,
  forwardWeather: false,
};

const STORAGE_KEY = 'neotime.settings.v1';

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
