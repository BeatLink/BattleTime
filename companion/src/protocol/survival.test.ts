import { describe, expect, it } from '@jest/globals';

import { SURVIVAL_GUIDES, SURVIVAL_TOPICS } from '@/bundles/survival-guides';
import type { WatchFile } from '@/protocol/file-service';

import { FileMissingError } from './file-service';
import {
  guideBytes,
  guideIdsIn,
  LARGEST_SURVIVAL_TITLE_BYTES,
  MOST_SURVIVAL_GUIDES,
  packSurvivalList,
  parseSurvivalList,
  readSurvivalList,
  sendSurvivalGuides,
  SurvivalError,
  survivalGuidePath,
  SURVIVAL_LIST_PATH,
  topicNumber,
  type SurvivalFiles,
} from './survival';

// ============================================================================
// A watch's files in memory
// ============================================================================

class FakeFiles implements SurvivalFiles {
  readonly files = new Map<string, Uint8Array>();
  readonly written: string[] = [];

  async read(path: string): Promise<Uint8Array> {
    const contents = this.files.get(path);
    if (!contents) {
      throw new FileMissingError(`${path} does not exist.`);
    }
    return contents;
  }

  async write(path: string, contents: Uint8Array): Promise<void> {
    this.files.set(path, contents);
    this.written.push(path);
  }

  async remove(path: string): Promise<void> {
    if (!this.files.delete(path)) {
      throw new FileMissingError(`${path} does not exist.`);
    }
  }

  async list(directory: string): Promise<WatchFile[]> {
    return [...this.files.entries()]
      .filter(([path]) => path.startsWith(`${directory}/`))
      .map(([path, contents]) => ({
        name: path.slice(directory.length + 1),
        path,
        size: contents.length,
        directory: false,
      }));
  }
}

// ============================================================================
// The guides
// ============================================================================

describe('the bundled guides', () => {
  it('each keep a slot of their own that the watch has a screen for', () => {
    const slots = SURVIVAL_GUIDES.map((guide) => guide.slot);
    expect(new Set(slots).size).toBe(slots.length);
    expect(Math.max(...slots)).toBeLessThan(MOST_SURVIVAL_GUIDES);
    expect(new Set(SURVIVAL_GUIDES.map((guide) => guide.id)).size).toBe(SURVIVAL_GUIDES.length);
  });

  it('fill every topic, each guide under one the watch numbers', () => {
    for (const topic of SURVIVAL_TOPICS) {
      expect(SURVIVAL_GUIDES.some((guide) => guide.topic === topic.id)).toBe(true);
    }
    expect(SURVIVAL_GUIDES.map(topicNumber).every((number) => number >= 0)).toBe(true);
    expect(topicNumber(SURVIVAL_GUIDES.find((guide) => guide.id === 'nonviolent-action')!)).toBe(3);
  });

  it("use only plain ASCII, so every character is in the watch's font, and titles that fit its list", () => {
    for (const guide of SURVIVAL_GUIDES) {
      expect(guide.text).toMatch(/^[\x20-\x7e\n]+$/);
      expect(new TextEncoder().encode(guide.title).length).toBeLessThanOrEqual(LARGEST_SURVIVAL_TITLE_BYTES);
    }
  });
});

// ============================================================================
// The list file
// ============================================================================

describe('the list file', () => {
  it('packs a version, a count, then each slot, topic, title length and padded title', () => {
    const file = packSurvivalList([
      { slot: 0, topic: 0, title: 'CPR' },
      { slot: 9, topic: 1, title: 'Lost in the wild' },
    ]);

    expect(file.length).toBe(2 + 2 * 26);
    expect([...file.subarray(0, 8)]).toEqual([2, 2, 0, 0, 3, 67, 80, 82]);
    expect([...file.subarray(28, 31)]).toEqual([9, 1, 16]);
    expect(parseSurvivalList(file)).toEqual([
      { slot: 0, topic: 0, title: 'CPR' },
      { slot: 9, topic: 1, title: 'Lost in the wild' },
    ]);
  });

  it('reads nothing from a file in another format', () => {
    expect(parseSurvivalList(new Uint8Array([1, 1, 0, 1, 65]))).toEqual([]);
  });

  it('refuses a topic the watch does not have', () => {
    expect(() => packSurvivalList([{ slot: 0, topic: SURVIVAL_TOPICS.length, title: 'CPR' }])).toThrow(SurvivalError);
  });

  it('refuses a title too long for the watch', () => {
    expect(() => packSurvivalList([{ slot: 0, topic: 0, title: 'x'.repeat(24) }])).toThrow(SurvivalError);
  });

  it('names the guides it lists by id', () => {
    expect(
      guideIdsIn([
        { slot: 9, topic: 1, title: 'Lost' },
        { slot: 23, topic: 0, title: 'Unknown' },
        { slot: 0, topic: 0, title: 'CPR' },
      ]),
    ).toEqual(['lost', 'cpr']);
  });
});

// ============================================================================
// Sending
// ============================================================================

describe('sendSurvivalGuides', () => {
  it("writes each chosen guide and a list in the app's own order", async () => {
    const files = new FakeFiles();

    const result = await sendSurvivalGuides(files, ['lost', 'cpr']);

    expect(result).toEqual({
      entries: [
        { slot: 0, topic: 0, title: 'CPR' },
        { slot: 9, topic: 1, title: 'Lost in the wild' },
      ],
      sent: 2,
      removed: 0,
    });
    expect(files.written).toEqual([survivalGuidePath(0), survivalGuidePath(9), SURVIVAL_LIST_PATH]);
    expect(files.files.get('/survival/0')).toEqual(guideBytes(SURVIVAL_GUIDES[0]));
    expect(await readSurvivalList(files)).toEqual(result.entries);
  });

  it('sends again only the guides the watch lacks, and removes the ones left out', async () => {
    const files = new FakeFiles();
    await sendSurvivalGuides(files, ['cpr', 'burns']);
    files.written.length = 0;

    const result = await sendSurvivalGuides(files, ['cpr', 'water']);

    expect(result.sent).toBe(1);
    expect(result.removed).toBe(1);
    expect(files.written).toEqual([survivalGuidePath(10), SURVIVAL_LIST_PATH]);
    expect(files.files.has(survivalGuidePath(3))).toBe(false);
  });

  it('sends a guide again when the copy on the watch is another length', async () => {
    const files = new FakeFiles();
    await sendSurvivalGuides(files, ['cpr']);
    files.files.set(survivalGuidePath(0), new Uint8Array(3));

    expect((await sendSurvivalGuides(files, ['cpr'])).sent).toBe(1);
    expect(files.files.get(survivalGuidePath(0))).toEqual(guideBytes(SURVIVAL_GUIDES[0]));
  });

  it('with no guides chosen takes every guide and the list off, leftover files too', async () => {
    const files = new FakeFiles();
    await sendSurvivalGuides(files, ['cpr', 'burns']);
    files.files.set(survivalGuidePath(12), new Uint8Array(4));

    const result = await sendSurvivalGuides(files, []);

    expect(result).toEqual({ entries: [], sent: 0, removed: 3 });
    expect(files.files.size).toBe(0);
    expect(await readSurvivalList(files)).toEqual([]);
  });

  it('refuses a guide it does not know', async () => {
    await expect(sendSurvivalGuides(new FakeFiles(), ['cpr', 'kraken'])).rejects.toThrow('kraken');
  });
});
