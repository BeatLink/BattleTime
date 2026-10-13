import { describe, expect, it, jest } from '@jest/globals';

import { MockGadgetbridge } from '@/ble/mock-gadgetbridge';
import { decodeWatchLine, encodeForWatch, type PhoneToWatchMessage } from '@/protocol/gadgetbridge';
import { ReplyRouter, type SlotsReport } from '@/protocol/requests';

import type { WatchSession } from './ble';
import { describeSlots, slots, slotsMove } from './commands/watch';

const REPORT: SlotsReport = {
  t: 'slots',
  update: [0, 392 * 1024],
  recovery: [0x65000, 388 * 1024],
  area: 0x100000,
  firmware: 388 * 1024,
  image: 171428,
  restart: false,
};

// A session whose requests are answered by the mock watch.
function mockSession(): { session: WatchSession; sent: PhoneToWatchMessage[] } {
  const watch = new MockGadgetbridge();
  const sent: PhoneToWatchMessage[] = [];
  const router: ReplyRouter = new ReplyRouter(async (message) => {
    sent.push(message);
    for (const line of watch.handle(encodeForWatch(message)) ?? []) {
      const decoded = decodeWatchLine(line);
      if (decoded) router.push(decoded);
    }
  }, 1000);
  return { session: { requests: router } as unknown as WatchSession, sent };
}

function printed(run: () => Promise<void>): Promise<string> {
  const lines: string[] = [];
  const write = jest.spyOn(process.stdout, 'write').mockImplementation((text) => {
    lines.push(String(text));
    return true;
  });
  return run()
    .then(() => lines.join(''))
    .finally(() => write.mockRestore());
}

describe('slots command', () => {
  it('describes both slots in hex with their sizes', () => {
    expect(describeSlots(REPORT)).toEqual([
      'update     0x000000 to 0x062000, 392 KB',
      'recovery   0x065000 to 0x0c6000, 388 KB, image 171428 bytes',
      'slots end by 0x100000; the update slot is 392 KB',
    ]);
    expect(describeSlots({ ...REPORT, image: 0 })[1]).toMatch(/, empty$/);
  });

  it('works out each kind of move', () => {
    expect(slotsMove(['fit'], REPORT)).toEqual({ recovery: [0x65000, 168 * 1024] });
    expect(slotsMove(['recovery', '0xc6000', '176128'], REPORT)).toEqual({ recovery: [0xc6000, 176128] });
    expect(slotsMove(['update', '0x200000'], REPORT)).toEqual({ update: [0x200000, 392 * 1024] });
    expect(slotsMove(['recovery', '0x65000', String(388 * 1024)], REPORT)).toBeNull();
  });

  it('refuses arguments it cannot read', () => {
    expect(() => slotsMove(['recovery', 'here', '1'], REPORT)).toThrow('Give the start in bytes');
    expect(() => slotsMove(['update'], REPORT)).toThrow('not nothing');
    expect(() => slotsMove(['shuffle'], REPORT)).toThrow('Use slots, slots fit');
  });

  it('prints the slots, or moves them and says the watch restarts', async () => {
    const { session, sent } = mockSession();

    expect(await printed(() => slots(session, []))).toContain('recovery   0x065000');
    const moved = await printed(() => slots(session, ['fit']));

    expect(moved).toContain('recovery   0x065000 to 0x08f000, 168 KB');
    expect(moved).toContain('The watch restarts to use them.');
    expect(sent.at(-1)).toEqual({ t: 'slots', recovery: [0x65000, 168 * 1024] });
    expect(await printed(() => slots(session, ['fit']))).toContain('already there');
  });
});
