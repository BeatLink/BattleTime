// Gadgetbridge messages that the watch answers: settings, alarms and step history.
// The watch side lives in wasp/gadgetbridge.py; keep the two in step.

import type {
  Alarm,
  AlarmReply,
  PhoneToWatchMessage,
  SettingsReply,
  StepsReply,
  WatchSettingValues,
  WatchToPhoneMessage,
} from './gadgetbridge';

// Message types that come back as the answer to a request.
export type ReplyType = 'settings' | 'alarm' | 'steps';

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

// A MicroPython traceback ends with the line that says what went wrong.
function lastLine(text: string): string {
  const lines = text.trim().split(/\r?\n/);
  return lines[lines.length - 1] || 'The watch reported an error.';
}

export async function readWatchSettings(channel: RequestChannel): Promise<SettingsReply> {
  return channel.request({ t: 'settings' }, 'settings');
}

export async function writeWatchSettings(
  channel: RequestChannel,
  patch: Partial<WatchSettingValues>,
): Promise<SettingsReply> {
  return channel.request({ t: 'settings', ...patch }, 'settings');
}

export async function readAlarms(channel: RequestChannel): Promise<Required<Alarm>[]> {
  const reply: AlarmReply = await channel.request({ t: 'alarm' }, 'alarm');
  return reply.d.map(normaliseAlarm);
}

export const MAX_ALARMS = 12;

export async function writeAlarms(channel: RequestChannel, alarms: Alarm[]): Promise<Required<Alarm>[]> {
  if (alarms.length > MAX_ALARMS) {
    throw new Error(`The watch holds at most ${MAX_ALARMS} alarms.`);
  }
  const reply: AlarmReply = await channel.request(
    { t: 'alarm', d: alarms.map(normaliseAlarm) },
    'alarm',
  );
  return reply.d.map(normaliseAlarm);
}

export function normaliseAlarm(alarm: Alarm): Required<Alarm> {
  return {
    h: Math.min(23, Math.max(0, Math.floor(alarm.h))),
    m: Math.min(59, Math.max(0, Math.floor(alarm.m))),
    on: alarm.on ?? true,
    rep: (alarm.rep ?? 0) & 0x7f,
  };
}

export async function readSteps(channel: RequestChannel, day: Date): Promise<StepsReply> {
  return channel.request(
    { t: 'steps', y: day.getFullYear(), m: day.getMonth() + 1, d: day.getDate() },
    'steps',
  );
}
