/**
 * @jest-environment jsdom
 */
import '@/test-support/jsdom-text-encoding';

import { afterEach, beforeEach, describe, expect, it, jest } from '@jest/globals';
import { createElement, render } from 'preact';
import { act } from 'preact/test-utils';

import type { DaemonState } from '@/services/daemon';
import { phoneBridge } from '@/services/phone';

import { defaultSettings } from './settings';
import { useWatch, WatchProvider } from './watch-provider';

// ============================================================================
// A fake app around the window
// ============================================================================

type Listener = (payload: never) => void;

const ok = (value?: unknown) => Promise.resolve({ ok: true, value });

const connectedState: DaemonState = {
  connection: 'connected',
  mode: 'application',
  watch: { id: 'AA', name: 'BattleTime' },
  capabilities: { dfu: true, time: true, battery: true, diagnostics: true, files: true, packages: true, uart: true },
  status: { t: 'status', bat: 77, mv: 3900, charging: false, steps: 1200 },
  batteryLevel: null,
  lastSynced: 1000,
  nowPlaying: { playing: true, track: 'Song' },
  findPhone: true,
  lastError: null,
  reconnecting: false,
  features: { noteStatus: { kind: 'synced', checkedAt: 5, bytes: 3 }, remoteLog: [] },
};

function fakeApp(storedSettings: Record<string, unknown>) {
  const daemonListeners: Record<string, Listener[]> = { state: [], settings: [], sent: [] };
  const daemon = {
    state: jest.fn(() => ok({ ...connectedState, connection: 'disconnected', watch: null })),
    settings: jest.fn(() => ok(storedSettings)),
    updateSettings: jest.fn(() => ok({})),
    importSettings: jest.fn(() => ok(true)),
    connect: jest.fn(() => ok()),
    disconnect: jest.fn(() => ok()),
    hold: jest.fn(() => ok()),
    refreshStatus: jest.fn(() => ok()),
    stopFindingPhone: jest.fn(() => ok()),
    sendWeather: jest.fn(() => ok()),
    syncNote: jest.fn(() => ok()),
    forgetNote: jest.fn(() => ok()),
    sendRemoteLayout: jest.fn(() => ok()),
    testRemoteButton: jest.fn(() => ok()),
    clearRemoteLog: jest.fn(() => ok()),
    onState: (listener: Listener) => (daemonListeners.state.push(listener), () => {}),
    onSettings: (listener: Listener) => (daemonListeners.settings.push(listener), () => {}),
    onSent: (listener: Listener) => (daemonListeners.sent.push(listener), () => {}),
  };
  const bluetooth = {
    startScan: jest.fn(() => ok()),
    stopScan: jest.fn(() => ok()),
    connect: jest.fn(() => ok()),
    disconnect: jest.fn(() => ok()),
    write: jest.fn(() => ok()),
    on: () => () => {},
  };
  (globalThis as { battletime?: unknown }).battletime = { ...bluetooth, daemon };
  const tell = (name: string, payload: unknown) => {
    for (const listener of daemonListeners[name]) {
      listener(payload as never);
    }
  };
  return { daemon, bluetooth, tell };
}

function mount() {
  const result = {} as { current: ReturnType<typeof useWatch> };
  function Probe() {
    result.current = useWatch();
    return null;
  }
  act(() => {
    render(createElement(WatchProvider, null, createElement(Probe, null)), document.createElement('div'));
  });
  return result;
}

// Lets the chain of promises behind loading the settings run out.
const settle = () =>
  act(async () => {
    for (let turn = 0; turn < 10; turn++) {
      await Promise.resolve();
    }
  });

beforeEach(() => {
  localStorage.clear();
  // jsdom has no fetch, which the remote buttons' web requests take as their default.
  (globalThis as { fetch?: unknown }).fetch = jest.fn();
});

afterEach(() => {
  delete (globalThis as { battletime?: unknown }).battletime;
  jest.restoreAllMocks();
});

// ============================================================================
// Tests
// ============================================================================

