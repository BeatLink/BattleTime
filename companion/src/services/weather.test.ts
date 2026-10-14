import { describe, expect, it, jest } from '@jest/globals';

import {
  currentWeather,
  describeWeatherCode,
  describeWeatherSource,
  findPlace,
  openWeatherMapCode,
  weatherPlace,
  wholeKelvin,
} from './weather';

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

describe('openWeatherMapCode', () => {
  it('gives the OpenWeatherMap code the watch reads for each WMO code', () => {
    expect(openWeatherMapCode(0)).toBe(800);
    expect(openWeatherMapCode(3)).toBe(804);
    expect(openWeatherMapCode(45)).toBe(741);
    expect(openWeatherMapCode(63)).toBe(501);
    expect(openWeatherMapCode(81)).toBe(521);
    expect(openWeatherMapCode(86)).toBe(621);
    expect(openWeatherMapCode(99)).toBe(202);
  });

  it('gives nothing for a code the table lacks', () => {
    expect(openWeatherMapCode(4)).toBeUndefined();
  });
});

describe('wholeKelvin', () => {
  it('gives a whole number the watch turns back into the rounded Celsius', () => {
    expect(wholeKelvin(0.4)).toBe(273);
    expect(wholeKelvin(20.6)).toBe(294);
    expect(wholeKelvin(-5.2)).toBe(268);
    expect(Number.isInteger(wholeKelvin(31.37))).toBe(true);
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
  it("asks for the place by its coordinates in km/h, with today's range", async () => {
    const { fetcher, urls } = answer(200, {
      current: { temperature_2m: 27.6, relative_humidity_2m: 90, weather_code: 61, wind_speed_10m: 0.4 },
      daily: { temperature_2m_max: [31.2], temperature_2m_min: [22.4] },
    });
    expect(await currentWeather(kingston, fetcher)).toEqual({
      t: 'weather',
      temp: 301,
      hi: 304,
      lo: 295,
      code: 500,
      hum: 90,
      txt: 'Rain',
      wind: 0,
      loc: 'Kingston',
    });
    expect(urls[0]).toContain('&daily=temperature_2m_max,temperature_2m_min&forecast_days=1&timezone=auto');
    expect(urls[0]).toContain('&latitude=18&longitude=-76.8&wind_speed_unit=kmh');
  });

  it('uses the current temperature for a missing range and leaves out an unknown sky', async () => {
    const { fetcher } = answer(200, {
      current: { temperature_2m: -5, relative_humidity_2m: 90, weather_code: 120, wind_speed_10m: 0.4 },
    });
    const message = await currentWeather(kingston, fetcher);
    expect(message).toMatchObject({ temp: 268, hi: 268, lo: 268, txt: 'Unknown' });
    expect(message).not.toHaveProperty('code');
  });

  it('widens the range to take in the current temperature', async () => {
    const { fetcher } = answer(200, {
      current: { temperature_2m: 35, relative_humidity_2m: 40, weather_code: 0, wind_speed_10m: 3 },
      daily: { temperature_2m_max: [33], temperature_2m_min: [36] },
    });
    expect(await currentWeather(kingston, fetcher)).toMatchObject({ temp: 308, hi: 308, lo: 308 });
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

describe('weatherPlace', () => {
  it('asks the device where it is when it can', async () => {
    const here = { name: 'Here', latitude: 1, longitude: 2 };
    await expect(weatherPlace(kingston, async () => here)).resolves.toBe(here);
  });

  it('uses the chosen place otherwise', async () => {
    await expect(weatherPlace(kingston, null)).resolves.toBe(kingston);
  });

  it('asks for a place when there is none', async () => {
    await expect(weatherPlace(null, null)).rejects.toThrow('Choose a place');
  });
});

describe('describeWeatherSource', () => {
  it('names the device, the chosen place, or nothing', () => {
    expect(describeWeatherSource(kingston, true, 'computer')).toBe('wherever this computer is');
    expect(describeWeatherSource(kingston, false, 'computer')).toBe('Kingston');
    expect(describeWeatherSource(null, false, 'phone')).toBeNull();
  });
});
