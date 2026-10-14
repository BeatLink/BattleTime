/**
 * @jest-environment jsdom
 */
import { beforeEach, describe, expect, it, jest } from '@jest/globals';
import { createElement, render } from 'preact';
import { act } from 'preact/test-utils';

import { ENTER_DFU_COMMAND, flashFirmware, type FlashStep } from '@/dfu/flash';
import type { DfuLink } from '@/dfu/link';
import { readFirmwarePackage, type FirmwarePackage } from '@/dfu/package';
import { setWatchTime } from '@/protocol/neotime';
import { pickFile } from '@/services/device-files';

import { useFirmware } from './use-firmware';
import { useTask } from './use-task';

jest.mock('@/services/device-files', () => ({ pickFile: jest.fn() }));
jest.mock('@/dfu/package', () => ({ readFirmwarePackage: jest.fn() }));
jest.mock('@/protocol/neotime', () => ({ setWatchTime: jest.fn() }));
jest.mock('@/dfu/flash', () => ({
  ...jest.requireActual<typeof import('@/dfu/flash')>('@/dfu/flash'),
  flashFirmware: jest.fn(),
}));

const pick = jest.mocked(pickFile);
const parse = jest.mocked(readFirmwarePackage);
const flashMock = jest.mocked(flashFirmware);

// Runs a hook inside a throwaway component so the test can read what it returns.
function renderHook<T>(hook: () => T): { current: T } {
  const result = {} as { current: T };
  function Probe() {
    result.current = hook();
    return null;
  }
  act(() => {
    render(createElement(Probe, null), document.createElement('div'));
  });
  return result;
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => (resolve = done));
  return { promise, resolve };
}

describe('useTask', () => {
  it('shows the label while the action runs and returns its result', async () => {
    const task = renderHook(useTask);
    const gate = deferred<number>();
    let running!: Promise<number | undefined>;
    act(() => {
      running = task.current.run('Syncing', () => gate.promise);
    });
    expect(task.current.busy).toBe('Syncing');
    await act(async () => gate.resolve(42));
    await expect(running).resolves.toBe(42);
    expect(task.current.busy).toBeNull();
    expect(task.current.error).toBeNull();
  });

  it('keeps the failure message until it is cleared', async () => {
    const task = renderHook(useTask);
    let value: unknown = 'unset';
    await act(async () => {
      value = await task.current.run('Syncing', () => Promise.reject(new Error('watch went away')));
    });
    expect(value).toBeUndefined();
    expect(task.current.error).toBe('watch went away');
    expect(task.current.busy).toBeNull();
    act(() => task.current.clearError());
    expect(task.current.error).toBeNull();
  });

  it('refuses a second action while one is running', async () => {
    const task = renderHook(useTask);
    const gate = deferred<string>();
    const second = jest.fn(async () => 'second');
    let first!: Promise<string | undefined>;
    let refused: unknown = 'unset';
    await act(async () => {
      first = task.current.run('First', () => gate.promise);
      refused = await task.current.run('Second', second);
    });
    expect(refused).toBeUndefined();
    expect(second).not.toHaveBeenCalled();
    expect(task.current.busy).toBe('First');
    await act(async () => gate.resolve('first'));
    await expect(first).resolves.toBe('first');
  });

  it('clears an old error when a new action starts', async () => {
    const task = renderHook(useTask);
    await act(async () => {
      await task.current.run('Fail', () => Promise.reject(new Error('boom')));
      await task.current.run('Pass', async () => 1);
    });
    expect(task.current.error).toBeNull();
  });
});

