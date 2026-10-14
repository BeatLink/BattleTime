import { createContext, type ComponentChildren } from 'preact';
import { useCallback, useContext, useEffect, useMemo, useRef, useState } from 'preact/hooks';

import { createTransport, detectTransportKind } from '@/ble/create-transport';
import { remoteTapLine } from '@/ble/mock-gadgetbridge';
import { MockTransport } from '@/ble/mock-transport';
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
  type StatusReply,
  type WatchToPhoneMessage,
  answerStateRequest,
  musicMessages,
} from '@/protocol/gadgetbridge';
import type { DfuLink } from '@/dfu/link';
import { FileService } from '@/protocol/file-service';
import { removeNote } from '@/protocol/notes';
import type { RemoteAction } from '@/protocol/remote';
import { PackagesClient } from '@/protocol/packages';
import {
  NO_CAPABILITIES,
  readBatteryLevel,
  readCapabilities,
  readDiagnostics,
  setWatchTime,
  type DiagnosticCounter,
  type WatchCapabilities,
} from '@/protocol/neotime';
import { readStatus, ReplyRouter, type RequestChannel } from '@/protocol/requests';
import { answerHttpRequest, parseHostList } from '@/services/http-proxy';
import { noteFileAccess, type NoteFileAccess } from '@/services/note-file';
import { noteReader } from '@/services/note-source';
import { phoneBridge, type PhoneEvent } from '@/services/phone';
import { controlCall } from '@/services/phone/call-control';
import { CallSummary } from '@/services/phone/call-summary';
import {
  buttonName,
  handleRemoteTap,
  REMOTE_LOG_LIMIT,
  runRemoteAction,
  unavailableReason,
  type RemoteLogEntry,
} from '@/services/remote-actions';
import { remotePlatform } from '@/services/remote-platform';
import { shareSyncInterval, syncBridge, syncIntervalMs } from '@/services/sync';
import { currentWeather, weatherPlace } from '@/services/weather';
import { watchClockChanges } from '@/state/clock-changes';
import { NoteSync, type NoteStatus } from '@/state/note-sync';
import { RemoteSync, type RemoteStatus } from '@/state/remote-sync';
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
  // Which services the connected watch serves; the UART features need `uart`.
  capabilities: WatchCapabilities;
  // NeoTime's diagnostic counters, read now.
  readDiagnostics: () => Promise<DiagnosticCounter[]>;
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
  // NeoTime's file service on the connected watch, opened on first use.
  files: () => Promise<FileService>;
  // The packages characteristic on the connected watch, opened on first use.
  packages: () => Promise<PackagesClient>;
  // Raw GATT for a firmware update, or null where the transport has no watch
  // or cannot reach past the UART service.
  dfuLink: () => DfuLink | null;
  // Channel for the Gadgetbridge messages the watch answers: settings, alarms and steps.
  requests: RequestChannel;
  // Fetch the weather for the chosen place and send it now.
  sendWeather: () => Promise<void>;
  // The watch's battery and step count as last read, and when that was.
  status: StatusReply | null;
  // The battery level from the standard Battery service, for a watch without the UART channel.
  batteryLevel: number | null;
  lastSynced: number | null;
  refreshStatus: () => Promise<void>;
  // What the phone is playing, as its media session last reported it.
  nowPlaying: NowPlaying | null;
  // Choosing and reading the text file the watch's notes app shows.
  noteFiles: NoteFileAccess;
  // How keeping the watch's note up to date last went.
  noteStatus: NoteStatus;
  // Reads the note's source now and sends it if it changed.
  syncNote: () => Promise<void>;
  // Clears the chosen source, stops keeping a note on the watch and takes it off.
  forgetNote: () => Promise<void>;
  // How sending the remote layout to the watch last went.
  remoteStatus: RemoteStatus;
  // Recent remote button taps and tests, newest first, and how each went.
  remoteLog: RemoteLogEntry[];
  clearRemoteLog: () => void;
  // Writes the remote layout to the watch now, even if it looks unchanged.
  sendRemoteLayout: () => Promise<void>;
  // Runs a stored button's action here, as a tap on the watch would.
  testRemoteButton: (index: number) => Promise<void>;
  // Why this device cannot run an action, or null when it can.
  remoteUnavailable: (action: RemoteAction) => string | null;
  // Delivers a tap as the mock watch, or is null with a real watch.
  simulateRemoteTap: ((index: number) => void) | null;
};

