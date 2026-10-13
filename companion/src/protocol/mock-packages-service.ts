// A stand-in for the watch's packages characteristic, over the mock watch's files.
// It answers as services/packages in the NeoTime tree does.

import { readImageHeader } from './app-image';
import { LIBRARY_UPLOAD_PATH, REFUSALS } from './packages';

const ID = /^[A-Za-z0-9_-]{1,32}$/;

// The most heap an app may need, as NeoTime's APP_HEAP_BUDGET_BYTES.
export const MOCK_ROOM = 33792;

// The mock's stand-in for the watch's estimate: the image, plus what the app host always holds.
const MOCK_WORKING_BYTES = 8000;

export class MockPackagesService {
  // The library last copied into internal flash, for tests to check.
  library: Uint8Array | null = null;

  // How many times the launcher was told to read its groups again, for tests to check.
  launcherReloads = 0;

  constructor(
    // The mock watch's files, shared with its file transfer service.
    private readonly files: Map<string, Uint8Array>,
    private readonly notify: (value: Uint8Array) => void,
  ) {}

  // The commands this service answers.
  static handles(command: string): boolean {
    return ['apps', 'install', 'remove', 'enable', 'install_library', 'launcher'].includes(command);
  }

  handle(message: { c: string; id?: string; on?: boolean }) {
    const { c, id } = message;
    if (c === 'apps') {
      this.reply({ c, room: MOCK_ROOM, a: this.list() });
      return;
    }
    if (c === 'launcher') {
      this.launcherReloads += 1;
      return this.reply({ c, ok: 1 });
    }
    if (c === 'install_library') {
      const image = this.files.get(LIBRARY_UPLOAD_PATH);
      if (!image) return this.reply({ c, err: 'no such app' });
      if (this.kindOf(image) !== 'library') return this.reply({ c, err: 'that image is not the shared library' });
      this.library = image;
      return this.reply({ c, ok: 1 });
    }
    if (!id || !ID.test(id)) {
      return this.reply({ c, err: 'an app id is letters, digits, - and _' });
    }
    const image = this.files.get(`/apps/${id}/app.ntapp`);
    if (!image) {
      return this.reply({ c, err: 'no such app' });
    }
    switch (c) {
      case 'install': {
        let header;
        try {
          header = readImageHeader(image);
        } catch {
          return this.reply({ c, err: 'the image fails the app checks' });
        }
        if (header.kind === 'library') return this.reply({ c, err: 'the shared library is installed with install_library' });
        if (header.id !== id) return this.reply({ c, err: "the image's id is not the folder's name" });
        if (this.needOf(image) > MOCK_ROOM) {
          this.files.set(`/apps/${id}/disabled`, new Uint8Array(0));
          return this.refuse(c, 1, this.needOf(image));
        }
        this.files.delete(`/apps/${id}/disabled`);
        return this.reply({ c, ok: 1, need: this.needOf(image), room: MOCK_ROOM });
      }
      case 'remove':
        for (const path of [...this.files.keys()]) {
          if (path.startsWith(`/apps/${id}/`)) this.files.delete(path);
        }
        return this.reply({ c, ok: 1 });
      case 'enable': {
        const turningOff = message.on === false;
        const lastFace = this.list().filter((app) => app[2] === 'face' && app[3] === 1).map((app) => app[0]);
        if (turningOff && id === 'launcher') return this.refuse(c, 3);
        if (turningOff && lastFace.length === 1 && lastFace[0] === id) return this.refuse(c, 4);
        if (!turningOff && this.needOf(image) > MOCK_ROOM) return this.refuse(c, 1, this.needOf(image));
        if (turningOff) this.files.set(`/apps/${id}/disabled`, new Uint8Array(0));
        else this.files.delete(`/apps/${id}/disabled`);
        return this.reply({ c, ok: 1, need: this.needOf(image), room: MOCK_ROOM });
      }
    }
  }

  private needOf(image: Uint8Array): number {
    return image.length + MOCK_WORKING_BYTES;
  }

  // A refusal as the watch words it, with the memory when that is the reason.
  private refuse(c: string, why: number, need?: number) {
    const err = REFUSALS[why];
    this.reply(need === undefined ? { c, err, why } : { c, err, why, need, room: MOCK_ROOM });
  }

  private kindOf(image: Uint8Array): string | null {
    try {
      return readImageHeader(image).kind;
    } catch {
      return null;
    }
  }

  private list(): [string, string, string, number, number, number, string][] {
    const apps: [string, string, string, number, number, number, string][] = [];
    for (const [path, image] of this.files) {
      const match = /^\/apps\/([^/]+)\/app\.ntapp$/.exec(path);
      if (!match) continue;
      try {
        const header = readImageHeader(image);
        if (header.kind === 'library' || header.id !== match[1]) continue;
        const enabled = this.files.has(`/apps/${match[1]}/disabled`) ? 0 : 1;
        apps.push([header.id, header.name, header.kind, enabled, image.length, this.needOf(image), header.group]);
      } catch {
        // Not a valid image, so not listed, as on the watch.
      }
    }
    return apps.sort((a, b) => a[0].localeCompare(b[0]));
  }

  private reply(message: object) {
    const bytes = new TextEncoder().encode(`${JSON.stringify(message)}\n`);
    for (let offset = 0; offset < bytes.length; offset += 20) {
      this.notify(bytes.subarray(offset, offset + 20));
    }
  }
}
