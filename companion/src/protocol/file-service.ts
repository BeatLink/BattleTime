// Adafruit BLE file transfer, the file service InfiniTime calls FSService and NeoTime serves at 0xFEBB.
// The watch side and the full description live in docs/companion-protocol.md in the NeoTime tree; keep them in step.

import type { DfuLink } from '@/dfu/link';
import { NotifyQueue } from '@/dfu/link';

import { FILE_TRANSFER, FILE_TRANSFER_SERVICE } from './neotime';

// ============================================================================
// Types
// ============================================================================

export type WatchFile = { name: string; path: string; size: number; directory: boolean };

export type WatchStorage = { free: number };

export type Progress = { file: string; sent: number; total: number };

export class FileServiceError extends Error {}

// The watch has no file or folder at the path asked for.
export class FileMissingError extends FileServiceError {}

// ============================================================================
// The protocol's numbers
// ============================================================================

export const COMMAND = {
  read: 0x10,
  readData: 0x11,
  readPacing: 0x12,
  write: 0x20,
  writePacing: 0x21,
  writeData: 0x22,
  delete: 0x30,
  deleteStatus: 0x31,
  makeDirectory: 0x40,
  makeDirectoryStatus: 0x41,
  listDirectory: 0x50,
  listDirectoryEntry: 0x51,
  move: 0x60,
  moveStatus: 0x61,
} as const;

export const STATUS = { ok: 0x01, error: 0x02, noFile: 0x03, protocolError: 0x04 } as const;

// The longest write the phone sends: the watch's 124-byte MTU less the 3-byte header. Phones ask for 185
// and BlueZ for 517, so both settle on the watch's 124.
export const LONGEST_WRITE = 121;

const WRITE_DATA_HEADER = 12;
const READ_DATA_HEADER = 16;
const LIST_ENTRY_HEADER = 28;

// The chunk the phone asks for each time; the watch sends what fits in one notification.
const READ_CHUNK = 4096;

// Measuring free space opens this file and deletes it again.
const FREE_SPACE_PROBE = '/.free';

// ============================================================================
// Paths
// ============================================================================

export function joinPath(dir: string, name: string): string {
  return dir === '/' ? `/${name}` : `${dir}/${name}`;
}

export function parentPath(path: string): string {
  const slash = path.lastIndexOf('/');
  return slash <= 0 ? '/' : path.slice(0, slash);
}

// Refuse what the watch would refuse anyway, so the user hears why before anything is sent.
export function checkPath(path: string): string {
  if (!path.startsWith('/') || path.split('/').some((part) => part === '..')) {
    throw new FileServiceError(`${path}: give an absolute path with no "..".`);
  }
  return path;
}

// ============================================================================
// Building commands
// ============================================================================

class Writer {
  private readonly bytes: Uint8Array;
  private readonly view: DataView;
  private at = 0;

  constructor(length: number) {
    this.bytes = new Uint8Array(length);
    this.view = new DataView(this.bytes.buffer);
  }

  u8(value: number) {
    this.view.setUint8(this.at, value);
    this.at += 1;
    return this;
  }

  u16(value: number) {
    this.view.setUint16(this.at, value, true);
    this.at += 2;
    return this;
  }

  u32(value: number) {
    this.view.setUint32(this.at, value, true);
    this.at += 4;
    return this;
  }

  zeros(count: number) {
    this.at += count;
    return this;
  }

  raw(bytes: Uint8Array) {
    this.bytes.set(bytes, this.at);
    this.at += bytes.length;
    return this;
  }

  done(): Uint8Array {
    return this.bytes;
  }
}

const encoder = new TextEncoder();
const decoder = new TextDecoder();

export function readCommand(path: Uint8Array, offset: number, chunkSize: number, pacing = false): Uint8Array {
  if (pacing) {
    return new Writer(12).u8(COMMAND.readPacing).u8(STATUS.ok).zeros(2).u32(offset).u32(chunkSize).done();
  }
  return new Writer(12 + path.length)
    .u8(COMMAND.read)
    .zeros(1)
    .u16(path.length)
    .u32(offset)
    .u32(chunkSize)
    .raw(path)
    .done();
}

export function writeCommand(path: Uint8Array, offset: number, totalSize: number): Uint8Array {
  return new Writer(20 + path.length)
    .u8(COMMAND.write)
    .zeros(1)
    .u16(path.length)
    .u32(offset)
    .zeros(8)
    .u32(totalSize)
    .raw(path)
    .done();
}

