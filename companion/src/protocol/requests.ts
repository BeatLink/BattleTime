// Gadgetbridge messages that the watch answers: settings, status and slots.
// The watch side is described in docs/companion-protocol.md in the NeoTime tree; keep the two in step.

import type {
  PhoneToWatchMessage,
  SettingsReply,
  SlotRange,
  SlotsReply,
  StatusReply,
  SettingsMessage,
  WatchToPhoneMessage,
} from './gadgetbridge';

// Message types that come back as the answer to a request.
export type ReplyType = 'settings' | 'status' | 'slots';

type ReplyOf<T extends ReplyType> = Extract<WatchToPhoneMessage, { t: T }>;

// What a request needs: a way to send a message and wait for the watch's answer of one type.
export interface RequestChannel {
  request<T extends ReplyType>(message: PhoneToWatchMessage, reply: T): Promise<ReplyOf<T>>;
}

// How long the watch has to answer before a request fails.
const REPLY_TIMEOUT_MS = 10000;

type Waiter = {
  type: ReplyType;
  resolve: (message: WatchToPhoneMessage) => void;
  reject: (error: Error) => void;
  timer: ReturnType<typeof setTimeout>;
};

// Matches the watch's answers to the requests waiting for them, oldest first.
export class ReplyRouter implements RequestChannel {
  private waiting: Waiter[] = [];
  private send: (message: PhoneToWatchMessage) => Promise<void>;

  constructor(send?: (message: PhoneToWatchMessage) => Promise<void>, private readonly timeoutMs = REPLY_TIMEOUT_MS) {
    this.send = send ?? (() => Promise.reject(new Error('Not connected to a watch.')));
  }

  // Change how requests reach the watch, for an owner whose send function changes.
  setSender(send: (message: PhoneToWatchMessage) => Promise<void>) {
    this.send = send;
  }

  request<T extends ReplyType>(message: PhoneToWatchMessage, reply: T): Promise<ReplyOf<T>> {
    return new Promise<ReplyOf<T>>((resolve, reject) => {
      const waiter: Waiter = {
        type: reply,
        resolve: resolve as (message: WatchToPhoneMessage) => void,
        reject,
        timer: setTimeout(() => {
          this.drop(waiter);
          reject(new Error('The watch did not reply.'));
        }, this.timeoutMs),
      };
      this.waiting.push(waiter);
      this.send(message).catch((error: Error) => {
        this.drop(waiter);
        clearTimeout(waiter.timer);
        reject(error);
      });
    });
  }

  // Offer a message from the watch. Returns true when a request took it.
  push(message: WatchToPhoneMessage): boolean {
    if (message.t === 'error') {
      // The watch reports a failed message without saying which, so it fails the oldest request.
      const waiter = this.waiting[0];
      if (!waiter) {
        return false;
      }
      this.settle(waiter);
      waiter.reject(new Error(lastLine(message.msg)));
      return true;
    }
    const waiter = this.waiting.find((candidate) => candidate.type === message.t);
    if (!waiter) {
      return false;
    }
    this.settle(waiter);
    waiter.resolve(message);
    return true;
  }

  reset() {
    for (const waiter of this.waiting) {
      clearTimeout(waiter.timer);
      waiter.reject(new Error('The watch disconnected.'));
    }
    this.waiting = [];
  }

  private settle(waiter: Waiter) {
    clearTimeout(waiter.timer);
    this.drop(waiter);
  }

  private drop(waiter: Waiter) {
    const index = this.waiting.indexOf(waiter);
    if (index >= 0) {
      this.waiting.splice(index, 1);
    }
  }
}

// A multi-line error ends with the line that says what went wrong.
function lastLine(text: string): string {
  const lines = text.trim().split(/\r?\n/);
  return lines[lines.length - 1] || 'The watch reported an error.';
}

export async function readWatchSettings(channel: RequestChannel): Promise<SettingsReply> {
  return channel.request({ t: 'settings' }, 'settings');
}

export async function writeWatchSettings(
  channel: RequestChannel,
  patch: Omit<SettingsMessage, 't'>,
): Promise<SettingsReply> {
  return channel.request({ t: 'settings', ...patch }, 'settings');
}

export async function readStatus(channel: RequestChannel): Promise<StatusReply> {
  return channel.request({ t: 'status' }, 'status');
}

// ============================================================================
// Slots
// ============================================================================

export type SlotsReport = Exclude<SlotsReply, { err: string }>;

export const SLOT_PAGE_BYTES = 4096;
// The watch refuses a recovery slot smaller than this.
export const SMALLEST_RECOVERY_SLOT_BYTES = 64 * 1024;

async function slotsRequest(channel: RequestChannel, move: { update?: SlotRange; recovery?: SlotRange }) {
  const reply = await channel.request({ t: 'slots', ...move }, 'slots');
  if ('err' in reply) {
    throw new Error(`The watch refused: ${reply.err}.`);
  }
  return reply;
}

export async function readSlots(channel: RequestChannel): Promise<SlotsReport> {
  return slotsRequest(channel, {});
}

// Moves the slots given; the watch copies the recovery image along and then restarts.
export async function moveSlots(
  channel: RequestChannel,
  move: { update?: SlotRange; recovery?: SlotRange },
): Promise<SlotsReport> {
  return slotsRequest(channel, move);
}

// The recovery slot at its start, shrunk to the whole pages its image needs, or the smallest slot allowed.
export function fittedRecoverySlot(report: SlotsReport): SlotRange {
  const pages = Math.ceil(report.image / SLOT_PAGE_BYTES) * SLOT_PAGE_BYTES;
  return [report.recovery[0], Math.max(pages, SMALLEST_RECOVERY_SLOT_BYTES)];
}
