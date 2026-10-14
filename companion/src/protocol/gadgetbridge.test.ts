import { describe, expect, it } from '@jest/globals';

import { answerStateRequest, musicMessages, watchFaceNumber } from './gadgetbridge';

describe('watchFaceNumber', () => {
  it('gives the number the watch makes from a face id', () => {
    expect(watchFaceNumber('analog')).toBe(0x3114e811);
    expect(watchFaceNumber('digital')).toBe(708936699);
    expect(watchFaceNumber('')).toBe(0x011c9dc5);
  });
});

describe('answerStateRequest', () => {
  const media = { playing: true, artist: 'Night Owls', track: 'Late Train', durationSec: 200, positionSec: 30 };

  it('sends the music playing again when the watch asks for it', () => {
    expect(answerStateRequest({ t: 'request', n: 'music' }, media, true)).toEqual([
      { t: 'musicinfo', artist: 'Night Owls', album: undefined, track: 'Late Train', dur: 200 },
      { t: 'musicstate', state: 'play', position: 30 },
    ]);
  });

  it('sends no music when none is known or forwarding is off', () => {
    expect(answerStateRequest({ t: 'request', n: 'music' }, null, true)).toEqual([]);
    expect(answerStateRequest({ t: 'request', n: 'music' }, media, false)).toEqual([]);
  });

  it('answers a navigation request with an empty step, since the app follows no navigation', () => {
    expect(answerStateRequest({ t: 'request', n: 'nav' }, media, true)).toEqual([{ t: 'nav' }]);
  });
});

describe('musicMessages', () => {
  it('says a paused track is paused', () => {
    expect(musicMessages({ playing: false, positionSec: 5 })[1]).toEqual({ t: 'musicstate', state: 'pause', position: 5 });
  });
});