export function writeDataCommand(offset: number, data: Uint8Array): Uint8Array {
  return new Writer(WRITE_DATA_HEADER + data.length)
    .u8(COMMAND.writeData)
    .u8(STATUS.ok)
    .zeros(2)
    .u32(offset)
    .u32(data.length)
    .raw(data)
    .done();
}

export function pathCommand(command: number, path: Uint8Array): Uint8Array {
  return new Writer(4 + path.length).u8(command).zeros(1).u16(path.length).raw(path).done();
}

export function makeDirectoryCommand(path: Uint8Array): Uint8Array {
  return new Writer(16 + path.length).u8(COMMAND.makeDirectory).zeros(1).u16(path.length).zeros(12).raw(path).done();
}

export function moveCommand(oldPath: Uint8Array, newPath: Uint8Array): Uint8Array {
  return new Writer(7 + oldPath.length + newPath.length)
    .u8(COMMAND.move)
    .zeros(1)
    .u16(oldPath.length)
    .u16(newPath.length)
    .raw(oldPath)
    .zeros(1)
    .raw(newPath)
    .done();
}

function view(bytes: Uint8Array): DataView {
  return new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
}

// ============================================================================
// The client
// ============================================================================

// How long one command may take, which covers a sector erase before a write.
const REPLY_TIMEOUT_MS = 15000;

export class FileService {
  private readonly answers = new NotifyQueue();
  private unsubscribe: (() => void) | null = null;
  // Commands run one at a time, since each answer is matched to the command before it.
  private queue: Promise<unknown> = Promise.resolve();

  private constructor(private readonly link: DfuLink) {}

  static async open(link: DfuLink): Promise<FileService> {
    const service = new FileService(link);
    service.unsubscribe = await link.subscribe(FILE_TRANSFER_SERVICE, FILE_TRANSFER, (value) =>
      service.answers.push(value),
    );
    return service;
  }

  close() {
    this.unsubscribe?.();
    this.unsubscribe = null;
  }

  list(dir: string): Promise<WatchFile[]> {
    return this.serially(async () => {
      const path = encoder.encode(checkPath(dir));
      const entries: WatchFile[] = [];
      let answer = await this.command(pathCommand(COMMAND.listDirectory, path), COMMAND.listDirectoryEntry, dir);
      for (;;) {
        const fields = view(answer);
        const nameLength = fields.getUint16(2, true);
        const entry = fields.getUint32(4, true);
        const total = fields.getUint32(8, true);
        if (entry >= total) {
          break;
        }
        const name = decoder.decode(answer.subarray(LIST_ENTRY_HEADER, LIST_ENTRY_HEADER + nameLength));
        if (name !== '.' && name !== '..') {
          const directory = (fields.getUint32(12, true) & 1) === 1;
          entries.push({ name, path: joinPath(dir, name), size: fields.getUint32(24, true), directory });
        }
        answer = await this.answer(COMMAND.listDirectoryEntry, dir);
      }
      return entries.sort((a, b) => Number(b.directory) - Number(a.directory) || a.name.localeCompare(b.name));
    });
  }

  read(path: string, onProgress?: (progress: Progress) => void): Promise<Uint8Array> {
    return this.serially(async () => {
      const encoded = encoder.encode(checkPath(path));
      let answer = await this.command(readCommand(encoded, 0, READ_CHUNK), COMMAND.readData, path);
      const total = view(answer).getUint32(8, true);
      const out = new Uint8Array(total);
      let got = 0;
      for (;;) {
        const fields = view(answer);
        const offset = fields.getUint32(4, true);
        const length = fields.getUint32(12, true);
        if (offset !== got || offset + length > total) {
          throw new FileServiceError(`${path}: the watch sent a piece out of place.`);
        }
        out.set(answer.subarray(READ_DATA_HEADER, READ_DATA_HEADER + length), offset);
        got += length;
        onProgress?.({ file: path, sent: got, total });
        if (got >= total) {
          return out;
        }
        if (length === 0) {
          throw new FileServiceError(`${path}: the watch stopped sending.`);
        }
        answer = await this.command(readCommand(encoded, got, READ_CHUNK, true), COMMAND.readData, path);
      }
    });
  }

