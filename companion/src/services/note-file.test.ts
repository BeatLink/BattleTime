import { afterEach, describe, expect, it, jest } from '@jest/globals';

import { chooseFiles } from './device-files';
import { bridgeNoteAccess, noteFileAccess, pickerNoteAccess, type NoteBridge } from './note-file';

jest.mock('./device-files', () => ({ chooseFiles: jest.fn() }));

const NOTE = { name: 'note.txt', uri: '/home/me/note.txt' };

// A bridge like tauri/src/bridge.js's, with the core's answers given by the test.
function fakeBridge() {
  let listener: ((id: number) => void) | null = null;
  const unwatched: number[] = [];
  const bridge: NoteBridge = {
    pick: async () => ({ ok: true, value: NOTE }),
    read: async (path) =>
      path === NOTE.uri ? { ok: true, value: new TextEncoder().encode('Milk') } : { ok: false, error: 'ENOENT' },
    watch: async () => ({ ok: true, value: 7 }),
    unwatch: async (id) => {
      unwatched.push(id);
      return { ok: true };
    },
    onChange: (next) => {
      listener = next;
      return () => {
        listener = null;
      };
    },
  };
  return { bridge, unwatched, change: (id: number) => listener?.(id) };
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

afterEach(() => {
  delete (globalThis as { neotime?: unknown }).neotime;
});

describe('the note file in the app', () => {
  it('picks and reads the file through the bridge', async () => {
    const access = bridgeNoteAccess(fakeBridge().bridge);

    expect(await access.pick()).toEqual(NOTE);
    expect(new TextDecoder().decode(await access.read(NOTE))).toBe('Milk');
    await expect(access.read({ name: 'gone', uri: '/gone' })).rejects.toThrow('ENOENT');
  });

  it('passes on only its own file changes, until stopped', async () => {
    const fake = fakeBridge();
    const changes: number[] = [];
    const stop = bridgeNoteAccess(fake.bridge).watch(NOTE, () => changes.push(1));
    await settle();

    fake.change(3);
    fake.change(7);
    stop?.();
    fake.change(7);

    expect(changes).toEqual([1]);
    expect(fake.unwatched).toEqual([7]);
  });

  it('stops a watch that is only set up after it was stopped', async () => {
    const fake = fakeBridge();
    const stop = bridgeNoteAccess(fake.bridge).watch(NOTE, () => undefined);

    stop?.();
    await settle();

    expect(fake.unwatched).toEqual([7]);
  });

  it('is the access used where the bridge is', () => {
    (globalThis as { neotime?: unknown }).neotime = { notes: fakeBridge().bridge };
    expect(noteFileAccess()).not.toBe(pickerNoteAccess);
    delete (globalThis as { neotime?: unknown }).neotime;
    expect(noteFileAccess()).toBe(pickerNoteAccess);
  });
});

describe('the note file in a plain browser', () => {
  const choose = jest.mocked(chooseFiles);

  it('keeps a link to the chosen file, or nothing when cancelled', async () => {
    const file = { name: 'list.txt' } as File;
    const createObjectURL = jest.fn(() => 'blob:list');
    (URL as { createObjectURL?: unknown }).createObjectURL = createObjectURL;
    choose.mockResolvedValueOnce([file]);
    choose.mockResolvedValueOnce([]);

    expect(await pickerNoteAccess.pick()).toEqual({ name: 'list.txt', uri: 'blob:list' });
    expect(await pickerNoteAccess.pick()).toBeNull();
    expect(choose).toHaveBeenCalledWith({ accept: 'text/*' });
    expect(createObjectURL).toHaveBeenCalledWith(file);
    expect(pickerNoteAccess.watch(NOTE, () => undefined)).toBeNull();
  });

  it('reads the file again, and asks for it again when it cannot', async () => {
    const realFetch = globalThis.fetch;
    globalThis.fetch = (async (uri: string) => {
      if (uri !== 'blob:list') {
        throw new Error('no access');
      }
      return { arrayBuffer: async () => new TextEncoder().encode('Eggs').buffer } as Response;
    }) as typeof fetch;
    try {
      const read = await pickerNoteAccess.read({ name: 'list.txt', uri: 'blob:list' });
      expect(new TextDecoder().decode(read)).toBe('Eggs');
      await expect(pickerNoteAccess.read({ name: 'old.txt', uri: 'blob:old' })).rejects.toThrow(
        'old.txt could not be read; choose it again.',
      );
    } finally {
      globalThis.fetch = realFetch;
    }
  });
});
