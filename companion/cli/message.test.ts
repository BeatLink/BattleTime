import { describe, expect, it } from '@jest/globals';

import type { WatchSession } from './ble';
import { message } from './commands/send';

// The message command only sends a line, which is all this session records.
function fakeSession(): { session: WatchSession; sent: string[] } {
  const sent: string[] = [];
  const session = { send: async (text: string) => void sent.push(text) } as unknown as WatchSession;
  return { session, sent };
}

describe('the message command', () => {
  it('sends an app message to the watch app the topic names', async () => {
    const { session, sent } = fakeSession();
    await message(session, 'sos', '{"on":0}');
    await message(session, 'buzz-back', '{"count":3,"loud":true,"note":"Hi"}');
    expect(sent).toEqual([
      'GB({"t":"app","ch":"sos","d":{"on":0}})\n',
      'GB({"t":"app","ch":"buzz-back","d":{"count":3,"loud":1,"note":"Hi"}})\n',
    ]);
  });

  it('sends nothing when the message is not JSON or breaks the limits', async () => {
    const { session, sent } = fakeSession();
    await expect(message(session, 'sos', 'on')).rejects.toThrow('as a JSON object');
    await expect(message(session, 'bad topic', '{}')).rejects.toThrow('is not a topic');
    await expect(message(session, 'sos', '{"a":"x","b":"y"}')).rejects.toThrow('at most 1 text');
    expect(sent).toEqual([]);
  });
});
