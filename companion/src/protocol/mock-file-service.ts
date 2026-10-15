// A stand-in for NeoTime's BLEFS file service, so files, backups and app installs work with no watch.
// It answers as services/blefs in the NeoTime tree does, one notification per answer.

import { encodeAppState, type StateEntry } from './app-state';
import { COMMAND, STATUS } from './file-service';
import { SLOTS_PER_DAY, STEP_BLOCK_BYTES, startOfDay, stepBlockPath, writeSlotsIntoBlock } from './steps';

// The folders NeoTime makes when it first prepares its file system.
const FOLDERS = ['/apps', '/settings', '/resources'];

const TOTAL_BYTES = 3 * 1024 * 1024;

// The room in one notification at the watch's 124-byte MTU.
const NOTIFICATION_ROOM = 121;

type Upload = { path: string; total: number; next: number };

export class MockFileService {
  readonly files = new Map<string, Uint8Array>();
  readonly folders = new Set(FOLDERS);
  private reading: string | null = null;
  private upload: Upload | null = null;

  constructor(private readonly notify: (value: Uint8Array) => void) {
    this.files.set('/settings/theme.json', new TextEncoder().encode('{"accent":"#00a0ff"}'));
    this.folders.add('/apps/clock');
    this.files.set('/apps/clock/app.ntapp', new Uint8Array(1200));
    this.files.set('/apps/clock/state', new Uint8Array([1, 2, 3]));
  }

  // A fortnight of step logs and the alarm app's saved state, so the Steps and Alarms screens have something to show.
  addSampleRecords(today: Date) {
    for (let back = 0; back < 14; back++) {
      const day = startOfDay(today, back);
      // Some days have no log, as when the watch was off.
      if (day.getDate() % 5 !== 0) {
        const path = stepBlockPath(day);
        const block = this.files.get(path) ?? new Uint8Array(STEP_BLOCK_BYTES).fill(0xff);
        writeSlotsIntoBlock(block, day, sampleDay(day.getDate(), back === 0 ? today : null));
        this.files.set(path, block);
        this.makeParents(path);
      }
    }
    this.files.set('/apps/alarm/state', encodeAppState(sampleAlarmState()));
    this.makeParents('/apps/alarm/state');
  }

  // One command written to the transfer characteristic.
  transfer(bytes: Uint8Array) {
    const fields = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    const text = (at: number, length: number) => new TextDecoder().decode(bytes.subarray(at, at + length));
    switch (bytes[0]) {
      case COMMAND.read:
        this.reading = text(12, fields.getUint16(2, true));
        this.readChunk(fields.getUint32(4, true), fields.getUint32(8, true));
        return;
      case COMMAND.readPacing:
        this.readChunk(fields.getUint32(4, true), fields.getUint32(8, true));
        return;
      case COMMAND.write:
        this.startWrite(text(20, fields.getUint16(2, true)), fields.getUint32(4, true), fields.getUint32(16, true));
        return;
      case COMMAND.writeData:
        this.continueWrite(fields.getUint32(4, true), bytes.subarray(12, 12 + fields.getUint32(8, true)));
        return;
      case COMMAND.delete:
        this.notify(Uint8Array.from([COMMAND.deleteStatus, this.delete(text(4, fields.getUint16(2, true)))]));
        return;
      case COMMAND.makeDirectory: {
        const answer = new Uint8Array(16);
        answer[0] = COMMAND.makeDirectoryStatus;
        answer[1] = this.makeDirectory(text(16, fields.getUint16(2, true)));
        this.notify(answer);
        return;
      }
      case COMMAND.listDirectory:
        this.list(text(4, fields.getUint16(2, true)) || '/');
        return;
      case COMMAND.move: {
        const oldLength = fields.getUint16(2, true);
        const status = this.move(text(6, oldLength), text(7 + oldLength, fields.getUint16(4, true)));
        this.notify(Uint8Array.from([COMMAND.moveStatus, status]));
        return;
      }
    }
  }

  // ==========================================================================
  // Reading and writing
  // ==========================================================================

