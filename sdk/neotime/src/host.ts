// The browser stand-in for the watch: system values, storage and the effects apps ask for.

import { signal } from "@preact/signals";

// ============================================================================
// Effects
// ============================================================================

/** One effect an app asked for, such as `{ kind: "vibrate", milliseconds: 200 }`. */
export interface Effect {
  kind: string;
  [field: string]: unknown;
}

type EffectListener = (effect: Effect) => void;

const effect_listeners: EffectListener[] = [];

export function emit(effect: Effect) {
  for (const listener of effect_listeners) {
    listener(effect);
  }
}

export function listen_for_effects(listener: EffectListener): () => void {
  effect_listeners.push(listener);
  return () => {
    const index = effect_listeners.indexOf(listener);
    if (index >= 0) effect_listeners.splice(index, 1);
  };
}

// ============================================================================
// Clock
// ============================================================================

export const clock = {
  frozen_at: null as number | null,
};

export const now_milliseconds = signal(Date.now());

let clock_timer: ReturnType<typeof setInterval> | null = null;

export function start_clock() {
  if (clock_timer !== null) return;
  clock_timer = setInterval(() => {
    if (clock.frozen_at === null) now_milliseconds.value = Date.now();
  }, 50);
}

export function stop_clock() {
  if (clock_timer !== null) clearInterval(clock_timer);
  clock_timer = null;
}

// ============================================================================
// Storage
// ============================================================================

const STORAGE_PREFIX = "neotime:";

export const storage = {
  app_id: "unknown",
};

export function read_stored<Value>(key: string): Value | undefined {
  try {
    const text = localStorage.getItem(`${STORAGE_PREFIX}${storage.app_id}:${key}`);
    return text === null ? undefined : (JSON.parse(text) as Value);
  } catch {
    return undefined;
  }
}

export function write_stored(key: string, value: unknown) {
  try {
    localStorage.setItem(`${STORAGE_PREFIX}${storage.app_id}:${key}`, JSON.stringify(value));
  } catch {
    // Storage can be missing in private windows, and the preview still works without it.
  }
}

export function clear_storage() {
  try {
    for (const key of Object.keys(localStorage)) {
      if (key.startsWith(STORAGE_PREFIX)) localStorage.removeItem(key);
    }
  } catch {
    // Nothing was stored.
  }
}
