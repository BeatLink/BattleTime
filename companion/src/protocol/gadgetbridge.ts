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

export type AlarmMessage = { t: 'alarm'; d: { h: number; m: number }[] };

export type FindWatchMessage = { t: 'find'; n: boolean };

export type VibrateMessage = { t: 'vibrate'; n: number };

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
  | CallMessage;

export type MusicControlMessage = {
  t: 'music';
  n: 'play' | 'pause' | 'next' | 'previous' | 'volumeup' | 'volumedown';
};

export type FindPhoneMessage = { t: 'findPhone'; n: boolean | 'true' | 'false' };

export type InfoMessage = { t: 'info'; msg: string };

export type ErrorMessage = { t: 'error'; msg: string };

// Everything the watch can send to the phone.
export type WatchToPhoneMessage = MusicControlMessage | FindPhoneMessage | InfoMessage | ErrorMessage;

// Send the message as a line of Python, which the watch REPL evaluates as a call to GB().
// Gadgetbridge prefixes this line with \x10, but that is an Espruino convention the watch
// ignores, and it would corrupt the line on a firmware built with the REPL history keys enabled.
export function encodeForWatch(message: PhoneToWatchMessage): string {
  return `GB(${JSON.stringify(message)})\r\n`;
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
