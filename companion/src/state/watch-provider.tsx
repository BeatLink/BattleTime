import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';

import { createTransport, transportKind } from '@/ble/create-transport';
import type { ConnectionState, DiscoveredWatch, WatchTransport } from '@/ble/transport';
import {
  decodeWatchLine,
  encodeForWatch,
  isFindPhoneOn,
  LineBuffer,
  type PhoneToWatchMessage,
  type WatchToPhoneMessage,
} from '@/protocol/gadgetbridge';
import { defaultSettings, loadSettings, saveSettings, type Settings } from '@/state/settings';

export type ConsoleEntry = {
  id: number;
  at: number;
  direction: 'in' | 'out';
  text: string;
};

const CONSOLE_LIMIT = 200;

type WatchContextValue = {
  transportKind: 'ble' | 'mock';
  connection: ConnectionState;
  watch: DiscoveredWatch | null;
  scanning: boolean;
  found: DiscoveredWatch[];
  console: ConsoleEntry[];
  lastError: string | null;
  findPhoneActive: boolean;
  settings: Settings;
  settingsLoaded: boolean;
  startScan: () => Promise<void>;
  stopScan: () => Promise<void>;
  connect: (watch: DiscoveredWatch) => Promise<void>;
  disconnect: () => Promise<void>;
  send: (message: PhoneToWatchMessage) => Promise<void>;
  sendRaw: (text: string) => Promise<void>;
  updateSettings: (patch: Partial<Settings>) => Promise<void>;
  clearConsole: () => void;
};

const WatchContext = createContext<WatchContextValue | null>(null);

// Owns the single transport for the app and exposes connection, traffic and settings to every screen.
export function WatchProvider({ children }: { children: ReactNode }) {
  const transportRef = useRef<WatchTransport | null>(null);
  const bufferRef = useRef(new LineBuffer());
  const nextId = useRef(1);

  const [connection, setConnection] = useState<ConnectionState>('disconnected');
  const [watch, setWatch] = useState<DiscoveredWatch | null>(null);
  const [scanning, setScanning] = useState(false);
  const [found, setFound] = useState<DiscoveredWatch[]>([]);
  const [consoleEntries, setConsoleEntries] = useState<ConsoleEntry[]>([]);
  const [lastError, setLastError] = useState<string | null>(null);
  const [findPhoneActive, setFindPhoneActive] = useState(false);
  const [settings, setSettings] = useState<Settings>(defaultSettings);
  const [settingsLoaded, setSettingsLoaded] = useState(false);

  const log = useCallback((direction: 'in' | 'out', text: string) => {
    setConsoleEntries((entries) => {
      const entry = { id: nextId.current++, at: Date.now(), direction, text };
      return [...entries, entry].slice(-CONSOLE_LIMIT);
    });
  }, []);

  const handleMessage = useCallback((message: WatchToPhoneMessage) => {
    switch (message.t) {
      case 'findPhone':
        setFindPhoneActive(isFindPhoneOn(message));
        break;
      case 'music':
        // Media control from the watch needs a platform media session, which is not wired up yet.
        break;
      case 'error':
        setLastError(message.msg);
        break;
      case 'info':
        break;
    }
  }, []);

  useEffect(() => {
    const transport = createTransport();
    transportRef.current = transport;
    transport.setListener({
      onState: (state) => {
        setConnection(state);
        if (state === 'disconnected') {
          bufferRef.current.reset();
        }
      },
      onLine: (chunk) => {
        for (const line of bufferRef.current.push(chunk)) {
          log('in', line);
          const message = decodeWatchLine(line);
          if (message) {
            handleMessage(message);
          }
        }
      },
      onError: (error) => setLastError(error.message),
    });

    loadSettings().then((loaded) => {
      setSettings(loaded);
      setSettingsLoaded(true);
    });

    return () => {
      transport.destroy();
      transportRef.current = null;
    };
  }, [handleMessage, log]);

  const startScan = useCallback(async () => {
    setFound([]);
    setScanning(true);
    setLastError(null);
    try {
      await transportRef.current?.startScan((candidate) => {
        setFound((list) => (list.some((w) => w.id === candidate.id) ? list : [...list, candidate]));
      });
    } catch (error) {
      setScanning(false);
      setLastError((error as Error).message);
    }
  }, []);

  const stopScan = useCallback(async () => {
    await transportRef.current?.stopScan();
    setScanning(false);
  }, []);

  const updateSettings = useCallback(async (patch: Partial<Settings>) => {
    setSettings((current) => {
      const next = { ...current, ...patch };
      void saveSettings(next);
      return next;
    });
  }, []);

  const connect = useCallback(
    async (target: DiscoveredWatch) => {
      await stopScan();
      setLastError(null);
      setWatch(target);
      try {
        await transportRef.current?.connect(target.id);
        await updateSettings({ lastWatchId: target.id, lastWatchName: target.name });
      } catch (error) {
        setLastError((error as Error).message);
      }
    },
    [stopScan, updateSettings],
  );

  const disconnect = useCallback(async () => {
    await transportRef.current?.disconnect();
  }, []);

  const sendRaw = useCallback(
    async (text: string) => {
      log('out', text.trimEnd());
      try {
        await transportRef.current?.write(text);
      } catch (error) {
        setLastError((error as Error).message);
      }
    },
    [log],
  );

  const send = useCallback((message: PhoneToWatchMessage) => sendRaw(encodeForWatch(message)), [sendRaw]);

  const clearConsole = useCallback(() => setConsoleEntries([]), []);

  const value = useMemo<WatchContextValue>(
    () => ({
      transportKind,
      connection,
      watch,
      scanning,
      found,
      console: consoleEntries,
      lastError,
      findPhoneActive,
      settings,
      settingsLoaded,
      startScan,
      stopScan,
      connect,
      disconnect,
      send,
      sendRaw,
      updateSettings,
      clearConsole,
    }),
    [
      connection,
      watch,
      scanning,
      found,
      consoleEntries,
      lastError,
      findPhoneActive,
      settings,
      settingsLoaded,
      startScan,
      stopScan,
      connect,
      disconnect,
      send,
      sendRaw,
      updateSettings,
      clearConsole,
    ],
  );

  return <WatchContext.Provider value={value}>{children}</WatchContext.Provider>;
}

export function useWatch(): WatchContextValue {
  const value = useContext(WatchContext);
  if (!value) {
    throw new Error('useWatch must be used inside WatchProvider');
  }
  return value;
}
