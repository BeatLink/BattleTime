import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';

import { createTransport, detectTransportKind } from '@/ble/create-transport';
import type {
  ConnectionState,
  DiscoveredWatch,
  TransportKind,
  WatchMode,
  WatchTransport,
} from '@/ble/transport';
import {
  decodeWatchLine,
  encodeForWatch,
  isFindPhoneOn,
  LineBuffer,
  type PhoneToWatchMessage,
  type WatchToPhoneMessage,
} from '@/protocol/gadgetbridge';
import type { DfuLink } from '@/dfu/link';
import { isPackageReply, type PackageReply } from '@/protocol/packages';
import { encodeSetTime } from '@/protocol/clock';
import { ReplyRouter, type RequestChannel } from '@/protocol/requests';
import type { PackageChannel } from '@/protocol/transfer';
import { answerHttpRequest, parseHostList } from '@/services/http-proxy';
import { phoneBridge, type PhoneEvent } from '@/services/phone';
import { currentWeather } from '@/services/weather';
import { ReplyQueue } from '@/state/reply-queue';
import { defaultSettings, loadSettings, saveSettings, type Settings } from '@/state/settings';

export type ConsoleEntry = {
  id: number;
  at: number;
  direction: 'in' | 'out';
  text: string;
};

const CONSOLE_LIMIT = 200;

type WatchContextValue = {
  transportKind: TransportKind;
  connection: ConnectionState;
  // What the connected watch is running. A watch in its bootloader answers
  // nothing but DFU, so most of the app has nothing to offer it.
  watchMode: WatchMode;
  // True only for a watch that is connected and running its firmware, which
  // is what every feature but the firmware update needs.
  watchReady: boolean;
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
  // Channel for the package manager commands, which need their replies back.
  packageChannel: PackageChannel;
  // Raw GATT for a firmware update, or null where the transport has no watch
  // or cannot reach past the UART service.
  dfuLink: () => DfuLink | null;
  // Channel for the Gadgetbridge messages the watch answers: settings, alarms and steps.
  requests: RequestChannel;
  // Fetch the weather for the chosen place and send it now.
  sendWeather: () => Promise<void>;
};

// Longest notification text sent, since the watch keeps notifications in a few kilobytes of heap.
const NOTIFY_TITLE_LIMIT = 60;
const NOTIFY_BODY_LIMIT = 300;

function clip(text: string, limit: number): string {
  return text.length > limit ? `${text.slice(0, limit - 1)}…` : text;
}

// How often the weather is sent while a watch is connected.
const WEATHER_INTERVAL_MS = 30 * 60 * 1000;

const WatchContext = createContext<WatchContextValue | null>(null);

// Parse a line of watch output as JSON, or return null when it is console text.
function parseLine(line: string): unknown {
  const trimmed = line.trim();
  if (!trimmed.startsWith('{')) {
    return null;
  }
  try {
    return JSON.parse(trimmed);
  } catch {
    return null;
  }
}

