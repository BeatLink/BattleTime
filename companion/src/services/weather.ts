// Current conditions from Open-Meteo, which needs no account or key, shaped as the watch's weather message.

import type { WeatherMessage } from '@/protocol/gadgetbridge';

export type Place = { name: string; latitude: number; longitude: number };

type Fetch = typeof fetch;

// Open-Meteo reports conditions as WMO weather codes.
const CONDITIONS: [number, string][] = [
  [0, 'Clear'],
  [1, 'Mostly clear'],
  [2, 'Partly cloudy'],
  [3, 'Overcast'],
  [48, 'Fog'],
  [57, 'Drizzle'],
  [67, 'Rain'],
  [77, 'Snow'],
  [82, 'Showers'],
  [86, 'Snow showers'],
  [99, 'Thunderstorm'],
];

export function describeWeatherCode(code: number): string {
  for (const [upTo, label] of CONDITIONS) {
    if (code <= upTo) {
      return label;
    }
  }
  return 'Unknown';
}

// Find a place by name, taking the best match.
export async function findPlace(name: string, fetcher: Fetch = fetch): Promise<Place> {
  const url = `https://geocoding-api.open-meteo.com/v1/search?count=1&name=${encodeURIComponent(name.trim())}`;
  const response = await fetcher(url);
  if (!response.ok) {
    throw new Error(`Looking up ${name} failed (${response.status}).`);
  }
  const body = (await response.json()) as {
    results?: { name: string; country?: string; latitude: number; longitude: number }[];
  };
  const best = body.results?.[0];
  if (!best) {
    throw new Error(`No place called ${name} was found.`);
  }
  return { name: best.name, latitude: best.latitude, longitude: best.longitude };
}

// The place to report for: where the device is when asked and it can tell, otherwise the place chosen by name.
export async function weatherPlace(
  chosen: Place | null,
  locate: (() => Promise<Place>) | null,
): Promise<Place> {
  if (locate) {
    return locate();
  }
  if (!chosen) {
    throw new Error('Choose a place for the weather in Settings first.');
  }
  return chosen;
}

// Where the weather is reported for, in words for the screens, or null when there is nowhere yet.
export function describeWeatherSource(chosen: Place | null, fromDevice: boolean, device: string): string | null {
  if (fromDevice) {
    return `wherever this ${device} is`;
  }
  return chosen ? chosen.name : null;
}

// The OpenWeatherMap code for each WMO code, since the watch reads skies as Gadgetbridge sends them.
const OPEN_WEATHER_MAP_CODES: Record<number, number> = {
  0: 800,
  1: 801,
  2: 802,
  3: 804,
  45: 741,
  48: 741,
  51: 300,
  53: 301,
  55: 302,
  56: 310,
  57: 311,
  61: 500,
  63: 501,
  65: 502,
  66: 511,
  67: 511,
  71: 600,
  73: 601,
  75: 602,
  77: 600,
  80: 520,
  81: 521,
  82: 522,
  85: 620,
  86: 621,
  95: 211,
  96: 201,
  99: 202,
};

export function openWeatherMapCode(code: number): number | undefined {
  return OPEN_WEATHER_MAP_CODES[code];
}

// The watch takes whole kelvin and subtracts 273, so this gives back the rounded Celsius exactly.
export function wholeKelvin(celsius: number): number {
  return Math.round(celsius) + 273;
}

export async function currentWeather(place: Place, fetcher: Fetch = fetch): Promise<WeatherMessage> {
  const url =
    'https://api.open-meteo.com/v1/forecast?current=temperature_2m,relative_humidity_2m,weather_code,wind_speed_10m' +
    '&daily=temperature_2m_max,temperature_2m_min&forecast_days=1&timezone=auto' +
    `&latitude=${place.latitude}&longitude=${place.longitude}&wind_speed_unit=kmh`;
  const response = await fetcher(url);
  if (!response.ok) {
    throw new Error(`The weather service answered ${response.status}.`);
  }
  const body = (await response.json()) as {
    current?: {
      temperature_2m: number;
      relative_humidity_2m: number;
      weather_code: number;
      wind_speed_10m: number;
    };
    daily?: { temperature_2m_max?: number[]; temperature_2m_min?: number[] };
  };
  const now = body.current;
  if (!now) {
    throw new Error('The weather service sent no current conditions.');
  }
  // Today's range, widened to take in the current temperature, or just the current one when the day is missing.
  const high = Math.max(body.daily?.temperature_2m_max?.[0] ?? now.temperature_2m, now.temperature_2m);
  const low = Math.min(body.daily?.temperature_2m_min?.[0] ?? now.temperature_2m, now.temperature_2m);
  const code = openWeatherMapCode(now.weather_code);
  return {
    t: 'weather',
    temp: wholeKelvin(now.temperature_2m),
    hi: wholeKelvin(high),
    lo: wholeKelvin(low),
    ...(code === undefined ? {} : { code }),
    hum: Math.round(now.relative_humidity_2m),
    txt: describeWeatherCode(now.weather_code),
    wind: Math.round(now.wind_speed_10m),
    loc: place.name,
  };
}
