// Icons in the shared library, which apps refer to by number instead of carrying their own.
// The preview draws each one as a symbol; the watch draws the library's RGB565 image.

// ============================================================================
// Icons
// ============================================================================

const ICON_SYMBOLS = {
  alarm: "⏰",
  backspace: "⌫",
  battery: "🔋",
  bell: "🔔",
  bell_off: "🔕",
  bluetooth: "ᛒ",
  calculator: "🧮",
  charging: "⚡",
  chart: "📈",
  check: "✓",
  clock: "🕑",
  close: "✕",
  clouds: "☁",
  dice: "🎲",
  few_clouds: "⛅",
  flag: "⚑",
  flashlight: "🔦",
  game: "🎮",
  heart: "♥",
  heavy_clouds: "☁",
  level: "◎",
  clear: "☀",
  metronome: "♩",
  minus: "−",
  mist: "🌫",
  moon: "☾",
  music: "♫",
  navigation: "➤",
  next: "⏭",
  notification: "✉",
  pause: "⏸",
  phone: "📱",
  play: "▶",
  plus: "+",
  previous: "⏮",
  rain: "🌧",
  settings: "⚙",
  showers: "🌦",
  snow: "❄",
  steps: "👣",
  stop: "■",
  stopwatch: "⏱",
  sun: "☀",
  thunderstorm: "⛈",
  timer: "⌛",
  turn_arrive: "⚑",
  turn_left: "↰",
  turn_right: "↱",
  turn_roundabout: "⟳",
  turn_sharp_left: "↙",
  turn_sharp_right: "↘",
  turn_slight_left: "↖",
  turn_slight_right: "↗",
  turn_straight: "↑",
  turn_u_turn: "↶",
  volume_down: "🔉",
  volume_up: "🔊",
  weather: "⛅",
  weather_night_clear: "☾",
  weather_night_cloudy: "☁",
  none: " ",
  pine_logo: "🌲",
} as const;

export type IconName = keyof typeof ICON_SYMBOLS;

export interface LibraryIcon {
  readonly library_icon: IconName;
}

/** Every icon in the shared library, for example `icons.heart`. */
export const icons = Object.fromEntries(
  Object.keys(ICON_SYMBOLS).map((name) => [name, { library_icon: name }]),
) as { readonly [Name in IconName]: LibraryIcon };

/** Looks up an icon by name, for icons chosen by state such as a weather condition. */
export function icon_named(name: IconName): LibraryIcon {
  return icons[name];
}

export function icon_symbol(icon: LibraryIcon): string {
  return ICON_SYMBOLS[icon.library_icon];
}

export function is_icon(value: unknown): value is LibraryIcon {
  return typeof value === "object" && value !== null && "library_icon" in value;
}