export type NowPlaying = Extract<PhoneEvent, { type: 'media' }>;

// Longest notification text sent, since the watch keeps notifications in a few kilobytes of heap.
const NOTIFY_TITLE_LIMIT = 60;
const NOTIFY_BODY_LIMIT = 300;

function clip(text: string, limit: number): string {
  return text.length > limit ? `${text.slice(0, limit - 1)}…` : text;
}

// How often the chosen note is read again while a watch is connected; the desktop app also hears each save.
const NOTE_CHECK_INTERVAL_MS = 60 * 1000;

// How often the weather is sent while a watch is connected.
const WEATHER_INTERVAL_MS = 30 * 60 * 1000;

const WatchContext = createContext<WatchContextValue | null>(null);

// Owns the single transport for the app and exposes connection, traffic and settings to every screen.
export function WatchProvider({ children }: { children: ComponentChildren }) {
  const transportRef = useRef<WatchTransport | null>(null);
  const bufferRef = useRef(new LineBuffer());
  const filesRef = useRef<Promise<FileService> | null>(null);
  const packagesRef = useRef<Promise<PackagesClient> | null>(null);
  const nextId = useRef(1);
  const callSummaryRef = useRef(new CallSummary());

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
  const [status, setStatus] = useState<StatusReply | null>(null);
  const [lastSynced, setLastSynced] = useState<number | null>(null);
  const [nowPlaying, setNowPlaying] = useState<NowPlaying | null>(null);
  // A watch's request arrives in a callback that outlives a render, so it reads what plays from here.
  const nowPlayingRef = useRef<NowPlaying | null>(null);
  useEffect(() => {
    nowPlayingRef.current = nowPlaying;
  }, [nowPlaying]);
  const [capabilities, setCapabilities] = useState<WatchCapabilities>(NO_CAPABILITIES);
  const [batteryLevel, setBatteryLevel] = useState<number | null>(null);
  // Callbacks that outlive a render read the settings from here.
  const settingsRef = useRef(settings);
  useEffect(() => {
    settingsRef.current = settings;
  }, [settings]);
  const sendMessageRef = useRef<(message: PhoneToWatchMessage) => Promise<void>>(async () => {});
  const [router] = useState(() => new ReplyRouter());
  // What this device can do for remote buttons; play/pause follows what is playing now.
  const playing = nowPlaying?.playing ?? false;
  const remoteDevice = useMemo(
    () =>
      remotePlatform({
        media: phoneBridge.available ? (command) => phoneBridge.mediaCommand(command) : null,
        isPlaying: () => playing,
        findPhone: () => setFindPhoneActive(true),
      }),
    [playing],
  );
  // Taps arrive in a callback that outlives a render, so it reads the device from here.
  const remoteDeviceRef = useRef(remoteDevice);
  useEffect(() => {
    remoteDeviceRef.current = remoteDevice;
  }, [remoteDevice]);
  const [remoteLog, setRemoteLog] = useState<RemoteLogEntry[]>([]);
  const addRemoteLog = useCallback((entry: RemoteLogEntry) => {
    setRemoteLog((entries) => [entry, ...entries].slice(0, REMOTE_LOG_LIMIT));
  }, []);


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
      case 'request':
        for (const reply of answerStateRequest(message, nowPlayingRef.current, settingsRef.current.forwardMusic)) {
          void sendMessageRef.current(reply);
        }
        break;
      case 'call':
        if ('n' in message) {
          callSummaryRef.current.noteWatchAnswer(message.n);
          void controlCall(phoneBridge, message.n).catch(() => {});
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
      case 'app':
        // Only the button's index comes from the watch; its action is looked up in this app's settings.
        void handleRemoteTap(message, settingsRef.current.remoteButtons, remoteDeviceRef.current).then((entry) => {
          if (entry) addRemoteLog(entry);
        });
        break;
      case 'error':
        setLastError(message.msg);
        break;
      case 'info':
        break;
    }
  }, [router, addRemoteLog]);

  useEffect(() => {
    const transport = createTransport();
    transportRef.current = transport;
    transport.setListener({
      onState: (state) => {
        setConnection(state);
        if (state === 'disconnected') {
          setWatchMode('application');
          setCapabilities(NO_CAPABILITIES);
          setBatteryLevel(null);
          bufferRef.current.reset();
          void filesRef.current?.then((service) => service.close()).catch(() => {});
          filesRef.current = null;
          void packagesRef.current?.then((client) => client.close()).catch(() => {});
          packagesRef.current = null;
          router.reset();
        }
      },
      onMode: (mode) => setWatchMode(mode),
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

  const send = useCallback((message: PhoneToWatchMessage) => sendRaw(encodeForWatch(message)), [sendRaw]);
  useEffect(() => {
    sendMessageRef.current = send;
    router.setSender(send);
  }, [send, router]);

  const watchReady = connection === 'connected' && watchMode === 'application';
  // The UART features need the message channel as well as running firmware.
  const uartReady = watchReady && capabilities.uart;
  // Phone events arrive whenever the platform sends them, so they check the link themselves.
  const readyRef = useRef(uartReady);
  useEffect(() => {
    readyRef.current = uartReady;
  }, [uartReady]);

  // Forward what the phone reports, unless the user turned it off or the phone is in Do Not Disturb.
  useEffect(() => {
    const quiet = async () =>
      settingsRef.current.respectDoNotDisturb && (await phoneBridge.isDoNotDisturb().catch(() => false));

    // An app that updates a notification posts it again, so unchanged repeats are not sent twice.
    const sent = new Map<number, string>();

    const forward = async (event: PhoneEvent) => {
      if (event.type === 'media') {
        setNowPlaying(event);
      }
      // Calls are followed even while the watch is away, so a summary never names the wrong outcome.
      const callSummary = event.type === 'call' ? callSummaryRef.current.noteCall(event) : null;
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
          if (callSummary && current.forwardNotifications && !(await quiet())) {
            await send(callSummary);
          }
          break;
        case 'media':
          if (current.forwardMusic) {
            for (const message of musicMessages(event)) {
              await send(message);
            }
          }
          break;
      }
    };

    return phoneBridge.subscribe((event) => void forward(event));
  }, [send]);

  // Find out what the watch serves, then set its clock, as Gadgetbridge does. Writing the time is also
  // the exchange that makes NeoTime keep a firmware it is trying out.
  useEffect(() => {
    if (!watchReady) {
      return;
    }
    let cancelled = false;
    // Run once the connection has settled, outside the render that noticed it.
    const timer = setTimeout(async () => {
      const link = transportRef.current?.dfuLink();
      if (!link) {
        return;
      }
      try {
        const found = await readCapabilities(link);
        if (cancelled) {
          return;
        }
        setCapabilities(found);
        if (found.time) {
          await setWatchTime(link);
        }
      } catch (error) {
        setLastError((error as Error).message);
      }
    }, 500);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [watchReady]);

  // Set the watch again whenever this device's clock or time zone changes, and every few hours anyway.
  const clockOn = watchReady && capabilities.time;
  useEffect(() => {
    if (!clockOn) {
      return;
    }
    return watchClockChanges(() => {
      const link = transportRef.current?.dfuLink();
      if (link) {
        void setWatchTime(link).catch((error: Error) => setLastError(error.message));
      }
    });
  }, [clockOn]);

  const readBattery = useCallback(async () => {
    const link = transportRef.current?.dfuLink();
    if (link) {
      setBatteryLevel(await readBatteryLevel(link));
      setLastSynced(Date.now());
    }
  }, []);

  const refreshStatus = useCallback(async () => {
    if (!capabilities.uart) {
      await readBattery();
      return;
    }
    const reply = await readStatus(router);
    setStatus(reply);
    setLastSynced(Date.now());
  }, [router, capabilities.uart, readBattery]);

  // Read the battery and step count once the clock is set, then on the sync interval.
  const statusOn = uartReady || (watchReady && capabilities.battery);
  const statusEvery = syncIntervalMs(settings.syncIntervalSeconds);
  useEffect(() => {
    if (!statusOn) {
      return;
    }
    const read = () => refreshStatus().catch(() => {});
    const first = setTimeout(read, 1500);
    const timer = setInterval(read, statusEvery);
    return () => {
      clearTimeout(first);
      clearInterval(timer);
    };
  }, [statusOn, statusEvery, refreshStatus]);

  // The desktop app's core reads the rest of the watch on the same interval.
  const syncIntervalSeconds = settings.syncIntervalSeconds;
  useEffect(() => {
    const bridge = syncBridge();
    if (settingsLoaded && bridge) {
      shareSyncInterval(bridge, syncIntervalMs(syncIntervalSeconds) / 1000).catch(() => {});
    }
  }, [settingsLoaded, syncIntervalSeconds]);

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
    const current = settingsRef.current;
    const locate = current.weatherFromDevice && phoneBridge.supportsLocation ? () => phoneBridge.locate() : null;
    await send(await currentWeather(await weatherPlace(current.weatherPlace, locate)));
  }, [send]);

  // Send the weather when a watch connects and every half hour after that.
  const weatherFromDevice = settings.weatherFromDevice && phoneBridge.supportsLocation;
  const weatherOn = uartReady && settings.forwardWeather && (settings.weatherPlace !== null || weatherFromDevice);
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

  const readWatchDiagnostics = useCallback(async () => {
    const link = transportRef.current?.dfuLink();
    if (!link) {
      throw new Error('Connect to a watch first.');
    }
    return readDiagnostics(link);
  }, []);

  const files = useCallback(() => {
    if (!filesRef.current) {
      const link = transportRef.current?.dfuLink();
      if (!link) {
        return Promise.reject(new Error('Connect to a watch first.'));
      }
      const opening = FileService.open(link);
      // A failed open is forgotten, so the next use tries again.
      opening.catch(() => {
        if (filesRef.current === opening) filesRef.current = null;
      });
      filesRef.current = opening;
    }
    return filesRef.current;
  }, []);

  const packages = useCallback(() => {
    if (!packagesRef.current) {
      const link = transportRef.current?.dfuLink();
      if (!link) {
        return Promise.reject(new Error('Connect to a watch first.'));
      }
      const opening = PackagesClient.open(link);
      // A failed open is forgotten, so the next use tries again.
      opening.catch(() => {
        if (packagesRef.current === opening) packagesRef.current = null;
      });
      packagesRef.current = opening;
    }
    return packagesRef.current;
  }, []);

  // Keep the watch's note matching its source: on connecting, after each save of a chosen file the
  // platform reports, every minute, and whenever the source, its contents or the font change here.
  const noteFiles = useMemo(() => noteFileAccess(), []);
  const [noteStatus, setNoteStatus] = useState<NoteStatus>({ kind: 'idle' });
  const noteSyncRef = useRef<NoteSync | null>(null);
  const noteSource = settings.noteSource;
  const noteFile = noteSource === 'file' ? settings.noteFile : null;
  const watchId = watch?.id ?? null;
  const hasNote = noteReader(settings, noteFiles) !== null;
  const notesOn = watchReady && capabilities.files && hasNote && watchId !== null;
  useEffect(() => {
    if (!notesOn || watchId === null) {
      noteSyncRef.current = null;
      return;
    }
    const sync = new NoteSync({
      files,
      watchId,
      current: () => ({
        read: noteReader(settingsRef.current, noteFiles),
        font: settingsRef.current.noteFont,
        onWatch: settingsRef.current.noteOnWatch,
      }),
      remember: (onWatch) => void updateSettings({ noteOnWatch: onWatch }),
      onStatus: setNoteStatus,
    });
    noteSyncRef.current = sync;
    void sync.sync();
    const timer = setInterval(() => void sync.sync(), NOTE_CHECK_INTERVAL_MS);
    const stopWatching = noteFile ? noteFiles.watch(noteFile, () => void sync.sync()) : null;
    return () => {
      clearInterval(timer);
      stopWatching?.();
      noteSyncRef.current = null;
    };
    // The file's link is what matters, not the object holding it.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [notesOn, noteSource, noteFile?.uri, watchId, noteFiles, files, updateSettings]);

  const { noteFont, noteText, noteUrl, noteUrlAccess } = settings;
  useEffect(() => {
    void noteSyncRef.current?.sync();
  }, [noteFont, noteText, noteUrl, noteUrlAccess]);

  const syncNote = useCallback(async () => {
    await noteSyncRef.current?.sync();
  }, []);

  const forgetNote = useCallback(async () => {
    const cleared = { typed: { noteText: '' }, file: { noteFile: null }, url: { noteUrl: '' } }[
      settingsRef.current.noteSource
    ];
    await updateSettings({ ...cleared, noteOnWatch: null });
    setNoteStatus({ kind: 'idle' });
    if (watchReady && capabilities.files) {
      await removeNote(await files());
    }
  }, [updateSettings, watchReady, capabilities.files, files]);

  // Keep the watch's remote layout matching the buttons here, once any have been set up.
  const [remoteStatus, setRemoteStatus] = useState<RemoteStatus>({ kind: 'idle' });
  const remoteSyncRef = useRef<RemoteSync | null>(null);
  const remoteButtons = settings.remoteButtons;
  const remoteInUse = remoteButtons.length > 0 || settings.remoteOnWatch !== null;
  const remoteOn = watchReady && capabilities.files && remoteInUse && watchId !== null;
  useEffect(() => {
    if (!remoteOn || watchId === null) {
      remoteSyncRef.current = null;
      return;
    }
    const sync = new RemoteSync({
      files,
      watchId,
      current: () => ({ buttons: settingsRef.current.remoteButtons, onWatch: settingsRef.current.remoteOnWatch }),
      remember: (onWatch) => void updateSettings({ remoteOnWatch: onWatch }),
      onStatus: setRemoteStatus,
    });
    remoteSyncRef.current = sync;
    void sync.sync();
    return () => {
      remoteSyncRef.current = null;
    };
  }, [remoteOn, watchId, files, updateSettings]);

  useEffect(() => {
    void remoteSyncRef.current?.sync();
  }, [remoteButtons]);

  const sendRemoteLayout = useCallback(async () => {
    if (!remoteSyncRef.current) {
      throw new Error('Connect to a watch that serves files first.');
    }
    await remoteSyncRef.current.sync(true);
  }, []);

  const testRemoteButton = useCallback(
    async (index: number) => {
      const button = settingsRef.current.remoteButtons[index];
      if (!button) {
        return;
      }
      const outcome = await runRemoteAction(button.action, remoteDevice);
      addRemoteLog({ at: Date.now(), button: index, label: buttonName(button), source: 'test', ...outcome });
    },
    [remoteDevice, addRemoteLog],
  );

  const remoteUnavailable = useCallback((action: RemoteAction) => unavailableReason(action, remoteDevice), [remoteDevice]);

  const clearRemoteLog = useCallback(() => setRemoteLog([]), []);

  const simulateRemoteTap = useMemo(
    () =>
      transportKind === 'mock'
        ? (index: number) => {
            const transport = transportRef.current;
            if (transport instanceof MockTransport) {
              transport.simulateWatchLine(remoteTapLine(index));
            }
          }
        : null,
    [transportKind],
  );

  const value = useMemo<WatchContextValue>(
    () => ({
      transportKind,
      connection,
      watchMode,
      watchReady,
      capabilities,
      readDiagnostics: readWatchDiagnostics,
      batteryLevel,
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
      files,
      packages,
      dfuLink,
      requests: router,
      sendWeather,
      status: uartReady ? status : null,
      lastSynced,
      refreshStatus,
      nowPlaying,
      noteFiles,
      noteStatus,
      syncNote,
      forgetNote,
      remoteStatus,
      remoteLog,
      clearRemoteLog,
      sendRemoteLayout,
      testRemoteButton,
      remoteUnavailable,
      simulateRemoteTap,
    }),
    [
      remoteStatus,
      remoteLog,
      clearRemoteLog,
      sendRemoteLayout,
      testRemoteButton,
      remoteUnavailable,
      simulateRemoteTap,
      noteFiles,
      noteStatus,
      syncNote,
      forgetNote,
      status,
      lastSynced,
      refreshStatus,
      nowPlaying,
      watchReady,
      uartReady,
      capabilities,
      readWatchDiagnostics,
      batteryLevel,
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
      files,
      packages,
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