  private readChunk(offset: number, size: number) {
    const contents = this.reading === null ? undefined : this.files.get(this.reading);
    if (!contents) {
      this.notify(readData(this.reading === null ? STATUS.protocolError : STATUS.noFile, offset, 0, new Uint8Array(0)));
      return;
    }
    const length = Math.max(0, Math.min(size, contents.length - offset, NOTIFICATION_ROOM - 16));
    this.notify(readData(STATUS.ok, offset, contents.length, contents.subarray(offset, offset + length)));
    if (offset + length >= contents.length) {
      this.reading = null;
    }
  }

  private startWrite(path: string, offset: number, total: number) {
    const existing = this.files.get(path) ?? new Uint8Array(0);
    if (this.folders.has(path) || existing.length < offset) {
      this.notify(writePacing(STATUS.error, offset, 0));
      return;
    }
    this.makeParents(path);
    this.files.set(path, existing.slice(0, offset));
    this.upload = offset < total ? { path, total, next: offset } : null;
    this.notify(writePacing(STATUS.ok, offset, Math.min(this.free(), total - offset)));
  }

  private continueWrite(offset: number, data: Uint8Array) {
    const upload = this.upload;
    if (!upload || offset > upload.next || offset + data.length > upload.total) {
      this.notify(writePacing(STATUS.protocolError, offset, 0));
      return;
    }
    const old = this.files.get(upload.path) ?? new Uint8Array(0);
    const next = offset + data.length;
    const contents = new Uint8Array(Math.max(old.length, next));
    contents.set(old);
    contents.set(data, offset);
    this.files.set(upload.path, contents);
    upload.next = Math.max(upload.next, next);
    if (next >= upload.total) {
      this.upload = null;
    }
    this.notify(writePacing(STATUS.ok, next, Math.min(this.free(), upload.total - next)));
  }

  private free(): number {
    const used = [...this.files.values()].reduce((sum, contents) => sum + contents.length, 0);
    return TOTAL_BYTES - used;
  }

  // ==========================================================================
  // Folders
  // ==========================================================================

  private makeParents(path: string) {
    for (let parentEnd = path.indexOf('/', 1); parentEnd > 0; parentEnd = path.indexOf('/', parentEnd + 1)) {
      this.folders.add(path.slice(0, parentEnd));
    }
  }

  private makeDirectory(path: string): number {
    const trimmed = path.replace(/\/+$/, '');
    if (this.files.has(trimmed)) {
      return STATUS.error;
    }
    this.makeParents(trimmed);
    this.folders.add(trimmed);
    return STATUS.ok;
  }

  private delete(path: string): number {
    const trimmed = path.replace(/\/+$/, '');
    if (!trimmed) {
      return STATUS.error;
    }
    if (this.files.delete(trimmed)) {
      return STATUS.ok;
    }
    if (!this.folders.delete(trimmed)) {
      return STATUS.noFile;
    }
    for (const file of [...this.files.keys()]) {
      if (file.startsWith(`${trimmed}/`)) this.files.delete(file);
    }
    for (const folder of [...this.folders]) {
      if (folder.startsWith(`${trimmed}/`)) this.folders.delete(folder);
    }
    return STATUS.ok;
  }

  private move(from: string, to: string): number {
    const contents = this.files.get(from);
    if (!contents) {
      return STATUS.noFile;
    }
    this.files.delete(from);
    this.files.set(to, contents);
    return STATUS.ok;
  }

  private list(dir: string) {
    if (dir !== '/' && !this.folders.has(dir)) {
      this.notify(entry(this.files.has(dir) ? STATUS.error : STATUS.noFile, 0, 0, 0, 0, ''));
      return;
    }
    const entries: [string, number, number][] = [];
    for (const folder of this.folders) {
      if (parent(folder) === dir) entries.push([name(folder), 1, 0]);
    }
    for (const [path, contents] of this.files) {
      if (parent(path) === dir) entries.push([name(path), 0, contents.length]);
    }
    entries.forEach(([entryName, flags, size], index) =>
      this.notify(entry(STATUS.ok, index, entries.length, flags, size, entryName)),
    );
    this.notify(entry(STATUS.ok, entries.length, entries.length, 0, 0, ''));
  }
}

