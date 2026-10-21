import { describe, expect, it } from '@jest/globals';

import { MockGadgetbridge } from '@/ble/mock-gadgetbridge';

import {
  appMessageData,
  appMessageFromData,
  appMessageNumber,
  encodeAppMessage,
  isName,
  LONGEST_LINE_BYTES,
  readAppMessage,
} from './app-message';
import { decodeWatchLine, type WatchToPhoneMessage } from './gadgetbridge';

describe('app messages to the watch', () => {
  it('go out as one GB line', () => {
    expect(encodeAppMessage({ topic: 'sos', numbers: [['on', 0]] })).toBe('GB({"t":"app","ch":"sos","d":{"on":0}})\n');
  });

  it('take numbers, booleans and one text from data', () => {
    const message = appMessageFromData('buzz-back', { count: 3, loud: true, note: 'Hi' });
    expect(message).toEqual({ topic: 'buzz-back', numbers: [['count', 3], ['loud', 1]], text: ['note', 'Hi'] });
    expect(appMessageData(message)).toEqual({ count: 3, loud: 1, note: 'Hi' });
    expect(appMessageNumber(message, 'count')).toBe(3);
    expect(appMessageNumber(message, 'missing')).toBeUndefined();
  });

  it('refuse data outside the limits', () => {
    const refusal = (data: unknown) => () => appMessageFromData('app', data);
    expect(refusal([1])).toThrow('must be an object');
    expect(refusal({ a: 1.5 })).toThrow('whole number');
    expect(refusal({ a: null })).toThrow('must be a number');
    expect(refusal({ a: 'x', b: 'y' })).toThrow('at most 1 text');
    expect(refusal({ a: 1, b: 2, c: 3, d: 4, e: 5 })).toThrow('at most 4 numbers');
    expect(refusal({ 'a b': 1 })).toThrow('is not a key');
    expect(refusal({ a: 3_000_000_000 })).toThrow('32-bit');
    expect(() => appMessageFromData('no topic', {})).toThrow('is not a topic');
    expect(() => encodeAppMessage({ topic: 'app', numbers: [['a', 1]], text: ['a', 'x'] })).toThrow('given twice');
  });

  it('name topics and keys with short plain names', () => {
    expect(isName('buzz-back') && isName('Remote_2') && isName('a'.repeat(32))).toBe(true);
    expect(isName('') || isName('a'.repeat(33)) || isName('a.b') || isName('é')).toBe(false);
  });

  it('escape other characters and refuse a line that is too long', () => {
    expect(encodeAppMessage({ topic: 'note', numbers: [], text: ['text', 'Zürich'] })).toContain('"text":"Z\\u00fcrich"');
    expect(() => encodeAppMessage({ topic: 'note', numbers: [], text: ['text', '😀'.repeat(60)] })).toThrow('at most 640');
    expect(encodeAppMessage({ topic: 'note', numbers: [], text: ['text', 'a'.repeat(590)] }).length).toBeLessThanOrEqual(
      LONGEST_LINE_BYTES + 1,
    );
  });
});

describe('app messages from the watch', () => {
  const read = (line: string) => readAppMessage(decodeWatchLine(line) as WatchToPhoneMessage);

  it('read back as messages', () => {
    expect(read('{"t":"app","ch":"remote","d":{"button":2,"held":false}}')).toEqual({
      topic: 'remote',
      numbers: [['button', 2], ['held', 0]],
    });
    expect(read('{"t":"app","ch":"remote"}')).toEqual({ topic: 'remote', numbers: [] });
    expect(read('{"t":"app","ch":"r","d":{"a":1.5,"b":"x"}}')).toEqual({ topic: 'r', numbers: [], text: ['b', 'x'] });
    expect(read('{"t":"app","ch":"bad topic","d":{}}')).toBeNull();
    expect(read('{"t":"findPhone","n":true}')).toBeNull();
  });
});

describe('the mock watch', () => {
  it('takes an app message and sends nothing back', () => {
    const watch = new MockGadgetbridge();
    expect(watch.handle(encodeAppMessage({ topic: 'sos', numbers: [['on', 0]] }))).toEqual([]);
  });
});
