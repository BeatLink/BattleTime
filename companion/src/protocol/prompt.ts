// The watch's prompt app: a temporary app that shows a question with Yes and No buttons.
//
// The question goes in /temp/prompt/text, and a tap sends {"t":"app","ch":"prompt","d":{"answer":1}} for Yes
// or {"answer":0} for No, after which the app closes itself.

import { base64ToBytes } from '@/ble/encoding';
import { BUNDLED_IMAGES, type BundledImage } from '@/bundles/watch-apps';

import { appMessageNumber, readAppMessage } from './app-message';
import type { WatchToPhoneMessage } from './gadgetbridge';

export const PROMPT_IMAGE_FILE = 'prompt.ntapp';

export const PROMPT_CHANNEL = 'prompt';

// The data file the prompt app shows.
export const PROMPT_TEXT_FILE = 'text';

export const PROMPT_ID = 'prompt';

// The ids of bundled apps that only ever run as temporary apps, so setting up a watch never installs them.
export const TEMPORARY_ONLY_IDS: ReadonlySet<string> = new Set([PROMPT_ID]);

// The bundled prompt image, or null when the bundle was built without it.
export function bundledPromptImage(bundle: BundledImage[] = BUNDLED_IMAGES): Uint8Array | null {
  const entry = bundle.find((image) => image.file === PROMPT_IMAGE_FILE);
  return entry ? base64ToBytes(entry.base64) : null;
}

// True for Yes and false for No, or null when the message is not a well-formed prompt answer.
export function promptAnswer(message: WatchToPhoneMessage): boolean | null {
  const read = readAppMessage(message);
  if (!read || read.topic !== PROMPT_CHANNEL) {
    return null;
  }
  const answer = appMessageNumber(read, 'answer');
  return answer === 1 ? true : answer === 0 ? false : null;
}
