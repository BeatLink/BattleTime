// What the window keeps between runs, in the web view's local storage under the same keys the phone app used.

import type { KeyValueStorage } from '@/services/app-store';

// Local storage behind the promise-based interface the settings, backups and store read through.
export function localKeyValueStorage(storage: Storage | null = typeof localStorage === 'undefined' ? null : localStorage): KeyValueStorage {
  const store = () => {
    if (!storage) {
      throw new Error('This window has no local storage.');
    }
    return storage;
  };
  return {
    async getItem(key) {
      return store().getItem(key);
    },
    async setItem(key, value) {
      store().setItem(key, value);
    },
    async removeItem(key) {
      store().removeItem(key);
    },
  };
}

export const appStorage: KeyValueStorage = localKeyValueStorage();
