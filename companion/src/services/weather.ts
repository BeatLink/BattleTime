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

export async function currentWeather(place: Place, fetcher: Fetch = fetch): Promise<WeatherMessage> {
  const url =
    'https://api.open-meteo.com/v1/forecast?current=temperature_2m,relative_humidity_2m,weather_code,wind_speed_10m' +
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
  };
  const now = body.current;
  if (!now) {
    throw new Error('The weather service sent no current conditions.');
  }
  return {
    t: 'weather',
    // The watch takes Kelvin, as Gadgetbridge sends it.
    temp: Math.round((now.temperature_2m + 273.15) * 10) / 10,
    hum: Math.round(now.relative_humidity_2m),
    txt: describeWeatherCode(now.weather_code),
    wind: Math.round(now.wind_speed_10m),
    loc: place.name,
  };
}
