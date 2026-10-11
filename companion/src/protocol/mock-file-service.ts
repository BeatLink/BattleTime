// A stand-in for NeoTime's file service, so files, backups and app installs work with no watch.

import { concat } from '@/dfu/bytes';

// The folders NeoTime makes when it first prepares its file system.
const FOLDERS = ['/apps', '/settings', '/resources'];

const TOTAL_BYTES = 3 * 1024 * 1024;

// The watch reports how far a write has got every this many bytes.
const PACE_BYTES = 4096;

type Upload = { path: string; size: number; received: Uint8Array; reported: number };

export class MockFileService {
  readonly files = new Map<string, Uint8Array>();
  readonly folders = new Set(FOLDERS);
  readonly installed = new Set<string>();
  private pending = '';
  private upload: Upload | null = null;

  constructor(
    // Replies for the control characteristic and chunks for the data one.
    private readonly notifyControl: (value: Uint8Array) => void,
    private readonly notifyData: (value: Uint8Array) => void,
  ) {
    this.files.set('/settings/theme.json', new TextEncoder().encode('{"accent":"#00a0ff"}'));
    this.folders.add('/apps/clock');
    this.files.set('/apps/clock/app.ntapp', new Uint8Array(1200));
    this.files.set('/apps/clock/state', new Uint8Array([1, 2, 3]));
    this.installed.add('clock');
  }

  control(bytes: Uint8Array) {
    this.pending += new TextDecoder().decode(bytes);
    let newline = this.pending.indexOf('\n');
    while (newline >= 0) {
      const line = this.pending.slice(0, newline);
      this.pending = this.pending.slice(newline + 1);
      this.handle(JSON.parse(line));
      newline = this.pending.indexOf('\n');
    }
  }

  data(bytes: Uint8Array) {
    const upload = this.upload;
    if (!upload) {
      return;
    }
    upload.received = concat(upload.received, bytes);
    const length = upload.received.length;
    if (Math.floor(length / PACE_BYTES) > Math.floor(upload.reported / PACE_BYTES)) {
      upload.reported = length;
      this.reply({ c: 'put', at: length });
    }
    if (length >= upload.size) {
      this.files.set(upload.path, upload.received);
      this.folders.add(parent(upload.path));
      this.upload = null;
      this.reply({ c: 'put', done: 1 });
    }
  }

  private reply(message: object) {
    const bytes = new TextEncoder().encode(`${JSON.stringify(message)}\n`);
    // Split long replies as a small MTU would, to exercise the phone's line buffer.
    for (let offset = 0; offset < bytes.length; offset += 20) {
      this.notifyControl(bytes.subarray(offset, offset + 20));
    }
  }

  private handle(message: { c: string; p?: string; n?: number; id?: string }) {
    const { c, p = '' } = message;
    switch (c) {
      case 'ls': {
        if (p !== '/' && !this.folders.has(p)) {
          this.reply({ c, err: `${p} does not exist` });
          return;
        }
        const entries: [string, number, number][] = [];
        for (const folder of this.folders) {
          if (parent(folder) === p) entries.push([name(folder), 0, 1]);
        }
        for (const [path, contents] of this.files) {
          if (parent(path) === p) entries.push([name(path), contents.length, 0]);
        }
        this.reply({ c, e: entries });
        return;
      }
      case 'get': {
        const contents = this.files.get(p);
        if (!contents) {
          this.reply({ c, err: `${p} does not exist` });
          return;
        }
        this.reply({ c, n: contents.length });
        for (let offset = 0; offset < contents.length; offset += 20) {
          this.notifyData(contents.subarray(offset, offset + 20));
        }
        return;
      }
      case 'put':
        this.upload = { path: p, size: message.n ?? 0, received: new Uint8Array(0), reported: 0 };
        this.reply({ c, ok: 1 });
        if (!message.n) {
          this.data(new Uint8Array(0));
        }
        return;
      case 'rm':
        if (!this.files.delete(p) && !this.folders.delete(p)) {
          this.reply({ c, err: `${p} does not exist` });
          return;
        }
        this.reply({ c, ok: 1 });
        return;
      case 'df': {
        const used = [...this.files.values()].reduce((sum, contents) => sum + contents.length, 0);
        this.reply({ c, free: TOTAL_BYTES - used, total: TOTAL_BYTES });
        return;
      }
      case 'install':
        if (!this.files.has(`/apps/${message.id}/app.ntapp`)) {
          this.reply({ c, err: `${message.id} has no image` });
          return;
        }
        this.installed.add(message.id ?? '');
        this.reply({ c, ok: 1 });
        return;
      case 'remove':
        for (const path of [...this.files.keys()]) {
          if (path.startsWith(`/apps/${message.id}/`)) this.files.delete(path);
        }
        this.folders.delete(`/apps/${message.id}`);
        this.installed.delete(message.id ?? '');
        this.reply({ c, ok: 1 });
        return;
      default:
        this.reply({ c, err: `unknown command ${c}` });
    }
  }
}

function parent(path: string): string {
  const slash = path.lastIndexOf('/');
  return slash <= 0 ? '/' : path.slice(0, slash);
}

function name(path: string): string {
  return path.slice(path.lastIndexOf('/') + 1);
}
