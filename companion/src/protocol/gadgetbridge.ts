// Message types for the Gadgetbridge (Bangle.js style) JSON lines BattleTime speaks over the Nordic UART Service.
// The watch side is described in docs/companion-protocol.md in the BattleTime tree; keep the two in step.

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

// Says the phone has just rewritten `/apps/<app>/state`, so the watch takes the new state up.
export type AppStateMessage = { t: 'state'; app: string };

export type FindWatchMessage = { t: 'find'; n: boolean };

// The number of short buzzes, which the watch keeps between 1 and 5.
export type VibrateMessage = { t: 'vibrate'; n: number };

// The eight theme colours, each "#rrggbb".
export const THEME_COLOR_KEYS = ['background', 'surface', 'text', 'muted', 'accent', 'success', 'warning', 'danger'] as const;
export type ThemeColorKey = (typeof THEME_COLOR_KEYS)[number];
export type ThemeColors = Record<ThemeColorKey, string>;

// The number the watch_face setting holds for a face: the 32-bit FNV-1a hash of its id with the top bit cleared, never 0.
export function watchFaceNumber(id: string): number {
  let hash = 0x811c9dc5;
  for (const byte of new TextEncoder().encode(id)) {
    hash = Math.imul(hash ^ byte, 0x01000193) >>> 0;
  }
  return hash & 0x7fffffff || 1;
}

// The watch settings the phone can change. The theme and brightness are always there; firmware older
// than the settings store leaves the rest out.
export type WatchSettingValues = {
  brightness: number;
  // A preset's name, or "custom" when the colours match none.
  theme: string;
  theme_colors: ThemeColors;
  blank_after?: number;
  clock_24h?: boolean;
  units?: 'Metric' | 'Imperial';
  step_goal?: number;
  always_on?: boolean;
  // 0 off, 1 hourly, 2 half-hourly.
  chimes?: 0 | 1 | 2;
  do_not_disturb?: boolean;
  raise_to_wake?: boolean;
  lower_to_sleep?: boolean;
  shake_to_wake?: boolean;
  // How fast a shake must be to wake the screen, 0 to 4095.
  shake_threshold?: number;
  // Do not disturb that also stops every wake gesture and the always-on screen; it turns do_not_disturb on.
  sleep_mode?: boolean;
  // The face to show, by the number watchFaceNumber gives for its id; 0 chooses none.
  watch_face?: number;
  // Whether the watch takes firmware updates and app installs.
  updates?: boolean;
  // True turns updates on only until the watch next restarts; it wins over updates when both are sent.
  updates_until_restart?: boolean;
  // True makes the watch advertise as "InfiniTime", so Gadgetbridge offers its InfiniTime support.
  gadgetbridge_name?: boolean;
};

// With no other keys the watch only reports its settings. Colours given are applied after the preset.
export type SettingsMessage = { t: 'settings' } & Partial<Omit<WatchSettingValues, 'theme_colors'>> & {
  theme_colors?: Partial<ThemeColors>;
};

export type StatusRequestMessage = { t: 'status' };

// A slot as [start, size] in bytes in the watch's external flash.
export type SlotRange = [number, number];

// With neither key the watch only reports where its update and recovery slots are; a slot given moves.
export type SlotsMessage = { t: 'slots'; update?: SlotRange; recovery?: SlotRange };

// The answer to a watch app's http request, matched by id.
export type HttpResponseMessage = { t: 'http'; id: string; resp?: string; err?: string };

