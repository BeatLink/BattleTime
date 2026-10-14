// The notification left on the watch once an incoming call ends: missed, ignored or accepted.
import type { CallControlMessage, NotificationMessage } from '@/protocol/gadgetbridge';

import type { PhoneEvent } from './types';

type CallEvent = Extract<PhoneEvent, { type: 'call' }>;

export type CallOutcome = 'missed' | 'ignored' | 'accepted';

const OUTCOME_TEXT: Record<CallOutcome, string> = {
  missed: 'Missed call',
  ignored: 'Ignored call',
  accepted: 'Accepted call',
};

// Phone notifications take ids below 2^31 and the watch's own apps take negative ones, so these count up from 2^31.
const FIRST_SUMMARY_ID = 2 ** 31;

export class CallSummary {
  private ringing: { caller: string; outcome: CallOutcome } | null = null;
  private nextId = FIRST_SUMMARY_ID;

  /** Follows the phone's call, and gives the notification to send when an incoming call ends. */
  noteCall(event: CallEvent): NotificationMessage | null {
    switch (event.state) {
      case 'incoming':
        this.ringing = { caller: event.name || event.number || 'Unknown caller', outcome: 'missed' };
        return null;
      case 'start':
        if (this.ringing) {
          this.ringing.outcome = 'accepted';
        }
        return null;
      case 'outgoing':
        this.ringing = null;
        return null;
      case 'end': {
        const call = this.ringing;
        this.ringing = null;
        if (!call) {
          return null;
        }
        return { t: 'notify', id: this.nextId++, src: 'Phone', title: call.caller, body: OUTCOME_TEXT[call.outcome] };
      }
    }
  }

  /** Counts a call rejected or muted on the watch as ignored, unless it is answered after all. */
  noteWatchAnswer(command: CallControlMessage['n']): void {
    if (this.ringing && command !== 'ACCEPT' && this.ringing.outcome === 'missed') {
      this.ringing.outcome = 'ignored';
    }
  }
}
