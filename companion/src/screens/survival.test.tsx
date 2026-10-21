/**
 * @jest-environment jsdom
 */
import '@/test-support/jsdom-text-encoding';

import { afterEach, describe, expect, it, jest } from '@jest/globals';
import { render } from 'preact';
import { act } from 'preact/test-utils';

import { packSurvivalList, parseSurvivalList, SURVIVAL_LIST_PATH } from '@/protocol/survival';

import SurvivalScreen from './survival';

// The watch's files, shared with the mocked hook.
const mockWatchFiles = new Map<string, Uint8Array>();

jest.mock('@/hooks/use-watch', () => {
  const { FileMissingError } = jest.requireActual<typeof import('@/protocol/file-service')>('@/protocol/file-service');
  return {
    useWatch: () => ({
      files: async () => ({
        read: async (path: string) => {
          const contents = mockWatchFiles.get(path);
          if (!contents) throw new FileMissingError(path);
          return contents;
        },
        write: async (path: string, contents: Uint8Array) => {
          mockWatchFiles.set(path, contents);
        },
        remove: async (path: string) => {
          if (!mockWatchFiles.delete(path)) throw new FileMissingError(path);
        },
        list: async () =>
          [...mockWatchFiles.entries()].map(([path, contents]) => ({
            name: path,
            path,
            size: contents.length,
            directory: false,
          })),
      }),
      watchReady: true,
      capabilities: { files: true },
    }),
  };
});

// ============================================================================
// Helpers
// ============================================================================

async function settle(): Promise<void> {
  for (let round = 0; round < 10; round++) {
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
  }
}

async function draw(): Promise<HTMLElement> {
  const host = document.createElement('div');
  document.body.appendChild(host);
  await act(async () => {
    render(<SurvivalScreen />, host);
  });
  await settle();
  return host;
}

function button(host: HTMLElement, text: string): HTMLButtonElement {
  const found = [...host.querySelectorAll('button')].find((candidate) => candidate.textContent?.includes(text));
  if (!found) throw new Error(`No button with "${text}" in: ${host.textContent}`);
  return found;
}

function toggle(host: HTMLElement, label: string): HTMLInputElement {
  const found = host.querySelector<HTMLInputElement>(`input[aria-label="${label}"]`);
  if (!found) throw new Error(`No switch for "${label}"`);
  return found;
}

afterEach(() => {
  document.body.innerHTML = '';
  mockWatchFiles.clear();
});

// ============================================================================
// Tests
// ============================================================================

describe('the survival screen', () => {
  it('ticks the guides the watch already holds', async () => {
    mockWatchFiles.set(SURVIVAL_LIST_PATH, packSurvivalList([{ slot: 1, topic: 0, title: 'Severe bleeding' }]));
    const host = await draw();

    expect(toggle(host, 'Severe bleeding').checked).toBe(true);
    expect(toggle(host, 'CPR').checked).toBe(false);
    expect(host.textContent).toContain('The watch already holds exactly these guides.');
  });

  it('sends the ticked guides to the watch', async () => {
    const host = await draw();

    await act(async () => toggle(host, 'CPR').click());
    await act(async () => toggle(host, 'Lost in the wild').click());
    await act(async () => button(host, 'Send 2 to the watch').click());
    await settle();

    expect(parseSurvivalList(mockWatchFiles.get(SURVIVAL_LIST_PATH)!)).toEqual([
      { slot: 0, topic: 0, title: 'CPR' },
      { slot: 9, topic: 1, title: 'Lost in the wild' },
    ]);
    expect(mockWatchFiles.has('/survival/0')).toBe(true);
    expect(host.textContent).toContain('The watch now holds 2 guides; 2 sent, 0 removed.');
  });

  it('takes every guide off once all are cleared', async () => {
    mockWatchFiles.set(SURVIVAL_LIST_PATH, packSurvivalList([{ slot: 0, topic: 0, title: 'CPR' }]));
    mockWatchFiles.set('/survival/0', new Uint8Array(5));
    const host = await draw();

    await act(async () => button(host, 'Clear').click());
    await act(async () => button(host, 'Take every guide off').click());
    await settle();

    expect(mockWatchFiles.size).toBe(0);
    expect(host.textContent).toContain('Took every guide off the watch.');
  });
});