// Owns the single transport for the app and exposes connection, traffic and settings to every screen.
export function WatchProvider({ children }: { children: ReactNode }) {
  const transportRef = useRef<WatchTransport | null>(null);
  const bufferRef = useRef(new LineBuffer());
  const packageRepliesRef = useRef(new ReplyQueue<PackageReply>());
  const nextId = useRef(1);

  const [connection, setConnection] = useState<ConnectionState>('disconnected');
  const [watchMode, setWatchMode] = useState<WatchMode>('application');
  const [watch, setWatch] = useState<DiscoveredWatch | null>(null);
  const [scanning, setScanning] = useState(false);
  const [found, setFound] = useState<DiscoveredWatch[]>([]);
  const [consoleEntries, setConsoleEntries] = useState<ConsoleEntry[]>([]);
  const [lastError, setLastError] = useState<string | null>(null);
  const [findPhoneActive, setFindPhoneActive] = useState(false);
  const [settings, setSettings] = useState<Settings>(defaultSettings);
  const [settingsLoaded, setSettingsLoaded] = useState(false);
  // Callbacks that outlive a render read the settings from here.
  const settingsRef = useRef(settings);
  useEffect(() => {
    settingsRef.current = settings;
  }, [settings]);
  const sendMessageRef = useRef<(message: PhoneToWatchMessage) => Promise<void>>(async () => {});
  const [router] = useState(() => new ReplyRouter());


  // Pure check, so it needs neither state nor an effect.
  const transportKind = useMemo<TransportKind>(() => detectTransportKind(), []);

  const log = useCallback((direction: 'in' | 'out', text: string) => {
    setConsoleEntries((entries) => {
      const entry = { id: nextId.current++, at: Date.now(), direction, text };
      return [...entries, entry].slice(-CONSOLE_LIMIT);
    });
  }, []);

  const handleMessage = useCallback((message: WatchToPhoneMessage) => {
    if (router.push(message)) {
      return;
    }
    switch (message.t) {
      case 'findPhone':
        setFindPhoneActive(isFindPhoneOn(message));
        break;
      case 'music':
        if (settingsRef.current.forwardMusic) {
          void phoneBridge.mediaCommand(message.n).catch(() => {});
        }
        break;
      case 'call':
        if ('n' in message) {
          if (message.n === 'ACCEPT') {
            void phoneBridge.answerCall().catch(() => {});
          } else if (message.n === 'REJECT') {
            void phoneBridge.rejectCall().catch(() => {});
          }
        }
        break;
      case 'http': {
        const current = settingsRef.current;
        const answer = current.httpEnabled
          ? answerHttpRequest(message, parseHostList(current.httpAllowedHosts))
          : Promise.resolve({ t: 'http' as const, id: message.id, err: 'disabled on the phone' });
        void answer.then((reply) => sendMessageRef.current(reply));
        break;
      }
      case 'error':
        setLastError(message.msg);
        break;
      case 'info':
        break;
    }
  }, [router]);

  useEffect(() => {
    const transport = createTransport();
    transportRef.current = transport;
    transport.setListener({
      onState: (state) => {
        setConnection(state);
        if (state === 'disconnected') {
          setWatchMode('application');
          bufferRef.current.reset();
          packageRepliesRef.current.reset();
          router.reset();
        }
      },
      onMode: (mode) => setWatchMode(mode),
      onLine: (chunk) => {
        for (const line of bufferRef.current.push(chunk)) {
          log('in', line);
          const parsed = parseLine(line);
          if (isPackageReply(parsed)) {
            packageRepliesRef.current.push(parsed);
            continue;
          }
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
  }, [handleMessage, log, router]);

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

  const sendBytes = useCallback(
    async (data: Uint8Array) => {
      log('out', `[${data.length} raw bytes]`);
      try {
        await transportRef.current?.writeBytes(data);
      } catch (error) {
        setLastError((error as Error).message);
      }
    },
    [log],
  );

  const send = useCallback((message: PhoneToWatchMessage) => sendRaw(encodeForWatch(message)), [sendRaw]);
  useEffect(() => {
    sendMessageRef.current = send;
    router.setSender(send);
  }, [send, router]);

  const watchReady = connection === 'connected' && watchMode === 'application';
  // Phone events arrive whenever the platform sends them, so they check the link themselves.
  const readyRef = useRef(watchReady);
  useEffect(() => {
    readyRef.current = watchReady;
  }, [watchReady]);

  // Forward what the phone reports, unless the user turned it off or the phone is in Do Not Disturb.
  useEffect(() => {
    const quiet = async () =>
      settingsRef.current.respectDoNotDisturb && (await phoneBridge.isDoNotDisturb().catch(() => false));

    // An app that updates a notification posts it again, so unchanged repeats are not sent twice.
    const sent = new Map<number, string>();

    const forward = async (event: PhoneEvent) => {
      const current = settingsRef.current;
      if (!readyRef.current) {
        return;
      }
      switch (event.type) {
        case 'notification':
          if (current.forwardNotifications && !(await quiet())) {
            const title = clip(event.title, NOTIFY_TITLE_LIMIT);
            const body = clip(event.body, NOTIFY_BODY_LIMIT);
            const key = `${event.app}\n${title}\n${body}`;
            if (sent.get(event.id) === key) {
              break;
            }
            sent.set(event.id, key);
            await send({ t: 'notify', id: event.id, src: event.app, title, body });
          }
          break;
        case 'notificationRemoved':
          if (current.forwardNotifications && sent.delete(event.id)) {
            await send({ t: 'notify-', id: event.id });
          }
          break;
        case 'call':
          if (current.forwardCalls && !(event.state === 'incoming' && (await quiet()))) {
            await send({ t: 'call', cmd: event.state, name: event.name, number: event.number });
          }
          break;
        case 'media':
          if (current.forwardMusic) {
            await send({
              t: 'musicinfo',
              artist: event.artist,
              album: event.album,
              track: event.track,
              dur: event.durationSec,
            });
            await send({ t: 'musicstate', state: event.playing ? 'play' : 'pause', position: event.positionSec });
          }
          break;
      }
    };

    return phoneBridge.subscribe((event) => void forward(event));
  }, [send]);

  // Set the watch clock each time a watch connects, as Gadgetbridge does.
  useEffect(() => {
    if (!watchReady) {
      return;
    }
    // Sent once the connection has settled, outside the render that noticed it.
    const timer = setTimeout(() => void sendRaw(encodeSetTime()), 500);
    return () => clearTimeout(timer);
  }, [watchReady, sendRaw]);

  // Keep the app running in the background while a watch is connected, so the link survives.
  const watchName = watch?.name ?? 'watch';
  useEffect(() => {
    if (!watchReady || !phoneBridge.available) {
      return;
    }
    void phoneBridge.startKeepAlive(watchName).catch(() => {});
    return () => {
      void phoneBridge.stopKeepAlive().catch(() => {});
    };
  }, [watchReady, watchName]);

  const sendWeather = useCallback(async () => {
    const place = settingsRef.current.weatherPlace;
    if (!place) {
      throw new Error('Choose a place for the weather in Settings first.');
    }
    await send(await currentWeather(place));
  }, [send]);

  // Send the weather when a watch connects and every half hour after that.
  const weatherOn = watchReady && settings.forwardWeather && settings.weatherPlace !== null;
  useEffect(() => {
    if (!weatherOn) {
      return;
    }
    const push = () => sendWeather().catch((error: Error) => setLastError(error.message));
    void push();
    const timer = setInterval(push, WEATHER_INTERVAL_MS);
    return () => clearInterval(timer);
  }, [weatherOn, sendWeather]);

  const clearConsole = useCallback(() => setConsoleEntries([]), []);

  const dfuLink = useCallback(() => transportRef.current?.dfuLink() ?? null, []);

  const packageChannel = useMemo<PackageChannel>(
    () => ({
      send: (text: string) => sendRaw(text),
      sendBytes: (data: Uint8Array) => sendBytes(data),
      next: () => packageRepliesRef.current.next(),
    }),
    [sendRaw, sendBytes],
  );

  const value = useMemo<WatchContextValue>(
    () => ({
      transportKind,
      connection,
      watchMode,
      watchReady,
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
      packageChannel,
      dfuLink,
      requests: router,
      sendWeather,
    }),
    [
      watchReady,
      sendWeather,
      router,
      transportKind,
      connection,
      watchMode,
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
      packageChannel,
      dfuLink,
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
