import { describe, expect, it } from '@jest/globals';

import type { DfuLink, WriteMode } from '@/dfu/link';
import { MockLegacyDfuTarget } from '@/dfu/mock-legacy-target';

import {
  COMMAND,
  FileMissingError,
  FileService,
  FileServiceError,
  LONGEST_WRITE,
  STATUS,
  checkPath,
  joinPath,
  makeDirectoryCommand,
  moveCommand,
  parentPath,
  pathCommand,
  readCommand,
  writeCommand,
  writeDataCommand,
} from './file-service';
import { FILE_TRANSFER, FILE_TRANSFER_SERVICE } from './neotime';

// ============================================================================
// Watches to talk to
// ============================================================================

function mockWatch() {
  return new MockLegacyDfuTarget('AA:BB:CC:DD:EE:00', 'neotime', true);
}

// A watch that answers every command with whatever the test hands it, and records what was written.
class ScriptedWatch implements DfuLink {
  readonly deviceId = 'AA:BB:CC:DD:EE:01';
  readonly written: Uint8Array[] = [];
  private listener: ((value: Uint8Array) => void) | null = null;

  constructor(private readonly script: (command: Uint8Array) => Uint8Array[]) {}

  async hasCharacteristic(): Promise<boolean> {
    return true;
  }

  async read(): Promise<Uint8Array> {
    return new Uint8Array(0);
  }

  async write(service: string, characteristic: string, data: Uint8Array, mode: WriteMode): Promise<void> {
    expect([service, characteristic, mode]).toEqual([FILE_TRANSFER_SERVICE, FILE_TRANSFER, 'request']);
    this.written.push(data);
    for (const answer of this.script(data)) {
      this.listener?.(answer);
    }
  }

  async subscribe(_service: string, _characteristic: string, onValue: (value: Uint8Array) => void) {
    this.listener = onValue;
    return () => {
      this.listener = null;
    };
  }

  async reconnect(): Promise<void> {}

  get subscribed() {
    return this.listener !== null;
  }
}

function answer(command: number, status: number, length = 20): Uint8Array {
  const bytes = new Uint8Array(length);
  bytes[0] = command;
  bytes[1] = status;
  return bytes;
}

function u32At(bytes: Uint8Array, at: number) {
  return new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).getUint32(at, true);
}

const text = (value: string) => new TextEncoder().encode(value);

// ============================================================================
// Paths and commands
// ============================================================================

describe('paths', () => {
  it('joins, finds parents and refuses relative or climbing paths', () => {
    expect(joinPath('/', 'apps')).toBe('/apps');
    expect(joinPath('/apps', 'clock')).toBe('/apps/clock');
    expect(parentPath('/apps/clock')).toBe('/apps');
    expect(parentPath('/apps')).toBe('/');
    expect(checkPath('/apps')).toBe('/apps');
    expect(() => checkPath('apps')).toThrow('absolute path');
    expect(() => checkPath('/apps/../x')).toThrow('absolute path');
  });
});

describe('commands', () => {
  it('lays out each command as the protocol describes', () => {
    expect([...readCommand(text('/a'), 5, 100)]).toEqual([0x10, 0, 2, 0, 5, 0, 0, 0, 100, 0, 0, 0, 47, 97]);
    expect([...readCommand(text('/a'), 5, 100, true)]).toEqual([0x12, 1, 0, 0, 5, 0, 0, 0, 100, 0, 0, 0]);
    expect([...writeCommand(text('/a'), 1, 9)]).toEqual([
      0x20, 0, 2, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 9, 0, 0, 0, 47, 97,
    ]);
    expect([...writeDataCommand(3, Uint8Array.from([7, 8]))]).toEqual([0x22, 1, 0, 0, 3, 0, 0, 0, 2, 0, 0, 0, 7, 8]);
    expect([...pathCommand(COMMAND.delete, text('/a'))]).toEqual([0x30, 0, 2, 0, 47, 97]);
    expect([...makeDirectoryCommand(text('/a'))]).toEqual([0x40, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 47, 97]);
    expect([...moveCommand(text('/a'), text('/b'))]).toEqual([0x60, 0, 2, 0, 2, 0, 47, 97, 0, 47, 98]);
  });
});

// ============================================================================
// Against the mock watch
// ============================================================================

