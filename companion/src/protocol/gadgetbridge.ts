// Message types for the Gadgetbridge (Bangle.js style) JSON protocol that wasp-os speaks over the Nordic UART Service.
// The watch side lives in wasp/gadgetbridge.py in the firmware tree; keep the two in step.

export type NotificationMessage = {
  t: 'notify';
  id: number;
  src?: string;
  title?: string;
  subject?: string;
  body?: string;
  sender?: string;
  tel?: string;
};

export type DismissNotificationMessage = { t: 'notify-'; id: number };

// One alarm as the watch keeps it. `rep` is a day mask, Monday 1 to Sunday 64, and 0 rings once.
export type Alarm = { h: number; m: number; on?: boolean; rep?: number };

// With `d` the watch replaces its alarms; without it the watch only reports them.
export type AlarmMessage = { t: 'alarm'; d?: Alarm[] };

export type FindWatchMessage = { t: 'find'; n: boolean };

// The number of short buzzes, which the watch keeps between 1 and 5.
export type VibrateMessage = { t: 'vibrate'; n: number };

// The watch settings the phone can change.
export type WatchSettingValues = {
  brightness: number;
  notify_level: number;
  units: 'Metric' | 'Imperial';
  blank_after: number;
  clock_24h: boolean;
  step_goal: number;
  theme: number[];
  face: string | null;
};

// With no other keys the watch only reports its settings.
export type SettingsMessage = { t: 'settings' } & Partial<WatchSettingValues>;

export type StepsRequestMessage = { t: 'steps'; y: number; m: number; d: number };

// The answer to a watch app's http request, matched by id.
export type HttpResponseMessage = { t: 'http'; id: string; resp?: string; err?: string };

export type WeatherMessage = {
  t: 'weather';
  temp: number;
  hum: number;
  txt: string;
  wind: number;
  loc: string;
};

export type MusicStateMessage = {
  t: 'musicstate';
  state: 'play' | 'pause';
  position?: number;
  shuffle?: number;
  repeat?: number;
};

export type MusicInfoMessage = {
  t: 'musicinfo';
  artist?: string;
  album?: string;
  track?: string;
  dur?: number;
  c?: number;
  n?: number;
};

export type CallMessage = {
  t: 'call';
  cmd: 'accept' | 'incoming' | 'outgoing' | 'reject' | 'start' | 'end';
  name?: string;
  number?: string;
};

// Everything the phone can send to the watch.
export type PhoneToWatchMessage =
  | NotificationMessage
  | DismissNotificationMessage
  | AlarmMessage
  | FindWatchMessage
  | VibrateMessage
  | WeatherMessage
  | MusicStateMessage
  | MusicInfoMessage
  | CallMessage
  | SettingsMessage
  | StepsRequestMessage
  | HttpResponseMessage;

export type MusicControlMessage = {
  t: 'music';
  n: 'play' | 'pause' | 'next' | 'previous' | 'volumeup' | 'volumedown';
};

export type FindPhoneMessage = { t: 'findPhone'; n: boolean | 'true' | 'false' };

export type InfoMessage = { t: 'info'; msg: string };

export type ErrorMessage = { t: 'error'; msg: string };

export type SettingsReply = { t: 'settings'; faces: [string, string][] } & WatchSettingValues;

export type AlarmReply = { t: 'alarm'; d: Alarm[] };

// A day's step counts in 6 minute slots, null when the watch has no log for it.
export type StepsReply = {
  t: 'steps';
  y: number;
  m: number;
  d: number;
  v: number[] | null;
  now: number;
};

// The watch's answer to an incoming call: pick up, hang up, or only stop buzzing.
export type CallControlMessage = { t: 'call'; n: 'ACCEPT' | 'REJECT' | 'IGNORE' };

// A watch app asking the phone to fetch a URL for it.
export type HttpRequestMessage = { t: 'http'; url: string; id: string };

// Everything the watch can send to the phone.
export type WatchToPhoneMessage =
  | MusicControlMessage
  | FindPhoneMessage
  | InfoMessage
  | ErrorMessage
  | SettingsReply
  | AlarmReply
  | StepsReply
  | CallControlMessage
  | HttpRequestMessage;

// JSON with every character outside printable ASCII escaped, since the watch's line editor drops those bytes.
export function asciiJson(value: unknown): string {
  return JSON.stringify(value).replace(
    /[\u007f-\uffff]/g,
    (char) => `\\u${char.charCodeAt(0).toString(16).padStart(4, '0')}`,
  );
}

// Send the message as a line of Python, which the watch REPL evaluates as a call to GB().
// Gadgetbridge prefixes this line with \x10, but that is an Espruino convention the watch
// ignores, and it would corrupt the line on a firmware built with the REPL history keys enabled.
export function encodeForWatch(message: PhoneToWatchMessage): string {
  return `GB(${asciiJson(message)})\r\n`;
}

// Turn one line of watch output into a message, or null when the line is not JSON.
export function decodeWatchLine(line: string): WatchToPhoneMessage | null {
  const trimmed = line.trim();
  if (!trimmed.startsWith('{')) {
    return null;
  }
  try {
    const parsed = JSON.parse(trimmed);
    if (parsed && typeof parsed.t === 'string') {
      return parsed as WatchToPhoneMessage;
    }
  } catch {
    // Not a complete JSON object, so treat it as plain console output.
  }
  return null;
}

// The watch sends findPhone with a string flag, so normalise it to a boolean.
export function isFindPhoneOn(message: FindPhoneMessage): boolean {
  return message.n === true || message.n === 'true';
}

// Collect raw UART chunks and hand back complete lines as they arrive.
export class LineBuffer {
  private pending = '';

  push(chunk: string): string[] {
    this.pending += chunk;
    const lines = this.pending.split(/\r?\n/);
    this.pending = lines.pop() ?? '';
    return lines.filter((line) => line.length > 0);
  }

  reset() {
    this.pending = '';
  }
}
