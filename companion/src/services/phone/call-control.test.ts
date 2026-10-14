import { describe, expect, it, jest } from '@jest/globals';

import { controlCall } from './call-control';
import { unavailablePhoneBridge } from './stub';
import type { PhoneBridge } from './types';

function bridgeWithCalls(): PhoneBridge {
  return {
    ...unavailablePhoneBridge,
    answerCall: jest.fn(async () => {}),
    rejectCall: jest.fn(async () => {}),
    silenceCall: jest.fn(async () => {}),
  };
}

describe('controlCall', () => {
  it('answers, rejects or silences the ringing call as the watch asks', async () => {
    const bridge = bridgeWithCalls();
    await controlCall(bridge, 'ACCEPT');
    expect(bridge.answerCall).toHaveBeenCalledTimes(1);
    await controlCall(bridge, 'REJECT');
    expect(bridge.rejectCall).toHaveBeenCalledTimes(1);
    await controlCall(bridge, 'IGNORE');
    expect(bridge.silenceCall).toHaveBeenCalledTimes(1);
  });

  it('passes on the bridge refusing', async () => {
    await expect(controlCall(unavailablePhoneBridge, 'IGNORE')).rejects.toThrow('only available in the app');
  });
});
