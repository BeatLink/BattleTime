// The packages characteristic: installs, lists, hides and removes apps, and installs the shared library.
// A file is sent first through the file transfer service; these commands then check it and act on it.
// The watch side is services/packages in the NeoTime tree, described in docs/companion-protocol.md.

import { NotifyQueue, type DfuLink } from '@/dfu/link';

import { readImageHeader, type ImageHeader } from './app-image';
import { NEOTIME_PACKAGES, NEOTIME_SERVICE } from './neotime';

// ============================================================================
// Types
// ============================================================================

export type WatchApp = { id: string; name: string; kind: 'app' | 'face'; enabled: boolean; bytes: number };

export type Progress = { file: string; sent: number; total: number };

// What installing needs from the file transfer service.
export type FileWriter = {
  write(path: string, data: Uint8Array, onProgress?: (progress: Progress) => void): Promise<void>;
  mkdir?(path: string): Promise<void>;
};

export class PackagesError extends Error {}

type Reply = { c: string; ok?: number; err?: string; a?: [string, string, string, number, number][] };

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
    const reply = await this.command({ c: 'apps' });
    return (reply.a ?? []).map(([id, name, kind, enabled, bytes]) => ({
      id,
      name,
      kind: kind === 'face' ? 'face' : 'app',
      enabled: Boolean(enabled),
      bytes,
    }));
  }

  async install(id: string): Promise<void> {
    appImagePath(id);
    await this.command({ c: 'install', id });
  }

  async remove(id: string): Promise<void> {
    appImagePath(id);
    await this.command({ c: 'remove', id });
  }

  async enable(id: string, on: boolean): Promise<void> {
    appImagePath(id);
    await this.command({ c: 'enable', id, on });
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
        throw new PackagesError(reply.err);
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
