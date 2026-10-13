// What this device can do for remote buttons: shell commands in the desktop app, intents on Android, media where the phone bridge reaches it.

import { androidIntents } from './android-intents';
import type { AndroidIntents, CommandResult, RemotePlatform } from './remote-actions';
import type { MediaCommand } from './phone';

type BridgeResult<Value> = { ok: boolean; error?: string; value?: Value };

// What electron/preload.js offers for remote buttons.
export type DesktopRemoteBridge = {
  runCommand(command: string): Promise<BridgeResult<CommandResult>>;
};

export function desktopRemoteBridge(): DesktopRemoteBridge | null {
  return (globalThis as { neotime?: { remote?: DesktopRemoteBridge } }).neotime?.remote ?? null;
}

// Runs a command through the desktop app's main process and hands back how it ended.
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
  fetch?: typeof fetch;
};

// Puts this device's parts together; the desktop bridge and Android intents are found unless given.
export function remotePlatform(parts: RemotePlatformParts): RemotePlatform {
  const desktop = parts.desktop === undefined ? desktopRemoteBridge() : parts.desktop;
  return {
    fetch: parts.fetch ?? ((input, init) => fetch(input, init)),
    runCommand: desktop ? desktopCommandRunner(desktop) : null,
    android: parts.android === undefined ? androidIntents : parts.android,
    media: parts.media,
    isPlaying: parts.isPlaying,
    findPhone: parts.findPhone,
  };
}
