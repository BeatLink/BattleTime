import { describe, expect, it } from '@jest/globals';

import { bytesToBase64 } from '@/ble/encoding';
import { testImage } from '@/test-support/app-images';

import { bundledPromptImage, PROMPT_IMAGE_FILE, promptAnswer, TEMPORARY_ONLY_IDS } from './prompt';

describe('the prompt app', () => {
  it('finds its image in a bundle by file name, or says it is missing', () => {
    const image = testImage('prompt', 'Prompt');
    expect(bundledPromptImage([{ file: PROMPT_IMAGE_FILE, base64: bytesToBase64(image) }])).toEqual(image);
    expect(bundledPromptImage([{ file: 'timer.ntapp', base64: bytesToBase64(testImage('timer', 'Timer')) }])).toBeNull();
    expect(bundledPromptImage([])).toBeNull();
  });

  it('is the one bundled app that only runs as a temporary app', () => {
    expect([...TEMPORARY_ONLY_IDS]).toEqual(['prompt']);
  });

  it('reads Yes and No from the watch and ignores everything else', () => {
    expect(promptAnswer({ t: 'app', ch: 'prompt', d: { answer: 1 } })).toBe(true);
    expect(promptAnswer({ t: 'app', ch: 'prompt', d: { answer: 0 } })).toBe(false);
    expect(promptAnswer({ t: 'app', ch: 'prompt', d: { answer: 2 } })).toBeNull();
    expect(promptAnswer({ t: 'app', ch: 'prompt', d: 1 })).toBeNull();
    expect(promptAnswer({ t: 'app', ch: 'prompt' })).toBeNull();
    expect(promptAnswer({ t: 'app', ch: 'remote', d: { answer: 1 } })).toBeNull();
    expect(promptAnswer({ t: 'findPhone', n: true })).toBeNull();
  });
});