describe('the window in the app', () => {
  it('hands its old settings to a daemon that has none', async () => {
    localStorage.setItem('battletime.settings.v1', JSON.stringify({ forwardMusic: false }));
    const { daemon } = fakeApp({});

    const watch = mount();
    await settle();

    expect(daemon.importSettings).toHaveBeenCalledWith(expect.objectContaining({ forwardMusic: false }));
    expect(watch.current.settings.forwardMusic).toBe(false);
    expect(watch.current.settingsLoaded).toBe(true);
  });

  it('reads the settings the daemon keeps and saves changes to it', async () => {
    const { daemon, tell } = fakeApp({ forwardCalls: false });

    const watch = mount();
    await settle();
    await act(() => watch.current.updateSettings({ forwardWeather: true }));

    expect(daemon.importSettings).not.toHaveBeenCalled();
    expect(watch.current.settings.forwardCalls).toBe(false);
    expect(daemon.updateSettings).toHaveBeenCalledWith({ forwardWeather: true });
    expect(localStorage.getItem('battletime.settings.v1')).toBeNull();

    act(() => tell('settings', { ...defaultSettings, forwardCalls: true }));
    expect(watch.current.settings.forwardCalls).toBe(true);
  });

  it('shows what the daemon reports about the watch', async () => {
    const { tell } = fakeApp({ lastWatchId: 'AA' });

    const watch = mount();
    await settle();
    expect(watch.current.connection).toBe('disconnected');
    act(() => tell('state', connectedState));
    act(() => tell('sent', 'GB({"t":"status"})'));

    expect(watch.current.watchReady).toBe(true);
    expect(watch.current.watch).toEqual({ id: 'AA', name: 'BattleTime', rssi: null });
    expect(watch.current.status?.bat).toBe(77);
    expect(watch.current.nowPlaying).toEqual({ type: 'media', playing: true, track: 'Song' });
    expect(watch.current.findPhoneActive).toBe(true);
    expect(watch.current.noteStatus).toEqual({ kind: 'synced', checkedAt: 5, bytes: 3 });
    expect(watch.current.remoteStatus).toEqual({ kind: 'idle' });
    expect(watch.current.console.at(-1)).toMatchObject({ direction: 'out', text: 'GB({"t":"status"})' });

    act(() => tell('state', { ...connectedState, lastError: 'The watch did not reply.' }));
    expect(watch.current.lastError).toBe('The watch did not reply.');
  });

  it('asks the daemon to connect, disconnect and run each background job', async () => {
    const { daemon, bluetooth } = fakeApp({});

    const watch = mount();
    await settle();
    await act(() => watch.current.connect({ id: 'AA', name: 'BattleTime', rssi: -50 }));
    await act(async () => {
      await watch.current.refreshStatus();
      await watch.current.sendWeather();
      await watch.current.syncNote();
      await watch.current.forgetNote();
      await watch.current.sendRemoteLayout();
      await watch.current.testRemoteButton(1);
      watch.current.clearRemoteLog();
      await watch.current.disconnect();
    });

    expect(daemon.connect).toHaveBeenCalledWith('AA', 'BattleTime');
    expect(bluetooth.connect).not.toHaveBeenCalled();
    expect(daemon.refreshStatus).toHaveBeenCalled();
    expect(daemon.sendWeather).toHaveBeenCalled();
    expect(daemon.syncNote).toHaveBeenCalled();
    expect(daemon.forgetNote).toHaveBeenCalled();
    expect(daemon.sendRemoteLayout).toHaveBeenCalled();
    expect(daemon.testRemoteButton).toHaveBeenCalledWith(1);
    expect(daemon.clearRemoteLog).toHaveBeenCalled();
    expect(daemon.disconnect).toHaveBeenCalled();
    expect(bluetooth.disconnect).not.toHaveBeenCalled();
  });

  it('on a phone, makes the watch a companion device before connecting and leaves phone events to the daemon', async () => {
    const { daemon } = fakeApp({});
    jest.replaceProperty(phoneBridge, 'kind', 'phone');
    const order: string[] = [];
    const associate = jest.spyOn(phoneBridge, 'associateWatch').mockImplementation(async () => {
      order.push('associate');
      return true;
    });
    daemon.connect.mockImplementation(() => {
      order.push('connect');
      return ok();
    });
    const subscribe = jest.spyOn(phoneBridge, 'subscribe');

    const watch = mount();
    await settle();
    await act(() => watch.current.connect({ id: 'AA', name: 'BattleTime', rssi: -50 }));

    expect(associate).toHaveBeenCalledWith('AA');
    expect(order).toEqual(['associate', 'connect']);
    expect(subscribe).not.toHaveBeenCalled();
  });

  it('shows why the daemon could not connect', async () => {
    const { daemon } = fakeApp({});
    daemon.connect.mockImplementationOnce(() => Promise.resolve({ ok: false, value: undefined, error: 'No watch in range.' } as never));

    const watch = mount();
    await settle();
    await act(() => watch.current.connect({ id: 'AA', name: 'BattleTime', rssi: null }));

    expect(watch.current.lastError).toBe('No watch in range.');
  });
});