// ============================================================================
// Answers
// ============================================================================

function readData(status: number, offset: number, total: number, chunk: Uint8Array): Uint8Array {
  const answer = new Uint8Array(16 + chunk.length);
  const fields = new DataView(answer.buffer);
  answer[0] = COMMAND.readData;
  answer[1] = status;
  fields.setUint32(4, offset, true);
  fields.setUint32(8, total, true);
  fields.setUint32(12, chunk.length, true);
  answer.set(chunk, 16);
  return answer;
}

function writePacing(status: number, offset: number, free: number): Uint8Array {
  const answer = new Uint8Array(20);
  const fields = new DataView(answer.buffer);
  answer[0] = COMMAND.writePacing;
  answer[1] = status;
  fields.setUint32(4, offset, true);
  fields.setUint32(16, free, true);
  return answer;
}

function entry(status: number, index: number, total: number, flags: number, size: number, entryName: string) {
  const nameBytes = new TextEncoder().encode(entryName);
  const answer = new Uint8Array(28 + nameBytes.length);
  const fields = new DataView(answer.buffer);
  answer[0] = COMMAND.listDirectoryEntry;
  answer[1] = status;
  fields.setUint16(2, nameBytes.length, true);
  fields.setUint32(4, index, true);
  fields.setUint32(8, total, true);
  fields.setUint32(12, flags, true);
  fields.setUint32(24, size, true);
  answer.set(nameBytes, 28);
  return answer;
}

function parent(path: string): string {
  const slash = path.lastIndexOf('/');
  return slash <= 0 ? '/' : path.slice(0, slash);
}

function name(path: string): string {
  return path.slice(path.lastIndexOf('/') + 1);
}

// ============================================================================
// Sample records
// ============================================================================

// A made-up but plausible day: quiet nights, a walk in the morning and another after work; today stops at the time given.
function sampleDay(seed: number, until: Date | null): number[] {
  const last = until ? Math.floor((until.getHours() * 60 + until.getMinutes()) / 6) : SLOTS_PER_DAY;
  return Array.from({ length: SLOTS_PER_DAY }, (_, slot) => {
    const hour = slot / 10;
    const active = (hour > 7.5 && hour < 9) || (hour > 17 && hour < 18.5) ? 90 : hour > 7 && hour < 22 ? 12 : 0;
    return slot < last ? Math.round(active * (0.6 + ((slot * 37 + seed * 11) % 10) / 12)) : 0;
  });
}

// The alarm app's state as it saves it: two alarms in use of its twelve places, and its own keys.
function sampleAlarmState(): StateEntry[] {
  // Places not in use hold the app's default alarm, 08:00 once and off.
  const list = (key: string, kind: StateEntry['kind'], values: number[], unused: number): StateEntry => ({
    key,
    kind,
    length: 12,
    width: 1,
    values: [...values, ...new Array(12 - values.length).fill(unused)].map(BigInt),
  });
  const one = (key: string, kind: StateEntry['kind'], width: StateEntry['width'], value: bigint): StateEntry => ({
    key,
    kind,
    length: 0,
    width,
    values: [value],
  });
  const none = -(1n << 63n);
  return [
    one('alarm_count', 'int', 1, 2n),
    list('alarms.hour', 'int', [7, 9], 8),
    list('alarms.minute', 'int', [0, 30], 0),
    list('alarms.days', 'int', [0x1f, 0], 0),
    list('alarms.enabled', 'bool', [1, 0], 0),
    one('next_ring_at', 'instant', 8, none),
    one('next_ring_place', 'int', 1, -1n),
    one('snoozed_until', 'instant', 8, none),
    one('snoozed_place', 'int', 1, -1n),
    one('rang_at', 'instant', 8, none),
    one('ringing_place', 'int', 1, -1n),
  ];
}
