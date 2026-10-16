/**
 * @jest-environment jsdom
 */
import { afterEach, describe, expect, it, jest } from '@jest/globals';
import { render } from 'preact';
import { act } from 'preact/test-utils';

import { BatteryLimitsRow } from './battery-limits-row';

// ============================================================================
// Helpers
// ============================================================================

function phone(lifted: boolean, liftedAfterAsking: boolean) {
  return {
    isIgnoringBatteryOptimizations: jest.fn(async () => lifted),
    requestIgnoreBatteryOptimizations: jest.fn(async () => liftedAfterAsking),
  };
}

async function draw(bridge: ReturnType<typeof phone>): Promise<HTMLElement> {
  const host = document.createElement('div');
  document.body.appendChild(host);
  await act(async () => {
    render(<BatteryLimitsRow bridge={bridge} />, host);
  });
  return host;
}

afterEach(() => {
  document.body.innerHTML = '';
});

// ============================================================================
// Tests
// ============================================================================

describe('BatteryLimitsRow', () => {
  it('offers to lift the limits, and says once they are lifted', async () => {
    const bridge = phone(false, true);
    const host = await draw(bridge);
    expect(host.textContent).toContain('tap to lift them');

    await act(async () => {
      host.querySelector('button')!.click();
    });

    expect(bridge.requestIgnoreBatteryOptimizations).toHaveBeenCalled();
    expect(host.textContent).toContain('Lifted');
    expect(host.querySelector('button')).toBeNull();
  });

  it('is only a note when the limits are already lifted', async () => {
    const host = await draw(phone(true, true));

    expect(host.textContent).toContain('Lifted');
    expect(host.querySelector('button')).toBeNull();
  });
});
