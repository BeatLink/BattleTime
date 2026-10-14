// Runs what a remote button does: a web request, a shell command on a computer, an Android intent, or a ready-made action.
//
// The platform's parts come in through `RemotePlatform`, so each runner can be tested with them faked, and
// an action this device cannot run is reported as unavailable instead of being tried.

import {
  remoteTapIndex,
  type AndroidIntent,
  type BuiltinAction,
  type RemoteAction,
  type RemoteButton,
} from '@/protocol/remote';
import type { WatchToPhoneMessage } from '@/protocol/gadgetbridge';
import type { MediaCommand } from '@/services/phone';

// ============================================================================
// What the platform offers
// ============================================================================

// How a shell command ended: its exit code (null when killed), what it printed, and whether it ran out of time.
export type CommandResult = { exitCode: number | null; output: string; timedOut: boolean };

// The three ways the Android bridge starts something.
export type AndroidIntents = {
  openUri(uri: string): Promise<void>;
  startActivity(
    packageName: string,
    className: string,
    action: string | null,
    extras: Record<string, string>,
  ): Promise<void>;
  sendBroadcast(action: string, packageName: string | null, extras: Record<string, string>): Promise<void>;
};

export type RemotePlatform = {
  fetch: typeof fetch;
  // Runs a shell command on the computer; null outside the desktop app.
  runCommand: ((command: string) => Promise<CommandResult>) | null;
  // Starts Android intents; null anywhere but the Android app.
  android: AndroidIntents | null;
  // Sends a media key to whatever is playing; null where nothing can be controlled.
  media: ((command: MediaCommand) => Promise<void>) | null;
  // Whether something is playing now, which decides what play/pause does.
  isPlaying: () => boolean;
  // Shows that the watch is looking for this device, as the watch's own find phone does.
  findPhone: () => void;
};

// How long a web request may take before it counts as failed.
export const HTTP_TIMEOUT_MS = 15000;

// The longest result kept for the log line.
const RESULT_LIMIT = 120;

// ============================================================================
// Names for the screen
// ============================================================================

export const BUILTIN_LABELS: Record<BuiltinAction, string> = {
  playpause: 'Play or pause',
  play: 'Play',
  pause: 'Pause',
  next: 'Next track',
  previous: 'Previous track',
  volumeup: 'Volume up',
  volumedown: 'Volume down',
  findphone: 'Find this device',
};

// A short line saying what an action does.
export function describeAction(action: RemoteAction): string {
  switch (action.kind) {
    case 'http':
      return `${action.method} ${action.url}`;
    case 'command':
      return `Run ${action.command}`;
    case 'android':
      return describeIntent(action.intent);
    case 'builtin':
      return BUILTIN_LABELS[action.name];
  }
}

function describeIntent(intent: AndroidIntent): string {
  switch (intent.type) {
    case 'deeplink':
      return `Open ${intent.uri}`;
    case 'activity':
      return `Start ${intent.packageName}/${intent.className}`;
    case 'broadcast':
      return `Broadcast ${intent.action}`;
  }
}

// ============================================================================
// Checking and running
// ============================================================================

export type ActionOutcome = { ok: boolean; message: string };

// Why this device cannot run the action, or null when it can.
export function unavailableReason(action: RemoteAction, platform: RemotePlatform): string | null {
  switch (action.kind) {
    case 'http':
      return null;
    case 'command':
      return platform.runCommand ? null : 'Shell commands run only in the desktop app.';
    case 'android':
      return platform.android ? null : 'Android intents run only in the Android app.';
    case 'builtin':
      if (action.name === 'findphone') return null;
      return platform.media ? null : 'Media control needs the Android app or the desktop app.';
  }
}

const clip = (text: string) => {
  const flat = text.replace(/\s+/g, ' ').trim();
  return flat.length > RESULT_LIMIT ? `${flat.slice(0, RESULT_LIMIT - 1)}…` : flat;
};

