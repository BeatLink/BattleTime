// Reads, lists and deletes files on the watch's flash through the package manager.
// Writing a file reuses sendFile in transfer.ts.

import { base64ToBytes } from '@/ble/encoding';

import {
  checksum,
  encodeLsDir,
  encodeMem,
  encodeRmFile,
  encodeSend,
  type PackageReply,
} from './packages';
import { TransferError, type PackageChannel, type Progress } from './transfer';

export type WatchFile = { name: string; path: string; size: number; directory: boolean };

export type WatchMemory = { free: number; alloc: number };

function fail(reply: PackageReply, what: string): never {
  throw new TransferError(reply.err ?? `The watch could not ${what}.`);
}

export function joinPath(dir: string, name: string): string {
  return dir ? `${dir}/${name}` : name;
}

export function parentPath(path: string): string {
  const slash = path.lastIndexOf('/');
  return slash < 0 ? '' : path.slice(0, slash);
}

export async function listDirectory(channel: PackageChannel, dir: string): Promise<WatchFile[]> {
  await channel.send(encodeLsDir(dir));
  const reply = await channel.next();
  if (reply.ok !== true || !Array.isArray(reply.entries)) {
    fail(reply, `list ${dir || 'the flash'}`);
  }
  return reply.entries
    .map(([name, size, directory]) => ({ name, path: joinPath(dir, name), size, directory }))
    .sort((a, b) => Number(b.directory) - Number(a.directory) || a.name.localeCompare(b.name));
}

// Copy one file off the watch, checking its length and checksum at the end.
export async function readFile(
  channel: PackageChannel,
  path: string,
  onProgress?: (progress: Progress) => void,
): Promise<Uint8Array> {
  await channel.send(encodeSend(path));
  const start = await channel.next();
  if (start.ok !== true || typeof start.tx !== 'number') {
    fail(start, `read ${path}`);
  }
  const size = start.tx;
  const data = new Uint8Array(size);
  let got = 0;

  for (;;) {
    const reply = await channel.next();
    if (reply.ok === false) {
      fail(reply, `read ${path}`);
    }
    if (typeof reply.d === 'string' && typeof reply.off === 'number') {
      const chunk = base64ToBytes(reply.d);
      if (reply.off !== got || got + chunk.length > size) {
        throw new TransferError(`${path}: a chunk arrived out of place.`);
      }
      data.set(chunk, got);
      got += chunk.length;
      onProgress?.({ file: path, sent: got, total: size });
      continue;
    }
    if (reply.ok === true && typeof reply.got === 'number') {
      if (reply.got !== size || got !== size) {
        throw new TransferError(`${path}: received ${got} of ${size} bytes.`);
      }
      if (reply.sum !== checksum(data)) {
        throw new TransferError(`${path}: checksum mismatch.`);
      }
      return data;
    }
    throw new TransferError(`Unexpected reply while reading ${path}.`);
  }
}

export async function deleteFile(channel: PackageChannel, path: string): Promise<void> {
  await channel.send(encodeRmFile(path));
  const reply = await channel.next();
  if (reply.ok !== true || reply.deleted !== path) {
    fail(reply, `delete ${path}`);
  }
}

export async function readMemory(channel: PackageChannel): Promise<WatchMemory> {
  await channel.send(encodeMem());
  const reply = await channel.next();
  if (reply.ok !== true || typeof reply.free !== 'number') {
    fail(reply, 'read its memory');
  }
  return { free: reply.free, alloc: reply.alloc ?? 0 };
}
