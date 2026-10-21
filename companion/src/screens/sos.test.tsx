/**
 * @jest-environment jsdom
 */
import { afterEach, beforeEach, describe, expect, it, jest } from '@jest/globals';
import { render } from 'preact';
import { act } from 'preact/test-utils';

import type { SosPermissions } from '@/services/phone/types';
import { defaultSettings, type Settings } from '@/state/settings';
import { IDLE_SOS, type SosStatus } from '@/state/sos';

import SosScreen from './sos';

// ============================================================================
// What the screen reads
// ============================================================================

const mockWatch = {
  settings: { ...defaultSettings } as Settings,
  updateSettings: jest.fn(async (patch: Partial<Settings>) => {
    mockWatch.settings = { ...mockWatch.settings, ...patch };
  }),
  sosStatus: IDLE_SOS as SosStatus,
  stopSos: jest.fn(async () => {}),
};

const ALLOWED: SosPermissions = { calls: true, texts: true, camera: true, microphone: true, overlay: true };

const mockPhone = {
  kind: 'phone' as 'phone' | 'computer',
  sosPermissions: jest.fn(async () => ALLOWED),
  requestSosPermissions: jest.fn(async () => ALLOWED),
  openOverlaySettings: jest.fn(async () => {}),
};

jest.mock('@/hooks/use-watch', () => ({ useWatch: () => mockWatch }));
jest.mock('@/services/phone', () => ({
  get phoneBridge() {
    return mockPhone;
  },
}));

// ============================================================================
// Helpers
// ============================================================================

let root: HTMLElement;

async function show() {
  await act(async () => {
    render(<SosScreen />, root);
  });
}

function text(): string {
  return root.textContent ?? '';
}

function button(title: string): HTMLButtonElement {
  const found = [...root.querySelectorAll('button')].find((each) => each.textContent === title);
  if (!found) throw new Error(`No button "${title}" in: ${text()}`);
  return found;
}

function type(label: string, value: string) {
  const input = root.querySelector(`input[aria-label="${label}"]`) as HTMLInputElement;
  input.value = value;
  input.dispatchEvent(new Event('input', { bubbles: true }));
}

beforeEach(() => {
  root = document.createElement('div');
  document.body.appendChild(root);
  mockWatch.settings = { ...defaultSettings };
  mockWatch.sosStatus = IDLE_SOS;
  mockPhone.kind = 'phone';
  mockPhone.sosPermissions.mockImplementation(async () => ALLOWED);
  jest.clearAllMocks();
});

afterEach(() => {
  render(null, root);
  root.remove();
});

// ============================================================================
// The screen
// ============================================================================

describe('SosScreen', () => {
  it('adds a contact from the two fields and refuses one without a number', async () => {
    await show();
    await act(async () => button('Add contact').click());
    expect(text()).toContain('Enter a phone number');
    expect(mockWatch.updateSettings).not.toHaveBeenCalled();

    await act(async () => {
      type('Name', 'Ann');
      type('Phone number', '555 0101');
    });
    await act(async () => button('Add contact').click());
    expect(mockWatch.updateSettings).toHaveBeenCalledWith({ sosContacts: [{ name: 'Ann', number: '555 0101' }] });
  });

  it('reorders and removes contacts', async () => {
    mockWatch.settings.sosContacts = [
      { name: 'Ann', number: '1111' },
      { name: '', number: '2222' },
    ];
    await show();
    expect(text()).toContain('1. Ann');
    expect(text()).toContain('2. 2222');
    await act(async () => button('Down').click());
    expect(mockWatch.updateSettings).toHaveBeenLastCalledWith({
      sosContacts: [
        { name: '', number: '2222' },
        { name: 'Ann', number: '1111' },
      ],
    });
    await act(async () => [...root.querySelectorAll('button')].filter((each) => each.textContent === 'Remove')[1].click());
    expect(mockWatch.updateSettings).toHaveBeenLastCalledWith({ sosContacts: [{ name: 'Ann', number: '1111' }] });
  });

  it('explains that the emergency number only opens the dialer', async () => {
    mockWatch.settings.sosCall = 'emergency';
    await show();
    expect(text()).toContain('the dialer opens with it typed in');
    expect((root.querySelector('input[aria-label="Emergency number"]') as HTMLInputElement).value).toBe('911');
  });

  it('shows a running SOS with its log and stops it', async () => {
    mockWatch.sosStatus = {
      active: true,
      startedAt: Date.now(),
      log: [
        { at: Date.now(), ok: false, message: 'Ann did not answer.' },
        { at: Date.now(), ok: true, message: 'Calling Ann.' },
      ],
    };
    await show();
    expect(text()).toContain('SOS on');
    expect(text()).toContain('Ann did not answer.');
    await act(async () => button('Stop SOS').click());
    expect(mockWatch.stopSos).toHaveBeenCalled();
  });

  it('lists what Android has not allowed and asks for it', async () => {
    mockPhone.sosPermissions.mockImplementation(async () => ({ ...ALLOWED, camera: false, overlay: false }));
    await show();
    expect(text()).toContain('Not allowed: Camera');
    expect(text()).toContain('Not allowed: Display over other apps');
    await act(async () => {
      button('Allow calls, texts and recording').click();
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
    expect(mockPhone.requestSosPermissions).toHaveBeenCalled();
    await act(async () => button('Allow display over other apps').click());
    expect(mockPhone.openOverlaySettings).toHaveBeenCalled();
  });

  it('says on a computer that only the Android app runs the SOS, and asks for nothing', async () => {
    mockPhone.kind = 'computer';
    await show();
    expect(text()).toContain('runs only in the Android app');
    expect(text()).not.toContain('Permissions');
  });
});
