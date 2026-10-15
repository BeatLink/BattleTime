// The packages characteristic: installs, lists, hides and removes apps, and runs temporary apps.
// A file is sent first through the file transfer service; these commands then check it and act on it.
// The watch side is services/packages in the BattleTime tree, described in docs/companion-protocol.md.

import { NotifyQueue, type DfuLink } from '@/dfu/link';

import { readImageHeader, type ImageHeader } from './app-image';
import { FileMissingError } from './file-service';
import { placeNewApps } from './launcher';
import { BATTLETIME_PACKAGES, BATTLETIME_SERVICE } from './battletime';

// ============================================================================
// Types
// ============================================================================

// `need` is the heap the app needs while open, in bytes, or null from firmware that does not say; `group` is the
// launcher group the app declares, empty when it declares none or the firmware does not say.
export type WatchApp = {
  id: string;
  name: string;
  kind: 'app' | 'face';
  enabled: boolean;
  bytes: number;
  need: number | null;
  group: string;
};

// The installed apps, with the most heap an app may need when the watch gives it.
export type WatchApps = { apps: WatchApp[]; room: number | null };

// What the watch said about the heap after installing an app or turning it on or off.
export type MemoryAnswer = { need: number | null; room: number | null };

export type Progress = { file: string; sent: number; total: number };

// What installing needs from the file transfer service; with `read`, a new app also takes its place in the launcher's groups.
export type FileWriter = {
  write(path: string, data: Uint8Array, onProgress?: (progress: Progress) => void): Promise<void>;
  mkdir?(path: string): Promise<void>;
  read?(path: string): Promise<Uint8Array>;
  readStart?(path: string, length: number): Promise<{ bytes: Uint8Array; total: number }>;
  list?(dir: string): Promise<{ name: string; path: string }[]>;
  remove?(path: string): Promise<void>;
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
  a?: [string, string, string, number, number, number?, string?][];
};

// Bytes as kilobytes with one decimal, as the apps screen and the command line show memory.
export function memoryText(bytes: number): string {
  return `${(bytes / 1024).toFixed(1)} kB`;
}

// The commands this client sends, so it can ignore lines meant for anyone else on the characteristic.
const COMMANDS = new Set(['apps', 'install', 'remove', 'enable', 'launcher', 'run', 'stop']);