export type WeatherMessage = {
  t: 'weather';
  temp: number;
  hi: number;
  lo: number;
  code?: number;
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

// The next step of the phone's navigation, as Gadgetbridge's Bangle.js driver sends it; with none of
// the step's fields it says navigation ended.
export type NavigationMessage = {
  t: 'nav';
  instr?: string;
  // As the phone shows it, such as "300 m" or "1.2 km".
  distance?: string;
  action?: string;
  eta?: string;
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
  | FindWatchMessage
  | VibrateMessage
  | WeatherMessage
  | MusicStateMessage
  | MusicInfoMessage
  | NavigationMessage
  | CallMessage
  | SettingsMessage
  | StatusRequestMessage
  | SlotsMessage
  | HttpResponseMessage
  | AppStateMessage;

export type MusicControlMessage = {
  t: 'music';
  n: 'play' | 'pause' | 'next' | 'previous' | 'volumeup' | 'volumedown';
};

export type FindPhoneMessage = { t: 'findPhone'; n: boolean | 'true' | 'false' };

// A watch app asking for the music or the navigation step again, which the phone answers by sending it.
export type StateRequestMessage = { t: 'request'; n: 'music' | 'nav' };

export type InfoMessage = { t: 'info'; msg: string };

export type ErrorMessage = { t: 'error'; msg: string };

// Every settings message is answered with all the settings, and the presets the watch offers.
export type SettingsReply = { t: 'settings'; themes: string[] } & WatchSettingValues;

// The battery level in percent and millivolts, and today's step count, null without a step counter.
export type StatusReply = {
  t: 'status';
  bat: number;
  mv: number;
  charging: boolean;
  steps: number | null;
};

// Where the slots are, the limits a move keeps to (slots end by `area`, the update slot is `firmware`
// plus one page) and the recovery image's length with its header, 0 for none. `restart` is true when
// this answers a move: the watch restarts to use the new slots. A refusal carries only `err`.
export type SlotsReply =
  | {
      t: 'slots';
      update: SlotRange;
      recovery: SlotRange;
      area: number;
      firmware: number;
      image: number;
      restart: boolean;
    }
  | { t: 'slots'; err: string };

// The watch's answer to an incoming call: pick up, hang up, or only stop buzzing.
export type CallControlMessage = { t: 'call'; n: 'ACCEPT' | 'REJECT' | 'IGNORE' };

// A watch app asking the phone to fetch a URL for it; a plain GET carries no method or body.
export type HttpRequestMessage = { t: 'http'; url: string; id: string; method?: string; body?: string };

// A watch app speaking on its own channel, such as the remote app's {"t":"app","ch":"remote","d":{"button":2}};
// read it with readAppMessage in app-message.ts.
export type AppChannelMessage = { t: 'app'; ch: string; d?: unknown };

// Everything the watch can send to the phone.
export type WatchToPhoneMessage =
  | MusicControlMessage
  | FindPhoneMessage
  | StateRequestMessage
  | InfoMessage
  | ErrorMessage
  | SettingsReply
  | StatusReply
  | SlotsReply
  | CallControlMessage
  | HttpRequestMessage
  | AppChannelMessage;

// JSON with every character outside printable ASCII escaped, since the watch's line editor drops those bytes.
export function asciiJson(value: unknown): string {
  return JSON.stringify(value).replace(
    /[\u007f-\uffff]/g,
    (char) => `\\u${char.charCodeAt(0).toString(16).padStart(4, '0')}`,
  );
}

// Send the message as the line GB({...}), as Gadgetbridge's Bangle.js driver does, so the watch serves both.
// Gadgetbridge also prefixes the line with \x10, an Espruino convention the watch does not need.
export function encodeForWatch(message: PhoneToWatchMessage): string {
  return `GB(${asciiJson(message)})\n`;
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

// What the phone knows of the music playing, as the media events report it.
export type PlayingMedia = {
  playing: boolean;
  artist?: string;
  album?: string;
  track?: string;
  durationSec?: number;
  positionSec?: number;
};

// The music lines the phone sends the watch for what is playing.
export function musicMessages(media: PlayingMedia): PhoneToWatchMessage[] {
  return [
    { t: 'musicinfo', artist: media.artist, album: media.album, track: media.track, dur: media.durationSec },
    { t: 'musicstate', state: media.playing ? 'play' : 'pause', position: media.positionSec },
  ];
}

// The answer to a watch's request: the music when it is forwarded and known, and for navigation an
// empty step, since this app follows no navigation, which tells the watch there is none.
export function answerStateRequest(
  request: StateRequestMessage,
  media: PlayingMedia | null,
  forwardMusic: boolean,
): PhoneToWatchMessage[] {
  if (request.n === 'nav') {
    return [{ t: 'nav' }];
  }
  return forwardMusic && media ? musicMessages(media) : [];
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
