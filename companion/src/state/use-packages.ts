// Drives the package manager from the UI: reads what the watch has, installs,
// removes and toggles, and keeps one operation in flight at a time.

import { useCallback, useEffect, useState } from 'react';

import { buildCatalog, decodeBundle, strayPackages, type CatalogEntry } from '@/packages/catalog';
import type { AbiInfo, InstalledPackage } from '@/protocol/packages';
import {
  installPackage,
  listPackages,
  readAbi,
  setEnabled as setEnabledOnWatch,
  uninstallPackage,
  writeConfig,
  type PackageChannel,
  type Progress,
} from '@/protocol/transfer';

export type PackageState = {
  ready: boolean;
  busy: string | null;
  progress: Progress | null;
  error: string | null;
  abi: AbiInfo | null;
  catalog: CatalogEntry[];
  strays: InstalledPackage[];
  refresh: () => Promise<void>;
  install: (entry: CatalogEntry) => Promise<void>;
  uninstall: (name: string) => Promise<void>;
  toggle: (name: string, enabled: boolean) => Promise<void>;
  configure: (name: string, values: Record<string, unknown>) => Promise<void>;
};

export function usePackages(channel: PackageChannel, connected: boolean): PackageState {
  const [ready, setReady] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);
  const [progress, setProgress] = useState<Progress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [abi, setAbi] = useState<AbiInfo | null>(null);
  const [installed, setInstalled] = useState<InstalledPackage[]>([]);

  const refresh = useCallback(async () => {
    if (!connected) {
      return;
    }
    setBusy('Reading the watch');
    setError(null);
    try {
      const watchAbi = await readAbi(channel);
      setAbi(watchAbi);
      setInstalled(await listPackages(channel));
      setReady(true);
    } catch (caught) {
      setError((caught as Error).message);
    } finally {
      setBusy(null);
    }
  }, [channel, connected]);

  // Read the watch once it connects. State is only touched after the first
  // await, and a watch that goes away mid-read cancels the rest.
  useEffect(() => {
    if (!connected) {
      return;
    }
    let cancelled = false;

    (async () => {
      try {
        const watchAbi = await readAbi(channel);
        if (cancelled) return;
        setAbi(watchAbi);

        const list = await listPackages(channel);
        if (cancelled) return;
        setInstalled(list);
        setReady(true);
      } catch (caught) {
        if (!cancelled) {
          setError((caught as Error).message);
        }
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [channel, connected]);

  const run = useCallback(
    async (label: string, action: () => Promise<void>) => {
      setBusy(label);
      setError(null);
      try {
        await action();
        setInstalled(await listPackages(channel));
      } catch (caught) {
        setError((caught as Error).message);
      } finally {
        setBusy(null);
        setProgress(null);
      }
    },
    [channel],
  );

  const install = useCallback(
    (entry: CatalogEntry) =>
      run(`Installing ${entry.label}`, async () => {
        await installPackage(channel, decodeBundle(entry.bundle), {
          abi,
          onProgress: setProgress,
        });
      }),
    [abi, channel, run],
  );

  const uninstall = useCallback(
    (name: string) => run(`Removing ${name}`, () => uninstallPackage(channel, name)),
    [channel, run],
  );

  const toggle = useCallback(
    (name: string, enabled: boolean) =>
      run(enabled ? `Enabling ${name}` : `Disabling ${name}`, () =>
        setEnabledOnWatch(channel, name, enabled),
      ),
    [channel, run],
  );

  const configure = useCallback(
    (name: string, values: Record<string, unknown>) =>
      run(`Configuring ${name}`, () => writeConfig(channel, name, values)),
    [channel, run],
  );

  // Everything the watch told us stops being true the moment it disconnects.
  const liveAbi = connected ? abi : null;
  const liveInstalled = connected ? installed : [];

  return {
    ready: connected && ready,
    busy,
    progress,
    error,
    abi: liveAbi,
    catalog: buildCatalog(liveInstalled, liveAbi),
    strays: strayPackages(liveInstalled),
    refresh,
    install,
    uninstall,
    toggle,
    configure,
  };
}
