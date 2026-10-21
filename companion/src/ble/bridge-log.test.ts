/**
 * @jest-environment jsdom
 */
import { readFileSync } from 'node:fs';
import { join } from 'node:path';

import { beforeAll, beforeEach, describe, expect, it, jest } from '@jest/globals';

// tauri/src/bridge.js runs as the window's first script, so the test runs its source the same way.
const BRIDGE_SOURCE = readFileSync(join(__dirname, '../../tauri/src/bridge.js'), 'utf8');

const invoke = jest.fn((_command: string, _args?: unknown) => Promise.resolve(null));

describe("the bridge's log forwarding", () => {
  beforeAll(() => {
    (window as unknown as { __TAURI_INTERNALS__: unknown }).__TAURI_INTERNALS__ = { invoke };
    jest.spyOn(console, 'warn').mockImplementation(() => undefined);
    jest.spyOn(console, 'error').mockImplementation(() => undefined);
    window.eval(BRIDGE_SOURCE);
  });

  beforeEach(() => invoke.mockClear());

  it("sends the window's errors to the companion's log with their stack", () => {
    console.error('Could not read:', new Error('the watch said no'));
    const [command, args] = invoke.mock.calls[0];
    expect(command).toBe('app_log');
    expect(args).toMatchObject({ level: 'error' });
    expect((args as { message: string }).message).toMatch(/^Could not read: the watch said no\n/);
  });

  it('sends warnings and writes objects as JSON', () => {
    console.warn('slow', { step: 'read' });
    expect(invoke).toHaveBeenCalledWith('app_log', { level: 'warn', message: 'slow {"step":"read"}' });
  });

  it('sends an object that cannot become JSON as text', () => {
    const loop: { self?: unknown } = {};
    loop.self = loop;
    console.warn(loop);
    expect(invoke).toHaveBeenCalledWith('app_log', { level: 'warn', message: '[object Object]' });
  });

  it('logs a rejection nothing handled', () => {
    const event = new Event('unhandledrejection') as Event & { reason: unknown };
    event.reason = 'no link';
    window.dispatchEvent(event);
    expect(invoke).toHaveBeenCalledWith('app_log', { level: 'error', message: 'Unhandled rejection: no link' });
  });
});
