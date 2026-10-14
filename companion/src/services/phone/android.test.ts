import { beforeEach, describe, expect, it, jest } from '@jest/globals';

import type { PhoneBridge, PhoneEvent } from './types';

const mockRequireOptionalNativeModule = jest.fn<(name: string) => unknown>();

jest.mock('expo', () => ({
  requireOptionalNativeModule: (name: string) => mockRequireOptionalNativeModule(name),
}));

// Loads the Android bridge afresh, so each test can decide whether the native module is there.
function loadBridge(): PhoneBridge {
  let bridge!: PhoneBridge;
  jest.isolateModules(() => {
    bridge = jest.requireActual<typeof import('./index.android')>('./index.android').phoneBridge;
  });
  return bridge;
}

function fakeNative() {
  let emit: (event: unknown) => void = () => {};
  const remove = jest.fn();
  const native = {
    hasNotificationAccess: jest.fn(async () => true),
    openNotificationAccessSettings: jest.fn(async () => {}),
    requestCallPermissions: jest.fn(async () => true),
    isDoNotDisturb: jest.fn(async () => true),
    answerCall: jest.fn(async () => {}),
    rejectCall: jest.fn(async () => {}),
    silenceCall: jest.fn(async () => {}),
    mediaCommand: jest.fn(async (_command: string) => {}),
    startKeepAlive: jest.fn(async (_name: string) => {}),
    stopKeepAlive: jest.fn(async () => {}),
    addListener: jest.fn((_name: string, listener: (event: unknown) => void) => {
      emit = listener;
      return { remove };
    }),
  };
  return { native, remove, emit: (event: unknown) => emit(event) };
}

beforeEach(() => {
  mockRequireOptionalNativeModule.mockReset();
});

describe('Android phone bridge', () => {
  it('falls back to the stub when the native module is missing, as in Expo Go', async () => {
    mockRequireOptionalNativeModule.mockReturnValue(null);
    const bridge = loadBridge();
    expect(bridge.available).toBe(false);
    await expect(bridge.answerCall()).rejects.toThrow('only available on Android');
    expect(mockRequireOptionalNativeModule).toHaveBeenCalledWith('PhoneBridge');
  });

  it('passes queries and actions through to the native module', async () => {
    const { native } = fakeNative();
    mockRequireOptionalNativeModule.mockReturnValue(native);
    const bridge = loadBridge();
    expect(bridge.available).toBe(true);
    await expect(bridge.hasNotificationAccess()).resolves.toBe(true);
    await expect(bridge.requestCallPermissions()).resolves.toBe(true);
    await expect(bridge.isDoNotDisturb()).resolves.toBe(true);
    await bridge.openNotificationAccessSettings();
    await bridge.answerCall();
    await bridge.rejectCall();
    await bridge.silenceCall();
    await bridge.mediaCommand('next');
    await bridge.startKeepAlive('PineTime');
    await bridge.stopKeepAlive();
    expect(native.openNotificationAccessSettings).toHaveBeenCalled();
    expect(native.answerCall).toHaveBeenCalled();
    expect(native.rejectCall).toHaveBeenCalled();
    expect(native.silenceCall).toHaveBeenCalled();
    expect(native.mediaCommand).toHaveBeenCalledWith('next');
    expect(native.startKeepAlive).toHaveBeenCalledWith('PineTime');
    expect(native.stopKeepAlive).toHaveBeenCalled();
  });

  it('hands on events it understands, drops the rest, and stops on unsubscribe', () => {
    const { native, remove, emit } = fakeNative();
    mockRequireOptionalNativeModule.mockReturnValue(native);
    const bridge = loadBridge();
    const events: PhoneEvent[] = [];
    const unsubscribe = bridge.subscribe((event) => events.push(event));
    expect(native.addListener).toHaveBeenCalledWith('onPhoneEvent', expect.any(Function));
    emit({ type: 'notificationRemoved', id: 3 });
    emit({ type: 'battery', level: 50 });
    expect(events).toEqual([{ type: 'notificationRemoved', id: 3 }]);
    unsubscribe();
    expect(remove).toHaveBeenCalled();
  });
});