describe('useFirmware', () => {
  const firmware: FirmwarePackage = {
    name: 'firmware.zip',
    image: { part: 'application', init: new Uint8Array(1), image: new Uint8Array(4) },
  };
  const link = {} as DfuLink;
  const sendRaw = jest.fn(async (_text: string) => {});

  function renderFirmware(dfu: DfuLink | null = link) {
    return renderHook(() => useFirmware(() => dfu, sendRaw));
  }

  // Makes the next update report one step and then end.
  function flashReports(step: FlashStep) {
    flashMock.mockImplementationOnce(async (_link, _firmware, options) => {
      options?.onStep?.(step);
    });
  }

  beforeEach(() => {
    jest.clearAllMocks();
  });

  it('takes a package and its size', () => {
    const state = renderFirmware();
    act(() => state.current.load(firmware));
    expect(state.current.firmware).toBe(firmware);
    expect(state.current.total).toBe(4);
    expect(state.current.sent).toBe(0);
    expect(state.current.finished).toBe(false);
  });

  it('reads and parses the file the user picks', async () => {
    pick.mockResolvedValueOnce({ name: 'fw.zip', data: new Uint8Array([9, 8]) });
    parse.mockReturnValueOnce(firmware);
    const state = renderFirmware();
    await act(() => state.current.choose());
    expect(pick).toHaveBeenCalledWith(expect.stringContaining('.zip'));
    expect(parse).toHaveBeenCalledWith(new Uint8Array([9, 8]), 'fw.zip');
    expect(state.current.firmware).toBe(firmware);
  });

  it('does nothing when the picker is cancelled', async () => {
    pick.mockResolvedValueOnce(null);
    const state = renderFirmware();
    await act(() => state.current.choose());
    expect(parse).not.toHaveBeenCalled();
    expect(state.current.firmware).toBeNull();
    expect(state.current.error).toBeNull();
  });

  it('drops the package and shows why when the picked file is not firmware', async () => {
    pick.mockResolvedValue({ name: 'x', data: new Uint8Array([1]) });
    parse.mockReturnValueOnce(firmware);
    parse.mockImplementationOnce(() => {
      throw new Error('No manifest.json in x');
    });
    const state = renderFirmware();
    await act(() => state.current.choose());
    await act(() => state.current.choose());
    expect(state.current.firmware).toBeNull();
    expect(state.current.error).toBe('No manifest.json in x');
    act(() => state.current.clearError());
    expect(state.current.error).toBeNull();
  });

  it('asks for a watch and a package before flashing', async () => {
    const noLink = renderFirmware(null);
    act(() => noLink.current.load(firmware));
    await act(() => noLink.current.flash());
    expect(noLink.current.error).toBe('Connect to a watch and pick a firmware package first.');

    const noPackage = renderFirmware();
    await act(() => noPackage.current.flash());
    expect(noPackage.current.error).toBe('Connect to a watch and pick a firmware package first.');
    expect(flashMock).not.toHaveBeenCalled();
  });

  it('resets wasp-os over the UART line for the reloader and sets the time to confirm the update', async () => {
    flashMock.mockImplementationOnce(async (dfu, _firmware, options) => {
      await options?.enterDfu?.();
      await options?.confirm?.(dfu);
    });
    const state = renderFirmware();
    act(() => state.current.load(firmware));
    await act(() => state.current.flash());
    expect(flashMock).toHaveBeenCalledWith(link, firmware, expect.anything());
    expect(sendRaw).toHaveBeenCalledWith(ENTER_DFU_COMMAND);
    expect(setWatchTime).toHaveBeenCalledWith(link);
    expect(state.current.flashing).toBe(false);
  });

  const part = (phase: string) => ({ part: 'application', phase, sent: 0, total: 4 }) as never;

  it.each<[FlashStep, string]>([
    [{ step: 'resetting' }, 'Restarting wasp-os in its bootloader'],
    [{ step: 'connecting' }, 'Waiting for the bootloader'],
    [{ step: 'detected' }, 'Talking to the watch'],
    [{ step: 'progress', progress: part('preparing') }, 'Preparing application'],
    [{ step: 'progress', progress: part('sending') }, 'Sending application'],
    [{ step: 'progress', progress: part('validating') }, 'Checking application'],
    [{ step: 'progress', progress: part('done') }, 'Sent application'],
    [{ step: 'progress', progress: part('unheard-of') }, 'Sending application'],
    [
      { step: 'reconnecting', attempt: 1, attempts: 4 },
      'Lost the watch; reconnecting to carry on (try 1 of 4)',
    ],
    [{ step: 'confirming' }, 'Waiting for the new firmware to start'],
    [{ step: 'finished', confirmed: true }, 'The new firmware is running and kept'],
    [
      { step: 'finished', confirmed: false },
      'Sent. Connect once the watch restarts, or it goes back to the old firmware',
    ],
  ])('describes step %j as it happens', async (step, status) => {
    flashReports(step);
    const state = renderFirmware();
    act(() => state.current.load(firmware));
    await act(() => state.current.flash());
    expect(state.current.status).toBe(status);
  });

  it('counts the bytes sent so far', async () => {
    flashReports({
      step: 'progress',
      progress: { part: 'application', phase: 'sending', sent: 2, total: 4 },
    });
    const state = renderFirmware();
    act(() => state.current.load(firmware));
    await act(() => state.current.flash());
    expect(state.current.sent).toBe(2);
    expect(state.current.finished).toBe(false);
  });

  it('marks the update finished when the last step arrives', async () => {
    flashReports({ step: 'finished', confirmed: true });
    const state = renderFirmware();
    act(() => state.current.load(firmware));
    await act(() => state.current.flash());
    expect(state.current.finished).toBe(true);
    expect(state.current.flashing).toBe(false);
  });

  it('shows why an update failed and clears the status', async () => {
    flashMock.mockImplementationOnce(async (_link, _firmware, options) => {
      options?.onStep?.({ step: 'detected' });
      throw new Error('The watch stopped answering');
    });
    const state = renderFirmware();
    act(() => state.current.load(firmware));
    await act(() => state.current.flash());
    expect(state.current.error).toBe('The watch stopped answering');
    expect(state.current.status).toBeNull();
    expect(state.current.flashing).toBe(false);
  });

  it('shows the update as running until it ends', async () => {
    const gate = deferred<void>();
    flashMock.mockImplementationOnce(() => gate.promise);
    const state = renderFirmware();
    act(() => state.current.load(firmware));
    let flashing!: Promise<void>;
    act(() => {
      flashing = state.current.flash();
    });
    expect(state.current.flashing).toBe(true);
    await act(async () => {
      gate.resolve();
      await flashing;
    });
    expect(state.current.flashing).toBe(false);
  });
});
