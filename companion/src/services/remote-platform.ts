// What this device can do for remote buttons: shell commands in the desktop app, intents on Android, media where the phone bridge reaches it.

import { androidIntents } from './android-intents';
import type { AndroidIntents, CommandResult, RemotePlatform } from './remote-actions';
import type { MediaCommand } from './phone';
import { fetchRequester, webRequester, type WebRequester } from './web-request';

type BridgeResult<Value> = { ok: boolean; error?: string; value?: Value };

// What window.battletime.remote (tauri/src/bridge.js) offers on the desktop.
export type DesktopRemoteBridge = {
  runCommand(command: string): Promise<BridgeResult<CommandResult>>;
};

export function desktopRemoteBridge(): DesktopRemoteBridge | null {
  return (globalThis as { battletime?: { remote?: DesktopRemoteBridge } }).battletime?.remote ?? null;
}

// Runs a command through the desktop app's core and hands back how it ended.
export function desktopCommandRunner(bridge: DesktopRemoteBridge): (command: string) => Promise<CommandResult> {
  return async (command) => {
    const result = await bridge.runCommand(command);
    if (!result.ok || !result.value) {
      throw new Error(result.error ?? 'The desktop app could not run the command.');
    }
    return result.value;
  };
}

export type RemotePlatformParts = {
  media: ((command: MediaCommand) => Promise<void>) | null;
  isPlaying: () => boolean;
  findPhone: () => void;
  desktop?: DesktopRemoteBridge | null;
  android?: AndroidIntents | null;
  web?: WebRequester;
  fetch?: typeof fetch;
};

// Puts this device's parts together; the desktop bridge, Android intents and the native web requests are found unless given.
export function remotePlatform(parts: RemotePlatformParts): RemotePlatform {
  const desktop = parts.desktop === undefined ? desktopRemoteBridge() : parts.desktop;
  return {
    web: parts.web ?? (parts.fetch ? fetchRequester(parts.fetch) : webRequester()),
    runCommand: desktop ? desktopCommandRunner(desktop) : null,
    android: parts.android === undefined ? androidIntents : parts.android,
    media: parts.media,
    isPlaying: parts.isPlaying,
    findPhone: parts.findPhone,
  };
}
