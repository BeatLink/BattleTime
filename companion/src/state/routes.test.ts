import { describe, expect, it } from '@jest/globals';

import { hashForRoute, isTabRoute, routeFromHash, STACK_TITLES, TAB_ROUTES } from './routes';

describe('routes', () => {
  it('reads the Watch tab from an empty hash or a bare one', () => {
    expect(routeFromHash('')).toBe('/');
    expect(routeFromHash('#')).toBe('/');
    expect(routeFromHash('#/')).toBe('/');
  });

  it('reads every tab and every screen back from the hash it makes', () => {
    for (const route of [...TAB_ROUTES, ...(Object.keys(STACK_TITLES) as (keyof typeof STACK_TITLES)[])]) {
      expect(routeFromHash(hashForRoute(route))).toBe(route);
    }
  });

  it('falls back to the Watch tab for an address it does not know', () => {
    expect(routeFromHash('#/nowhere')).toBe('/');
    expect(routeFromHash('#scan')).toBe('/');
  });

  it('tells the tabs from the screens opened over them', () => {
    expect(isTabRoute('/console')).toBe(true);
    expect(isTabRoute('/scan')).toBe(false);
  });
});
