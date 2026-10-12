import { describe, expect, it, jest } from '@jest/globals';

import { currentWeather, describeWeatherCode, findPlace } from './weather';

const kingston = { name: 'Kingston', latitude: 18, longitude: -76.8 };

// A fetch that answers every request with one status and JSON body, and records the URLs asked for.
function answer(status: number, body: unknown = {}) {
  const urls: string[] = [];
  const fetcher = (async (input: RequestInfo | URL) => {
    urls.push(String(input));
    return new Response(JSON.stringify(body), { status });
  }) as typeof fetch;
  return { fetcher, urls };
}

describe('describeWeatherCode', () => {
  it('names each WMO code band by its upper bound', () => {
    expect(describeWeatherCode(0)).toBe('Clear');
    expect(describeWeatherCode(45)).toBe('Fog');
    expect(describeWeatherCode(48)).toBe('Fog');
    expect(describeWeatherCode(49)).toBe('Drizzle');
    expect(describeWeatherCode(85)).toBe('Snow showers');
  });

  it('calls a code beyond the table unknown', () => {
    expect(describeWeatherCode(100)).toBe('Unknown');
  });
});

describe('findPlace', () => {
  it('asks for the trimmed, encoded name and takes the first match', async () => {
    const { fetcher, urls } = answer(200, {
      results: [{ name: 'São Paulo', country: 'Brazil', latitude: -23.5, longitude: -46.6 }],
    });
    await expect(findPlace('  São Paulo ', fetcher)).resolves.toEqual({
      name: 'São Paulo',
      latitude: -23.5,
      longitude: -46.6,
    });
    expect(urls[0]).toBe('https://geocoding-api.open-meteo.com/v1/search?count=1&name=S%C3%A3o%20Paulo');
  });

  it('reports a failed lookup with its status', async () => {
    await expect(findPlace('Kingston', answer(503).fetcher)).rejects.toThrow('Looking up Kingston failed (503).');
  });

  it('reports a name with no match', async () => {
    await expect(findPlace('Nowhere', answer(200, {}).fetcher)).rejects.toThrow('No place called Nowhere was found.');
    await expect(findPlace('Nowhere', answer(200, { results: [] }).fetcher)).rejects.toThrow('No place called');
  });
});

describe('currentWeather', () => {
  it('asks for the place by its coordinates in km/h', async () => {
    const { fetcher, urls } = answer(200, {
      current: { temperature_2m: -5, relative_humidity_2m: 90, weather_code: 120, wind_speed_10m: 0.4 },
    });
    expect(await currentWeather(kingston, fetcher)).toEqual({
      t: 'weather',
      temp: 268.2,
      hum: 90,
      txt: 'Unknown',
      wind: 0,
      loc: 'Kingston',
    });
    expect(urls[0]).toContain('&latitude=18&longitude=-76.8&wind_speed_unit=kmh');
  });

  it('reports a failed forecast with its status', async () => {
    await expect(currentWeather(kingston, answer(429).fetcher)).rejects.toThrow('The weather service answered 429.');
  });

  it('reports a forecast with no current conditions', async () => {
    await expect(currentWeather(kingston, answer(200, {}).fetcher)).rejects.toThrow(
      'The weather service sent no current conditions.',
    );
  });
});

describe('without a fetcher given', () => {
  it('uses the global fetch', async () => {
    const spy = jest.spyOn(globalThis, 'fetch').mockImplementation(async (input) =>
      String(input).includes('geocoding')
        ? new Response(JSON.stringify({ results: [kingston] }))
        : new Response(
            JSON.stringify({
              current: { temperature_2m: 20, relative_humidity_2m: 50, weather_code: 0, wind_speed_10m: 5 },
            }),
          ),
    );
    try {
      const place = await findPlace('Kingston');
      expect((await currentWeather(place)).txt).toBe('Clear');
      expect(spy).toHaveBeenCalledTimes(2);
    } finally {
      spy.mockRestore();
    }
  });
});