// How long a command may take; checking a large app's code is the slowest.
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
    client.unsubscribe = await link.subscribe(BATTLETIME_SERVICE, BATTLETIME_PACKAGES, (value) => client.onValue(value));
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
    const apps = (reply.a ?? []).map(([id, name, kind, enabled, bytes, need, group]): WatchApp => ({
      id,
      name,
      kind: kind === 'face' ? 'face' : 'app',
      enabled: Boolean(enabled),
      bytes,
      need: need ?? null,
      group: group ?? '',
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

  // Has the launcher read /settings/launcher again, after it was written.
  async reloadLauncher(): Promise<void> {
    await this.command({ c: 'launcher' });
  }

  // Opens the temporary app in /temp/<id> on top of whatever the watch shows, or reopens it fresh.
  async run(id: string): Promise<MemoryAnswer> {
    appImagePath(id);
    return memoryOf(await this.command({ c: 'run', id }));
  }

  // Closes the temporary app if one is open; its files stay.
  async stop(): Promise<void> {
    await this.command({ c: 'stop' });
  }

  private async command(message: { c: string } & Record<string, unknown>): Promise<Reply> {
    const bytes = new TextEncoder().encode(`${JSON.stringify(message)}\n`);
    for (let offset = 0; offset < bytes.length; offset += WRITE_BYTES) {
      await this.link.write(BATTLETIME_SERVICE, BATTLETIME_PACKAGES, bytes.subarray(offset, offset + WRITE_BYTES), 'request');
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

// Sends an app or face image and installs it, returning what it was. The shared library ships inside the firmware.
export async function installImage(
  files: FileWriter,
  packages: PackagesClient,
  image: Uint8Array,
  onProgress?: (progress: Progress) => void,
): Promise<ImageHeader> {
  const header = readImageHeader(image);
  if (header.kind === 'library') {
    throw new PackagesError('The shared library ships inside the firmware; update the firmware instead.');
  }
  const path = appImagePath(header.id);
  await files.mkdir?.(`/apps/${header.id}`).catch(() => {});
  await files.write(path, image, onProgress);
  await packages.install(header.id);
  if (files.read) {
    await placeNewApps({ read: files.read.bind(files), write: files.write.bind(files) }, packages);
  }
  return header;
}

// ============================================================================
// Temporary apps
// ============================================================================

// Temporary apps live here, one folder each, and never show in the launcher or the apps listing.
export const TEMPORARY_DIRECTORY = '/temp';

// The image's name inside a temporary app's folder; every other file there is the app's data.
export const TEMPORARY_IMAGE_FILE = 'app.ntapp';

// The most one data file may hold; they are rewritten on every run, so they are meant to be small.
export const TEMPORARY_DATA_LIMIT = 4096;

// The image header holds the CRC of the whole image, so its bytes and the length tell two images apart.
const IMAGE_HEADER_BYTES = 20;

// What running a temporary app did: whether the image was sent or already on the watch, and its memory.
export type TemporaryRun = { header: ImageHeader; uploaded: boolean } & MemoryAnswer;

// A file in a temporary app's folder, such as /temp/prompt/text.
export function temporaryAppPath(id: string, file: string): string {
  appImagePath(id);
  if (!/^[A-Za-z0-9._-]{1,32}$/.test(file) || file === '.' || file === '..') {
    throw new PackagesError(`${file}: a temporary app's file name is 1 to 32 letters, digits, "-", "_" and ".".`);
  }
  return `${TEMPORARY_DIRECTORY}/${id}/${file}`;
}

// A data file's path; the name may not be the image's own.
function dataFilePath(id: string, name: string): string {
  if (name === TEMPORARY_IMAGE_FILE) {
    throw new PackagesError(`${name} is the app's image, so it cannot be a data file.`);
  }
  return temporaryAppPath(id, name);
}

// Whether the watch already holds this exact image at the path, read from its header alone.
async function sameImageOnWatch(files: FileWriter, path: string, image: Uint8Array): Promise<boolean> {
  if (!files.readStart) return false;
  try {
    const { bytes, total } = await files.readStart(path, IMAGE_HEADER_BYTES);
    return total === image.length && bytes.length === IMAGE_HEADER_BYTES && bytes.every((byte, at) => byte === image[at]);
  } catch {
    return false;
  }
}

// Sends a temporary app's image unless the watch has it cached, writes its data files and opens it.
export async function runTemporaryApp(
  files: FileWriter,
  packages: PackagesClient,
  image: Uint8Array,
  data: Record<string, string | Uint8Array> = {},
  onProgress?: (progress: Progress) => void,
): Promise<TemporaryRun> {
  const header = readImageHeader(image);
  if (header.kind !== 'app') {
    throw new PackagesError(`Only an app can run as a temporary app, and ${header.id} is a ${header.kind === 'face' ? 'watch face' : 'shared library'}.`);
  }
  const imagePath = temporaryAppPath(header.id, TEMPORARY_IMAGE_FILE);
  const dataFiles = Object.entries(data).map(([name, contents]) => {
    const bytes = typeof contents === 'string' ? new TextEncoder().encode(contents) : contents;
    if (bytes.length > TEMPORARY_DATA_LIMIT) {
      throw new PackagesError(`${name} holds ${bytes.length} bytes; a temporary app's data file holds at most ${TEMPORARY_DATA_LIMIT}.`);
    }
    return { path: dataFilePath(header.id, name), bytes };
  });
  await files.mkdir?.(`${TEMPORARY_DIRECTORY}/${header.id}`).catch(() => {});
  const uploaded = !(await sameImageOnWatch(files, imagePath, image));
  if (uploaded) {
    await files.write(imagePath, image, onProgress);
  }
  for (const file of dataFiles) {
    await files.write(file.path, file.bytes, onProgress);
  }
  await removeStaleFiles(files, header.id, new Set([imagePath, ...dataFiles.map((file) => file.path)]));
  return { header, uploaded, ...(await packages.run(header.id)) };
}

// Deletes what an earlier run left in the app's folder, so the app never reads old data.
async function removeStaleFiles(files: FileWriter, id: string, keep: Set<string>) {
  if (!files.list || !files.remove) return;
  for (const entry of await files.list(`${TEMPORARY_DIRECTORY}/${id}`)) {
    if (!keep.has(entry.path)) await files.remove(entry.path);
  }
}

// Closes the open temporary app, then deletes one temporary app's folder, or all of /temp with no id.
export async function forgetTemporaryApp(
  files: Pick<Required<FileWriter>, 'remove'>,
  packages: PackagesClient,
  id?: string,
): Promise<void> {
  if (id !== undefined) appImagePath(id);
  const folder = id === undefined ? TEMPORARY_DIRECTORY : `${TEMPORARY_DIRECTORY}/${id}`;
  await packages.stop();
  try {
    await files.remove(folder);
  } catch (error) {
    // A folder that is not there is already forgotten.
    if (!(error instanceof FileMissingError)) throw error;
  }
}
