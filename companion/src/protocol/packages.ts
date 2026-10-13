// The packages characteristic: installs, lists, hides and removes apps, and installs the shared library.
// A file is sent first through the file transfer service; these commands then check it and act on it.
// The watch side is services/packages in the NeoTime tree, described in docs/companion-protocol.md.

import { NotifyQueue, type DfuLink } from '@/dfu/link';

import { readImageHeader, type ImageHeader } from './app-image';
import { NEOTIME_PACKAGES, NEOTIME_SERVICE } from './neotime';

// ============================================================================
// Types
// ============================================================================

// `need` is the heap the app needs while open, in bytes, or null from firmware that does not say.
export type WatchApp = { id: string; name: string; kind: 'app' | 'face'; enabled: boolean; bytes: number; need: number | null };

// The installed apps, with the most heap an app may need when the watch gives it.
export type WatchApps = { apps: WatchApp[]; room: number | null };

// What the watch said about the heap after installing an app or turning it on or off.
export type MemoryAnswer = { need: number | null; room: number | null };

export type Progress = { file: string; sent: number; total: number };

// What installing needs from the file transfer service.
export type FileWriter = {
  write(path: string, data: Uint8Array, onProgress?: (progress: Progress) => void): Promise<void>;
  mkdir?(path: string): Promise<void>;
};

// Why the watch refused, as docs/companion-protocol.md numbers it.
export const REFUSALS: Record<number, string> = {
  1: 'it needs more memory than an app can have',
  2: 'the launcher would run out of memory',
  3: 'the launcher stays on',
  4: 'the last watch face stays on',
  5: 'it is open on the watch',
};

export class PackagesError extends Error {
  constructor(
    message: string,
    // The refusal's number, the heap needed and the most an app may have, when the watch refused a change.
    readonly why: number | null = null,
    readonly need: number | null = null,
    readonly room: number | null = null,
  ) {
    super(need !== null && room !== null ? `${message} (needs ${memoryText(need)} of ${memoryText(room)})` : message);
  }
}

type Reply = {
  c: string;
  ok?: number;
  err?: string;
  why?: number;
  need?: number;
  room?: number;
  a?: [string, string, string, number, number, number?][];
};

// Bytes as kilobytes with one decimal, as the apps screen and the command line show memory.
export function memoryText(bytes: number): string {
  return `${(bytes / 1024).toFixed(1)} kB`;
}

// Where a new shared library goes before install_library.
export const LIBRARY_UPLOAD_PATH = '/resources/shared.ntlib';

// The commands this client sends, so it can ignore lines meant for anyone else on the characteristic.
const COMMANDS = new Set(['apps', 'install', 'remove', 'enable', 'install_library']);

// How long a command may take; copying the library into internal flash is the slowest.
const REPLY_TIMEOUT_MS = 20000;

// One write's payload at the smallest MTU.
const WRITE_BYTES = 20;

export function appImagePath(id: string): string {
  if (!/^[A-Za-z0-9_-]{1,32}$/.test(id)) {
    throw new PackagesError(`${id}: an app id is 1 to 32 letters, digits, "-" and "_".`);
  }
  return `/apps/${id}/app.ntapp`;
}

// ============================================================================
// The client
// ============================================================================

export class PackagesClient {
  private readonly replies = new NotifyQueue();
  private pending = '';
  private unsubscribe: (() => void) | null = null;

  private constructor(private readonly link: DfuLink) {}

  static async open(link: DfuLink): Promise<PackagesClient> {
    const client = new PackagesClient(link);
    client.unsubscribe = await link.subscribe(NEOTIME_SERVICE, NEOTIME_PACKAGES, (value) => client.onValue(value));
    return client;
  }

  close() {
    this.unsubscribe?.();
    this.unsubscribe = null;
  }

  // Every installed app and face, in the order the launcher shows them.
  async apps(): Promise<WatchApp[]> {
    return (await this.listing()).apps;
  }

  // The installed apps with each one's heap need, and the most an app may have.
  async listing(): Promise<WatchApps> {
    const reply = await this.command({ c: 'apps' });
    const apps = (reply.a ?? []).map(([id, name, kind, enabled, bytes, need]): WatchApp => ({
      id,
      name,
      kind: kind === 'face' ? 'face' : 'app',
      enabled: Boolean(enabled),
      bytes,
      need: need ?? null,
    }));
    return { apps, room: reply.room ?? null };
  }

  async install(id: string): Promise<MemoryAnswer> {
    appImagePath(id);
    return memoryOf(await this.command({ c: 'install', id }));
  }

  async remove(id: string): Promise<void> {
    appImagePath(id);
    await this.command({ c: 'remove', id });
  }

  // Turns an app on or off; the watch first checks it fits, and refuses with a PackagesError that says why.
  async enable(id: string, on: boolean): Promise<MemoryAnswer> {
    appImagePath(id);
    return memoryOf(await this.command({ c: 'enable', id, on }));
  }

  async installLibrary(): Promise<void> {
    await this.command({ c: 'install_library' });
  }

  private async command(message: { c: string } & Record<string, unknown>): Promise<Reply> {
    const bytes = new TextEncoder().encode(`${JSON.stringify(message)}\n`);
    for (let offset = 0; offset < bytes.length; offset += WRITE_BYTES) {
      await this.link.write(NEOTIME_SERVICE, NEOTIME_PACKAGES, bytes.subarray(offset, offset + WRITE_BYTES), 'request');
    }
    for (;;) {
      const reply = JSON.parse(new TextDecoder().decode(await this.replies.next(REPLY_TIMEOUT_MS))) as Reply;
      if (reply.c !== message.c) {
        continue;
      }
      if (reply.err) {
        throw new PackagesError(reply.err, reply.why ?? null, reply.need ?? null, reply.room ?? null);
      }
      return reply;
    }
  }

  // Replies span notifications, so collect bytes until each newline, keeping only this client's lines.
  private onValue(value: Uint8Array) {
    this.pending += new TextDecoder().decode(value);
    let newline = this.pending.indexOf('\n');
    while (newline >= 0) {
      const line = this.pending.slice(0, newline).trim();
      this.pending = this.pending.slice(newline + 1);
      newline = this.pending.indexOf('\n');
      try {
        if (line && COMMANDS.has((JSON.parse(line) as Reply).c)) {
          this.replies.push(new TextEncoder().encode(line));
        }
      } catch {
        // Not a line for this client.
      }
    }
  }
}

// What the apps screen says about an app: its kind, whether it is hidden, its size and the memory it needs.
export function appDetail(app: WatchApp): string {
  const need = app.need === null ? '' : ` · needs ${memoryText(app.need)}`;
  return `${app.kind === 'face' ? 'Watch face' : 'App'}${app.enabled ? '' : ', hidden'} · ${memoryText(app.bytes)}${need}`;
}

function memoryOf(reply: Reply): MemoryAnswer {
  return { need: reply.need ?? null, room: reply.room ?? null };
}

// ============================================================================
// Installing a picked or bundled image
// ============================================================================

// Sends an app, face or shared library image and installs it, returning what it was.
export async function installImage(
  files: FileWriter,
  packages: PackagesClient,
  image: Uint8Array,
  onProgress?: (progress: Progress) => void,
): Promise<ImageHeader> {
  const header = readImageHeader(image);
  if (header.kind === 'library') {
    await files.write(LIBRARY_UPLOAD_PATH, image, onProgress);
    await packages.installLibrary();
    return header;
  }
  const path = appImagePath(header.id);
  await files.mkdir?.(`/apps/${header.id}`).catch(() => {});
  await files.write(path, image, onProgress);
  await packages.install(header.id);
  return header;
}
