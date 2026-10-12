// NeoTime's file service: JSON commands on a control characteristic and raw bytes on a data characteristic.
// The watch side and the full description live in docs/companion-protocol.md in the NeoTime tree; keep them in step.

import type { DfuLink } from '@/dfu/link';
import { NotifyQueue } from '@/dfu/link';
import { DFU_PAYLOAD_SIZE } from '@/dfu/uuids';

import { NEOTIME_FILE_CONTROL, NEOTIME_FILE_DATA, NEOTIME_SERVICE } from './neotime';

// ============================================================================
// Types
// ============================================================================

export type WatchFile = { name: string; path: string; size: number; directory: boolean };

export type WatchStorage = { free: number; total: number };

export type Progress = { file: string; sent: number; total: number };

export class FileServiceError extends Error {}

// A reply on the control characteristic: the command it answers plus whatever that command reports.
type Reply = {
  c: string;
  err?: string;
  e?: [string, number, boolean | number][];
  n?: number;
  ok?: number | boolean;
  at?: number;
  done?: number | boolean;
  free?: number;
  total?: number;
};

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

// Where an app's compiled image goes, which `install` then checks and adds.
export function appImagePath(id: string): string {
  if (!/^[a-z0-9_-]+$/i.test(id)) {
    throw new FileServiceError(`${id}: an app id is letters, digits, "-" and "_".`);
  }
  return `/apps/${id}/app.ntapp`;
}

// ============================================================================
// The client
// ============================================================================

// Commands the packages client sends on the same characteristic.
const PACKAGES_COMMANDS = new Set(['apps', 'install', 'remove', 'enable', 'install_library']);

// How long one command may take, which covers a sector erase before a write.
const REPLY_TIMEOUT_MS = 15000;

// The watch reports how far a write has got every this many bytes, and the phone waits for it.
const PACE_BYTES = 4096;

export class FileService {
  private readonly replies = new NotifyQueue();
  private readonly data = new NotifyQueue();
  private pending = '';
  private unsubscribers: (() => void)[] = [];

  private constructor(private readonly link: DfuLink) {}

  static async open(link: DfuLink): Promise<FileService> {
    const service = new FileService(link);
    service.unsubscribers = await Promise.all([
      link.subscribe(NEOTIME_SERVICE, NEOTIME_FILE_CONTROL, (value) => service.onControl(value)),
      link.subscribe(NEOTIME_SERVICE, NEOTIME_FILE_DATA, (value) => service.data.push(value)),
    ]);
    return service;
  }

  close() {
    for (const unsubscribe of this.unsubscribers) {
      unsubscribe();
    }
    this.unsubscribers = [];
  }

  async list(dir: string): Promise<WatchFile[]> {
    const reply = await this.command({ c: 'ls', p: checkPath(dir) });
    if (!Array.isArray(reply.e)) {
      throw new FileServiceError(`The watch did not list ${dir}.`);
    }
    return reply.e
      .map(([name, size, directory]) => ({ name, path: joinPath(dir, name), size, directory: Boolean(directory) }))
      .sort((a, b) => Number(b.directory) - Number(a.directory) || a.name.localeCompare(b.name));
  }

  async read(path: string, onProgress?: (progress: Progress) => void): Promise<Uint8Array> {
    const reply = await this.command({ c: 'get', p: checkPath(path) });
    if (typeof reply.n !== 'number') {
      throw new FileServiceError(`The watch did not say how long ${path} is.`);
    }
    const out = new Uint8Array(reply.n);
    let got = 0;
    while (got < out.length) {
      const chunk = await this.data.next(REPLY_TIMEOUT_MS);
      if (got + chunk.length > out.length) {
        throw new FileServiceError(`${path}: the watch sent more than it said it would.`);
      }
      out.set(chunk, got);
      got += chunk.length;
      onProgress?.({ file: path, sent: got, total: out.length });
    }
    return out;
  }

  async write(path: string, contents: Uint8Array, onProgress?: (progress: Progress) => void): Promise<void> {
    await this.command({ c: 'put', p: checkPath(path), n: contents.length });
    let acknowledged = 0;
    for (let offset = 0; offset < contents.length; offset += DFU_PAYLOAD_SIZE) {
      const chunk = contents.subarray(offset, offset + DFU_PAYLOAD_SIZE);
      await this.link.write(NEOTIME_SERVICE, NEOTIME_FILE_DATA, chunk, 'command');
      const sent = offset + chunk.length;
      // Wait at each 4 KB mark for the watch to say it has that much, so the phone never runs ahead.
      if (sent < contents.length && Math.floor(sent / PACE_BYTES) > Math.floor(acknowledged / PACE_BYTES)) {
        const reply = await this.nextReply('put');
        if (typeof reply.at !== 'number') {
          throw new FileServiceError(`${path}: the watch answered out of turn.`);
        }
        acknowledged = reply.at;
      }
      onProgress?.({ file: path, sent, total: contents.length });
    }
    // The last progress reports and then the finish arrive here, in that order.
    for (;;) {
      const reply = await this.nextReply('put');
      if (reply.done) {
        return;
      }
    }
  }

  async remove(path: string): Promise<void> {
    await this.command({ c: 'rm', p: checkPath(path) });
  }

  async storage(): Promise<WatchStorage> {
    const reply = await this.command({ c: 'df' });
    return { free: reply.free ?? 0, total: reply.total ?? 0 };
  }

  // Send an app's compiled image and ask the watch to check it and add it to the installed apps.
  async installApp(id: string, image: Uint8Array, onProgress?: (progress: Progress) => void): Promise<void> {
    await this.write(appImagePath(id), image, onProgress);
    await this.command({ c: 'install', id });
  }

  async removeApp(id: string): Promise<void> {
    appImagePath(id);
    await this.command({ c: 'remove', id });
  }

  // ==========================================================================
  // Commands and replies
  // ==========================================================================

  private async command(message: { c: string } & Record<string, unknown>): Promise<Reply> {
    const bytes = new TextEncoder().encode(`${JSON.stringify(message)}\n`);
    for (let offset = 0; offset < bytes.length; offset += DFU_PAYLOAD_SIZE) {
      await this.link.write(NEOTIME_SERVICE, NEOTIME_FILE_CONTROL, bytes.subarray(offset, offset + DFU_PAYLOAD_SIZE), 'request');
    }
    return this.nextReply(message.c);
  }

  private async nextReply(command: string): Promise<Reply> {
    const raw = await this.replies.next(REPLY_TIMEOUT_MS);
    const reply = JSON.parse(new TextDecoder().decode(raw)) as Reply;
    // The packages client shares this characteristic, so its replies pass by here too.
    if (reply.c !== command && PACKAGES_COMMANDS.has(reply.c)) {
      return this.nextReply(command);
    }
    if (reply.err) {
      throw new FileServiceError(reply.err);
    }
    if (reply.c !== command) {
      throw new FileServiceError(`The watch answered ${reply.c} to ${command}.`);
    }
    return reply;
  }

  // Replies can span notifications, so collect bytes until each newline.
  private onControl(value: Uint8Array) {
    this.pending += new TextDecoder().decode(value);
    let newline = this.pending.indexOf('\n');
    while (newline >= 0) {
      const line = this.pending.slice(0, newline).trim();
      this.pending = this.pending.slice(newline + 1);
      if (line) {
        this.replies.push(new TextEncoder().encode(line));
      }
      newline = this.pending.indexOf('\n');
    }
  }
}
