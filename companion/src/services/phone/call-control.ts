// What the phone does when the watch's call screen answers, rejects or mutes a call.
import type { CallControlMessage } from '@/protocol/gadgetbridge';

import type { PhoneBridge } from './types';

export function controlCall(bridge: PhoneBridge, command: CallControlMessage['n']): Promise<void> {
  switch (command) {
    case 'ACCEPT':
      return bridge.answerCall();
    case 'REJECT':
      return bridge.rejectCall();
    case 'IGNORE':
      return bridge.silenceCall();
  }
}
