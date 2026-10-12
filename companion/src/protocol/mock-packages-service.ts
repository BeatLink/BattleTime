// A stand-in for the watch's packages characteristic, over the mock watch's files.
// It answers as services/packages in the NeoTime tree does.

import { readImageHeader } from './app-image';
import { LIBRARY_UPLOAD_PATH } from './packages';

const ID = /^[A-Za-z0-9_-]{1,32}$/;

export class MockPackagesService {
  // The library last copied into internal flash, for tests to check.
  library: Uint8Array | null = null;

  constructor(
    // The mock watch's files, shared with its file transfer service.
    private readonly files: Map<string, Uint8Array>,
    private readonly notify: (value: Uint8Array) => void,
  ) {}

  // The commands this service answers.
  static handles(command: string): boolean {
    return ['apps', 'install', 'remove', 'enable', 'install_library'].includes(command);
  }

  handle(message: { c: string; id?: string; on?: boolean }) {
    const { c, id } = message;
    if (c === 'apps') {
      this.reply({ c, a: this.list() });
      return;
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
        this.files.delete(`/apps/${id}/disabled`);
        return this.reply({ c, ok: 1 });
      }
      case 'remove':
        for (const path of [...this.files.keys()]) {
          if (path.startsWith(`/apps/${id}/`)) this.files.delete(path);
        }
        return this.reply({ c, ok: 1 });
      case 'enable':
        if (message.on === false) this.files.set(`/apps/${id}/disabled`, new Uint8Array(0));
        else this.files.delete(`/apps/${id}/disabled`);
        return this.reply({ c, ok: 1 });
    }
  }

  private kindOf(image: Uint8Array): string | null {
    try {
      return readImageHeader(image).kind;
    } catch {
      return null;
    }
  }

  private list(): [string, string, string, number, number][] {
    const apps: [string, string, string, number, number][] = [];
    for (const [path, image] of this.files) {
      const match = /^\/apps\/([^/]+)\/app\.ntapp$/.exec(path);
      if (!match) continue;
      try {
        const header = readImageHeader(image);
        if (header.kind === 'library' || header.id !== match[1]) continue;
        const enabled = this.files.has(`/apps/${match[1]}/disabled`) ? 0 : 1;
        apps.push([header.id, header.name, header.kind, enabled, image.length]);
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