  // Up to `length` bytes from the start of a file, with the file's whole size, without reading the rest.
  readStart(path: string, length: number): Promise<{ bytes: Uint8Array; total: number }> {
    return this.serially(async () => {
      const encoded = encoder.encode(checkPath(path));
      let answer = await this.command(readCommand(encoded, 0, length), COMMAND.readData, path);
      const total = view(answer).getUint32(8, true);
      const out = new Uint8Array(Math.min(length, total));
      let got = 0;
      for (;;) {
        const fields = view(answer);
        const offset = fields.getUint32(4, true);
        const piece = Math.min(fields.getUint32(12, true), out.length - got);
        if (offset !== got) {
          throw new FileServiceError(`${path}: the watch sent a piece out of place.`);
        }
        out.set(answer.subarray(READ_DATA_HEADER, READ_DATA_HEADER + piece), offset);
        got += piece;
        if (got >= out.length) {
          return { bytes: out, total };
        }
        if (piece === 0) {
          throw new FileServiceError(`${path}: the watch stopped sending.`);
        }
        answer = await this.command(readCommand(encoded, got, out.length - got, true), COMMAND.readData, path);
      }
    });
  }

  // The watch makes any missing folders above the file, and replaces a file that is already there.
  write(path: string, contents: Uint8Array, onProgress?: (progress: Progress) => void): Promise<void> {
    return this.serially(async () => {
      const encoded = encoder.encode(checkPath(path));
      let answer = await this.command(writeCommand(encoded, 0, contents.length), COMMAND.writePacing, path);
      let offset = 0;
      while (offset < contents.length) {
        const allowed = view(answer).getUint32(16, true);
        if (allowed === 0) {
          throw new FileServiceError(`${path}: the watch has no room left.`);
        }
        const piece = contents.subarray(offset, offset + Math.min(allowed, LONGEST_WRITE - WRITE_DATA_HEADER));
        answer = await this.command(writeDataCommand(offset, piece), COMMAND.writePacing, path);
        offset += piece.length;
        onProgress?.({ file: path, sent: offset, total: contents.length });
      }
    });
  }

  // Deleting a folder deletes everything in it.
  remove(path: string): Promise<void> {
    return this.serially(async () => {
      const encoded = encoder.encode(checkPath(path));
      await this.command(pathCommand(COMMAND.delete, encoded), COMMAND.deleteStatus, path);
    });
  }

  // Makes the folder and any missing above it; a folder that is already there is fine.
  mkdir(path: string): Promise<void> {
    return this.serially(async () => {
      const encoded = encoder.encode(checkPath(path));
      await this.command(makeDirectoryCommand(encoded), COMMAND.makeDirectoryStatus, path);
    });
  }

  move(from: string, to: string): Promise<void> {
    return this.serially(async () => {
      const command = moveCommand(encoder.encode(checkPath(from)), encoder.encode(checkPath(to)));
      await this.command(command, COMMAND.moveStatus, from);
    });
  }

  // The protocol has no free-space command, but a write's first answer says how much the watch can take.
  storage(): Promise<WatchStorage> {
    const probe = encoder.encode(FREE_SPACE_PROBE);
    return this.serially(async () => {
      const answer = await this.command(writeCommand(probe, 0, 0xffffffff), COMMAND.writePacing, FREE_SPACE_PROBE);
      await this.command(pathCommand(COMMAND.delete, probe), COMMAND.deleteStatus, FREE_SPACE_PROBE);
      return { free: view(answer).getUint32(16, true) };
    });
  }

  // ==========================================================================
  // Commands and answers
  // ==========================================================================

  private serially<T>(work: () => Promise<T>): Promise<T> {
    const result = this.queue.then(work, work);
    this.queue = result.catch(() => undefined);
    return result;
  }

  private async command(bytes: Uint8Array, expected: number, path: string): Promise<Uint8Array> {
    if (bytes.length > LONGEST_WRITE) {
      throw new FileServiceError(`${path}: the path is too long to send.`);
    }
    await this.link.write(FILE_TRANSFER_SERVICE, FILE_TRANSFER, bytes, 'request');
    return this.answer(expected, path);
  }

  private async answer(expected: number, path: string): Promise<Uint8Array> {
    const answer = await this.answers.next(REPLY_TIMEOUT_MS);
    if (answer[0] !== expected) {
      throw new FileServiceError(`The watch answered ${answer[0]} to a command expecting ${expected}.`);
    }
    switch (answer[1]) {
      case STATUS.ok:
        return answer;
      case STATUS.noFile:
        throw new FileMissingError(`${path} does not exist.`);
      case STATUS.protocolError:
        throw new FileServiceError(`${path}: the watch did not understand the request.`);
      default:
        throw new FileServiceError(`${path}: the watch could not do that.`);
    }
  }
}
