// Runs one watch operation at a time for a screen and keeps what it is doing and how it failed.

import { useCallback, useRef, useState } from 'react';

export type Task = {
  busy: string | null;
  error: string | null;
  clearError: () => void;
  // Run an action under a label, returning its result, or undefined if it failed or another was running.
  run: <T>(label: string, action: () => Promise<T>) => Promise<T | undefined>;
};

export function useTask(): Task {
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const running = useRef(false);

  const run = useCallback(async <T,>(label: string, action: () => Promise<T>) => {
    if (running.current) {
      return undefined;
    }
    running.current = true;
    setBusy(label);
    setError(null);
    try {
      return await action();
    } catch (caught) {
      setError((caught as Error).message);
      return undefined;
    } finally {
      running.current = false;
      setBusy(null);
    }
  }, []);

  const clearError = useCallback(() => setError(null), []);

  return { busy, error, clearError, run };
}
