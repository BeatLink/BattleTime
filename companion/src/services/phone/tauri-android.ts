// The Tauri app on Android: the phone bridge plugin's Kotlin, reached through `window.battletime.phone` from tauri/src/bridge.js.
//
// The plugin reads the notification listener, calls, media sessions and Do Not Disturb, and sends raw
// events that events.ts checks.

import type { AndroidIntents } from '../remote-actions';
import { toPhoneEvent } from './events';
import type { DeviceLocation, MediaCommand, PhoneBridge, PhoneEvent } from './types';

// ============================================================================
// What the bridge carries
// ============================================================================

type BridgeResult = { ok: boolean; error?: string; value?: unknown };

export type TauriPhoneDevice = {
  hasNotificationAccess(): Promise<BridgeResult>;
  openNotificationAccessSettings(): Promise<BridgeResult>;
  requestCallPermissions(): Promise<BridgeResult>;
  isDoNotDisturb(): Promise<BridgeResult>;
  answerCall(): Promise<BridgeResult>;
  rejectCall(): Promise<BridgeResult>;
  silenceCall(): Promise<BridgeResult>;
  mediaCommand(command: MediaCommand): Promise<BridgeResult>;
  locate(): Promise<BridgeResult>;
  associateWatch(address: string): Promise<BridgeResult>;
  isIgnoringBatteryOptimizations(): Promise<BridgeResult>;
  requestIgnoreBatteryOptimizations(): Promise<BridgeResult>;
  openUri(uri: string): Promise<BridgeResult>;
  startActivity(
    packageName: string,
    className: string,
    action: string | null,
    extras: Record<string, string>,
  ): Promise<BridgeResult>;
  sendBroadcast(action: string, packageName: string | null, extras: Record<string, string>): Promise<BridgeResult>;
  onEvent(listener: (raw: unknown) => void): () => void;
};

// The phone half of the bridge, present only in the Tauri app on Android.
export function tauriPhoneDevice(): TauriPhoneDevice | null {
  return (globalThis as { battletime?: { phone?: TauriPhoneDevice } }).battletime?.phone ?? null;
}

async function unwrap<T>(pending: Promise<BridgeResult>): Promise<T> {
  const result = await pending;
  if (!result.ok) {
    throw new Error(result.error ?? 'The phone bridge failed.');
  }
  return result.value as T;
}

// ============================================================================
// The bridge
// ============================================================================

export function createTauriPhoneBridge(device: TauriPhoneDevice): PhoneBridge {
  return {
    available: true,
    kind: 'phone',
    supportsCalls: true,
    supportsLocation: true,
    hasNotificationAccess: () => unwrap<boolean>(device.hasNotificationAccess()),
    openNotificationAccessSettings: () => unwrap<void>(device.openNotificationAccessSettings()),
    requestCallPermissions: () => unwrap<boolean>(device.requestCallPermissions()),
    isDoNotDisturb: () => unwrap<boolean>(device.isDoNotDisturb()),
    subscribe(listener: (event: PhoneEvent) => void) {
      return device.onEvent((raw) => {
        const event = toPhoneEvent(raw);
        if (event) listener(event);
      });
    },
    answerCall: () => unwrap<void>(device.answerCall()),
    rejectCall: () => unwrap<void>(device.rejectCall()),
    silenceCall: () => unwrap<void>(device.silenceCall()),
    mediaCommand: (command) => unwrap<void>(device.mediaCommand(command)),
    async locate(): Promise<DeviceLocation> {
      const found = await unwrap<{ latitude: number; longitude: number; name: string }>(device.locate());
      return { latitude: found.latitude, longitude: found.longitude, name: found.name || 'Here' };
    },
    associateWatch: (id) => unwrap<boolean>(device.associateWatch(id)),
    isIgnoringBatteryOptimizations: () => unwrap<boolean>(device.isIgnoringBatteryOptimizations()),
    requestIgnoreBatteryOptimizations: () => unwrap<boolean>(device.requestIgnoreBatteryOptimizations()),
  };
}

// The remote buttons' Android intents, started by the same plugin.
export function tauriIntents(device: TauriPhoneDevice | null): AndroidIntents | null {
  if (!device) {
    return null;
  }
  return {
    openUri: (uri) => unwrap<void>(device.openUri(uri)),
    startActivity: (packageName, className, action, extras) =>
      unwrap<void>(device.startActivity(packageName, className, action, extras)),
    sendBroadcast: (action, packageName, extras) => unwrap<void>(device.sendBroadcast(action, packageName, extras)),
  };
}