describe('file service on the mock watch', () => {
  it('writes into folders that do not exist yet and replaces a longer file', async () => {
    const watch = mockWatch();
    const service = await FileService.open(watch);
    await service.write('/apps/timer/app.ntapp', new Uint8Array(300).fill(9));
    expect(watch.fileService.folders.has('/apps/timer')).toBe(true);
    await service.write('/apps/timer/app.ntapp', Uint8Array.from([1, 2]));
    expect(await service.read('/apps/timer/app.ntapp')).toEqual(Uint8Array.from([1, 2]));
  });

  it('writes and reads an empty file', async () => {
    const service = await FileService.open(mockWatch());
    const progress: number[] = [];
    await service.write('/empty', new Uint8Array(0));
    expect(await service.read('/empty', (step) => progress.push(step.total))).toEqual(new Uint8Array(0));
    expect(progress).toEqual([0]);
  });

  it('lists folders first, then files by name, with their sizes', async () => {
    const service = await FileService.open(mockWatch());
    await service.write('/apps/a.txt', Uint8Array.from([1, 2, 3]));
    expect(await service.list('/apps')).toEqual([
      { name: 'clock', path: '/apps/clock', size: 0, directory: true },
      { name: 'a.txt', path: '/apps/a.txt', size: 3, directory: false },
    ]);
    await expect(service.list('/nope')).rejects.toThrow('/nope does not exist');
    await expect(service.list('/settings/theme.json')).rejects.toThrow('could not do that');
  });

  it('makes folders, moves files and deletes a folder with everything in it', async () => {
    const watch = mockWatch();
    const service = await FileService.open(watch);
    await service.mkdir('/resources/fonts');
    await service.mkdir('/resources/fonts');
    expect(watch.fileService.folders.has('/resources/fonts')).toBe(true);
    await expect(service.mkdir('/settings/theme.json')).rejects.toThrow('could not do that');

    await service.move('/settings/theme.json', '/settings/old.json');
    expect(watch.fileService.files.has('/settings/old.json')).toBe(true);
    await expect(service.move('/nope', '/b')).rejects.toThrow('/nope does not exist');

    await service.remove('/apps/clock');
    expect(watch.fileService.files.has('/apps/clock/state')).toBe(false);
    expect(watch.fileService.folders.has('/apps/clock')).toBe(false);
    await expect(service.remove('/nope')).rejects.toThrow('does not exist');
  });

  it('measures free space and leaves no probe file behind', async () => {
    const watch = mockWatch();
    const service = await FileService.open(watch);
    const before = (await service.storage()).free;
    await service.write('/resources/blob', new Uint8Array(1000));
    expect((await service.storage()).free).toBe(before - 1000);
    expect(watch.fileService.files.has('/.free')).toBe(false);
  });

  it('runs commands one at a time when they are started together', async () => {
    const service = await FileService.open(mockWatch());
    const data = Uint8Array.from({ length: 500 }, (_value, index) => index & 0xff);
    const [, listing, contents] = await Promise.all([
      service.write('/resources/one', data),
      service.list('/resources'),
      service.read('/resources/one'),
    ]);
    expect(listing.map((file) => file.name)).toEqual(['one']);
    expect(contents).toEqual(data);
  });

  it('keeps going after a command fails', async () => {
    const service = await FileService.open(mockWatch());
    await expect(service.read('/nope')).rejects.toThrow('does not exist');
    await expect(service.read('/nope')).rejects.toBeInstanceOf(FileMissingError);
    await expect(service.read('/nope')).rejects.toBeInstanceOf(FileServiceError);
    expect(await service.read('/apps/clock/state')).toEqual(Uint8Array.from([1, 2, 3]));
  });

  it('stops listening when closed', async () => {
    const watch = new ScriptedWatch(() => []);
    const service = await FileService.open(watch);
    expect(watch.subscribed).toBe(true);
    service.close();
    service.close();
    expect(watch.subscribed).toBe(false);
  });
});

// ============================================================================
// Against a watch that misbehaves
// ============================================================================

describe('file service with a misbehaving watch', () => {
  it('sends pieces no longer than the watch allows or one write holds', async () => {
    const watch = new ScriptedWatch(() => {
      const reply = answer(COMMAND.writePacing, STATUS.ok);
      new DataView(reply.buffer).setUint32(16, 30, true);
      return [reply];
    });
    const service = await FileService.open(watch);
    await service.write('/a', new Uint8Array(70));
    const pieces = watch.written.slice(1).map((command) => u32At(command, 8));
    expect(pieces).toEqual([30, 30, 10]);
    expect(Math.max(...watch.written.map((command) => command.length))).toBeLessThanOrEqual(LONGEST_WRITE);
  });

  it('stops a write when the watch has no room', async () => {
    const service = await FileService.open(new ScriptedWatch(() => [answer(COMMAND.writePacing, STATUS.ok)]));
    await expect(service.write('/a', new Uint8Array(5))).rejects.toThrow('no room left');
  });

  it('refuses a path too long for one write', async () => {
    const watch = new ScriptedWatch(() => []);
    const service = await FileService.open(watch);
    await expect(service.write(`/${'a'.repeat(LONGEST_WRITE)}`, new Uint8Array(1))).rejects.toThrow('too long');
    expect(watch.written).toHaveLength(0);
  });

  it('turns each failing status into its own message', async () => {
    const statuses = [STATUS.protocolError, STATUS.error, 0x7f];
    const service = await FileService.open(
      new ScriptedWatch(() => [answer(COMMAND.deleteStatus, statuses.shift() ?? STATUS.ok, 2)]),
    );
    await expect(service.remove('/a')).rejects.toThrow('did not understand');
    await expect(service.remove('/a')).rejects.toThrow('could not do that');
    await expect(service.remove('/a')).rejects.toThrow('could not do that');
  });

  it('refuses an answer to a different command', async () => {
    const service = await FileService.open(new ScriptedWatch(() => [answer(COMMAND.moveStatus, STATUS.ok, 2)]));
    await expect(service.remove('/a')).rejects.toThrow('answered 97');
  });

  it('refuses read pieces out of place or a read that stalls', async () => {
    const piece = (offset: number, total: number, length: number) => {
      const reply = answer(COMMAND.readData, STATUS.ok, 16 + length);
      const fields = new DataView(reply.buffer);
      fields.setUint32(4, offset, true);
      fields.setUint32(8, total, true);
      fields.setUint32(12, length, true);
      return reply;
    };
    const misplaced = await FileService.open(new ScriptedWatch(() => [piece(4, 10, 2)]));
    await expect(misplaced.read('/a')).rejects.toThrow('out of place');
    const stalled = await FileService.open(new ScriptedWatch(() => [piece(0, 10, 0)]));
    await expect(stalled.read('/a')).rejects.toThrow('stopped sending');
  });
});
