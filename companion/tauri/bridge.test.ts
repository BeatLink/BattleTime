import { readFileSync } from 'node:fs';
import { join } from 'node:path';

import { describe, expect, it, jest } from '@jest/globals';

// ============================================================================
// Helpers
// ============================================================================

type Bridge = Record<string, unknown> & {
  phone?: Record<string, (...args: unknown[]) => Promise<{ ok: boolean; value?: unknown; error?: string }>> & {
    onEvent(listener: (payload: unknown) => void): () => void;
  };
  plugins: { pageBase: string };
};

// Runs bridge.js against a fake Tauri, as src/lib.rs does with the Android flag set or not.
function loadBridge(onAndroid: boolean) {
  const invoke = jest.fn(async (command: string, args?: unknown) => ({ command, args }));
  let pluginListener: (payload: unknown) => void = () => {};
  const addPluginListener = jest.fn(async (_plugin: string, _event: string, listener: (payload: unknown) => void) => {
    pluginListener = listener;
  });
  const windowListeners = new Map<string, (event: unknown) => void>();
  const printed = { warn: jest.fn(), error: jest.fn() };
  const fakeWindow: Record<string, unknown> = {
    console: { ...printed },
    addEventListener: (name: string, listener: (event: unknown) => void) => windowListeners.set(name, listener),
    __BATTLETIME_ANDROID__: onAndroid,
    __TAURI_INTERNALS__: { invoke },
    __TAURI__: { core: { addPluginListener }, event: { listen: jest.fn() } },
  };
  const source = readFileSync(join(__dirname, 'src', 'bridge.js'), 'utf8');
  new Function('window', source)(fakeWindow);
  return {
    bridge: fakeWindow.battletime as Bridge,
    invoke,
    console: fakeWindow.console as typeof printed,
    printed,
    fireWindowEvent: (name: string, event: unknown) => windowListeners.get(name)?.(event),
    addPluginListener,
    sendPhoneEvent: (payload: unknown) => pluginListener(payload),
  };
}

// ============================================================================
// Tests
// ============================================================================

describe('bridge.js', () => {
  it('gives the desktop its computer and shell commands, and no phone', () => {
    const { bridge } = loadBridge(false);
    expect(bridge.desktop).toBeDefined();
    expect(bridge.remote).toBeDefined();
    expect(bridge.phone).toBeUndefined();
    expect(bridge.plugins.pageBase).toBe('plugin://');
  });

  it('gives Android the phone in place of the computer', () => {
    const { bridge } = loadBridge(true);
    expect(bridge.desktop).toBeUndefined();
    expect(bridge.remote).toBeUndefined();
    expect(bridge.phone).toBeDefined();
    expect(bridge.notes).toBeDefined();
    expect(bridge.plugins.pageBase).toBe('http://plugin.localhost/');
  });

  it('calls the phone plugin commands by their snake_case names', async () => {
    const { bridge, invoke } = loadBridge(true);
    const phone = bridge.phone!;
    await expect(phone.isDoNotDisturb()).resolves.toMatchObject({ ok: true });
    await phone.associateWatch('AA');
    await phone.startActivity('org.example', 'org.example.Main', null, { a: 'b' });
    expect(invoke).toHaveBeenCalledWith('plugin:phone-bridge|is_do_not_disturb', undefined);
    expect(invoke).toHaveBeenCalledWith('plugin:phone-bridge|associate_watch', { address: 'AA' });
    expect(invoke).toHaveBeenCalledWith('plugin:phone-bridge|start_activity', {
      packageName: 'org.example',
      className: 'org.example.Main',
      action: null,
      extras: { a: 'b' },
    });
  });

  it('reaches the daemon by its commands and hears its events', async () => {
    const { bridge, invoke } = loadBridge(false);
    const daemon = bridge.daemon as Record<string, (...args: unknown[]) => unknown>;
    await daemon.connect('AA', 'BattleTime');
    await daemon.updateSettings({ autoReconnect: false });
    await daemon.testRemoteButton(2);
    await daemon.hold(true);
    expect(invoke).toHaveBeenCalledWith('daemon_connect', { id: 'AA', name: 'BattleTime' });
    expect(invoke).toHaveBeenCalledWith('daemon_update_settings', { patch: { autoReconnect: false } });
    expect(invoke).toHaveBeenCalledWith('daemon_test_remote_button', { index: 2 });
    expect(invoke).toHaveBeenCalledWith('daemon_hold', { held: true });
    expect(typeof daemon.onState(() => {})).toBe('function');
  });

  it('turns a rejected command into an answer with the reason', async () => {
    const { bridge, invoke } = loadBridge(true);
    invoke.mockRejectedValueOnce('Allow the app to answer calls first');
    await expect(bridge.phone!.answerCall()).resolves.toEqual({
      ok: false,
      error: 'Allow the app to answer calls first',
    });
  });

  it('follows the plugin event once and hands each event to every listener until it stops', () => {
    const { bridge, addPluginListener, sendPhoneEvent } = loadBridge(true);
    const first = jest.fn();
    const second = jest.fn();
    const stopFirst = bridge.phone!.onEvent(first);
    bridge.phone!.onEvent(second);
    expect(addPluginListener).toHaveBeenCalledTimes(1);
    expect(addPluginListener).toHaveBeenCalledWith('phone-bridge', 'phone', expect.any(Function));
    sendPhoneEvent({ type: 'call', state: 'end' });
    stopFirst();
    sendPhoneEvent({ type: 'call', state: 'start' });
    expect(first).toHaveBeenCalledTimes(1);
    expect(second).toHaveBeenCalledTimes(2);
  });

  it("sends the window's errors to the companion's log with their stack, and still prints them", () => {
    const { console, printed, invoke } = loadBridge(false);
    const error = new Error('the watch said no');
    console.error('Could not read:', error);
    expect(printed.error).toHaveBeenCalledWith('Could not read:', error);
    const [command, args] = invoke.mock.calls[0];
    expect(command).toBe('app_log');
    expect(args).toMatchObject({ level: 'error' });
    expect((args as { message: string }).message).toMatch(/^Could not read: the watch said no\n/);
  });

  it('sends warnings, writing objects as JSON and any that cannot be as text', () => {
    const { console, invoke } = loadBridge(true);
    const loop: { self?: unknown } = {};
    loop.self = loop;
    console.warn('slow', { step: 'read' });
    console.warn(loop);
    expect(invoke).toHaveBeenCalledWith('app_log', { level: 'warn', message: 'slow {"step":"read"}' });
    expect(invoke).toHaveBeenCalledWith('app_log', { level: 'warn', message: '[object Object]' });
  });

  it('logs uncaught errors and rejections nothing handled', () => {
    const { fireWindowEvent, invoke } = loadBridge(false);
    fireWindowEvent('error', { message: 'Script error.' });
    fireWindowEvent('unhandledrejection', { reason: 'no link' });
    expect(invoke).toHaveBeenCalledWith('app_log', { level: 'error', message: 'Script error.' });
    expect(invoke).toHaveBeenCalledWith('app_log', { level: 'error', message: 'Unhandled rejection: no link' });
  });
});
