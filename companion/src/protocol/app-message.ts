// App messages: a few values for one watch app, sent on its topic, as docs/companion-protocol.md's "App messages" describes them.
//
// This is the only TypeScript that writes or reads the {"t":"app",...} line; everything else deals in
// AppMessage, so the line can change without touching its users. core/src/protocol/app_message.rs mirrors it.

import { asciiJson, type WatchToPhoneMessage } from './gadgetbridge';

// ============================================================================
// Limits
// ============================================================================

// The longest topic or key, in characters.
export const MOST_NAME_CHARACTERS = 32;

// The most numbers one message may carry.
export const MOST_NUMBERS = 4;

// The most texts one message may carry.
export const MOST_TEXTS = 1;

// How much of a text the watch keeps, in bytes.
export const TEXT_BYTES_KEPT = 120;

// The longest line the watch reads, `GB(` and `)` included but not the newline.
export const LONGEST_LINE_BYTES = 640;

// ============================================================================
// The message
// ============================================================================

export type AppMessage = {
  // The watch app's id.
  topic: string;
  // Named whole numbers, in the order they were given.
  numbers: [string, number][];
  // One named text, which the watch cuts to TEXT_BYTES_KEPT bytes.
  text?: [string, string];
};

// Whether a topic or key is 1 to 32 letters, digits, _ or -.
export function isName(name: string): boolean {
  return /^[A-Za-z0-9_-]{1,32}$/.test(name);
}

// Throws with the reason when the watch could not take this message.
export function checkAppMessage(message: AppMessage): void {
  if (!isName(message.topic)) {
    throw new Error(`"${message.topic}" is not a topic: use 1 to ${MOST_NAME_CHARACTERS} letters, digits, _ and -.`);
  }
  if (message.numbers.length > MOST_NUMBERS) {
    throw new Error(`An app message carries at most ${MOST_NUMBERS} numbers.`);
  }
  const keys = [...message.numbers.map(([key]) => key), ...(message.text ? [message.text[0]] : [])];
  const seen = new Set<string>();
  for (const key of keys) {
    if (!isName(key)) {
      throw new Error(`"${key}" is not a key: use 1 to ${MOST_NAME_CHARACTERS} letters, digits, _ and -.`);
    }
    if (seen.has(key)) {
      throw new Error(`"${key}" is given twice.`);
    }
    seen.add(key);
  }
  const outside = message.numbers.find(([, value]) => !Number.isInteger(value) || value < -(2 ** 31) || value >= 2 ** 31);
  if (outside) {
    throw new Error(`"${outside[0]}" does not fit in a 32-bit whole number.`);
  }
}

// A message from an object of numbers, booleans and at most one string, checked against the limits; throws otherwise.
export function appMessageFromData(topic: string, data: unknown): AppMessage {
  if (!data || typeof data !== 'object' || Array.isArray(data)) {
    throw new Error("An app message's data must be an object.");
  }
  const message: AppMessage = { topic, numbers: [] };
  for (const [key, value] of Object.entries(data)) {
    if (typeof value === 'boolean') {
      message.numbers.push([key, value ? 1 : 0]);
    } else if (typeof value === 'number') {
      if (!Number.isInteger(value)) {
        throw new Error(`"${key}" must be a whole number.`);
      }
      message.numbers.push([key, value]);
    } else if (typeof value === 'string') {
      if (message.text) {
        throw new Error(`An app message carries at most ${MOST_TEXTS} text.`);
      }
      message.text = [key, value];
    } else {
      throw new Error(`"${key}" must be a number, true or false, or a text.`);
    }
  }
  checkAppMessage(message);
  return message;
}

// The values as one object, numbers first, as the window and plugins see them.
export function appMessageData(message: AppMessage): Record<string, number | string> {
  const data: Record<string, number | string> = Object.fromEntries(message.numbers);
  if (message.text) {
    data[message.text[0]] = message.text[1];
  }
  return data;
}

// A number by its key, or undefined when the message has none.
export function appMessageNumber(message: AppMessage, key: string): number | undefined {
  return message.numbers.find(([name]) => name === key)?.[1];
}

// ============================================================================
// Phone to watch
// ============================================================================

// The line GB({"t":"app","ch":...,"d":{...}}) for the watch; throws when it cannot be sent.
export function encodeAppMessage(message: AppMessage): string {
  checkAppMessage(message);
  const line = `GB(${asciiJson({ t: 'app', ch: message.topic, d: appMessageData(message) })})`;
  if (line.length > LONGEST_LINE_BYTES) {
    throw new Error(`The message is ${line.length} bytes as a line; the watch reads at most ${LONGEST_LINE_BYTES}.`);
  }
  return `${line}\n`;
}

// ============================================================================
// Watch to phone
// ============================================================================

// The app message in a decoded watch message, or null when it is another kind or has no valid topic.
export function readAppMessage(message: WatchToPhoneMessage): AppMessage | null {
  if (message.t !== 'app' || typeof message.ch !== 'string' || !isName(message.ch)) {
    return null;
  }
  const read: AppMessage = { topic: message.ch, numbers: [] };
  const data = message.d && typeof message.d === 'object' && !Array.isArray(message.d) ? message.d : {};
  for (const [key, value] of Object.entries(data)) {
    if (typeof value === 'boolean') {
      read.numbers.push([key, value ? 1 : 0]);
    } else if (typeof value === 'number' && Number.isInteger(value)) {
      read.numbers.push([key, value]);
    } else if (typeof value === 'string' && !read.text) {
      read.text = [key, value];
    }
  }
  return read;
}