async function runHttp(action: Extract<RemoteAction, { kind: 'http' }>, platform: RemotePlatform): Promise<ActionOutcome> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), HTTP_TIMEOUT_MS);
  try {
    const response = await platform.fetch(action.url, {
      method: action.method,
      headers: action.headers,
      body: action.method === 'GET' ? undefined : action.body,
      signal: controller.signal,
    });
    const text = clip(await response.text().catch(() => ''));
    return { ok: response.ok, message: `HTTP ${response.status}${text ? `: ${text}` : ''}` };
  } catch (error) {
    return {
      ok: false,
      message: controller.signal.aborted ? `No answer within ${HTTP_TIMEOUT_MS / 1000} s.` : errorText(error),
    };
  } finally {
    clearTimeout(timer);
  }
}

async function runCommand(command: string, platform: RemotePlatform): Promise<ActionOutcome> {
  const result = await platform.runCommand!(command);
  const output = clip(result.output);
  if (result.timedOut) {
    return { ok: false, message: `Stopped after running too long${output ? `: ${output}` : ''}` };
  }
  return {
    ok: result.exitCode === 0,
    message: `Exit ${result.exitCode ?? 'killed'}${output ? `: ${output}` : ''}`,
  };
}

async function runIntent(intent: AndroidIntent, android: AndroidIntents): Promise<ActionOutcome> {
  switch (intent.type) {
    case 'deeplink':
      await android.openUri(intent.uri);
      break;
    case 'activity':
      await android.startActivity(intent.packageName, intent.className, intent.action ?? null, intent.extras ?? {});
      break;
    case 'broadcast':
      await android.sendBroadcast(intent.action, intent.packageName ?? null, intent.extras ?? {});
      break;
  }
  return { ok: true, message: `${describeIntent(intent)}: done.` };
}

async function runBuiltin(name: BuiltinAction, platform: RemotePlatform): Promise<ActionOutcome> {
  if (name === 'findphone') {
    platform.findPhone();
    return { ok: true, message: 'Showing that the watch is looking for this device.' };
  }
  const command: MediaCommand = name === 'playpause' ? (platform.isPlaying() ? 'pause' : 'play') : name;
  await platform.media!(command);
  return { ok: true, message: `Sent ${command} to the media player.` };
}

function errorText(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

// Runs one action and says how it went; it never throws, and an unavailable action is skipped with the reason.
export async function runRemoteAction(action: RemoteAction, platform: RemotePlatform): Promise<ActionOutcome> {
  const reason = unavailableReason(action, platform);
  if (reason) {
    return { ok: false, message: `Skipped: ${reason}` };
  }
  try {
    switch (action.kind) {
      case 'http':
        return await runHttp(action, platform);
      case 'command':
        return await runCommand(action.command, platform);
      case 'android':
        return await runIntent(action.intent, platform.android!);
      case 'builtin':
        return await runBuiltin(action.name, platform);
    }
  } catch (error) {
    return { ok: false, message: errorText(error) };
  }
}

// ============================================================================
// Taps from the watch
// ============================================================================

// One line in the screen's log of recent taps and tests.
export type RemoteLogEntry = {
  at: number;
  button: number;
  label: string;
  source: 'watch' | 'test';
  ok: boolean;
  message: string;
};

export const REMOTE_LOG_LIMIT = 20;

// A button's name for the log: its label, or what it does when it has none.
export function buttonName(button: RemoteButton): string {
  return button.label.trim() || describeAction(button.action);
}

// Runs the stored button a watch message names, or gives null when the message is not a remote tap.
//
// Only the index comes from the watch; the action is always looked up in the companion's own buttons.
export async function handleRemoteTap(
  message: WatchToPhoneMessage,
  buttons: readonly RemoteButton[],
  platform: RemotePlatform,
  now: () => number = Date.now,
): Promise<RemoteLogEntry | null> {
  const index = remoteTapIndex(message);
  if (index === null) {
    return null;
  }
  const button = buttons[index];
  if (!button) {
    return {
      at: now(),
      button: index,
      label: `Button ${index + 1}`,
      source: 'watch',
      ok: false,
      message: `Ignored: there is no button ${index + 1}.`,
    };
  }
  const outcome = await runRemoteAction(button.action, platform);
  return { at: now(), button: index, label: buttonName(button), source: 'watch', ...outcome };
}
