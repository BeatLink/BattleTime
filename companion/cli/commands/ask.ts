// Asks a yes-or-no question on the watch with the bundled prompt app, run as a temporary app, and waits for the tap.

import { decodeWatchLine } from '@/protocol/gadgetbridge';
import { runTemporaryApp } from '@/protocol/packages';
import { bundledPromptImage, PROMPT_IMAGE_FILE, PROMPT_TEXT_FILE, promptAnswer } from '@/protocol/prompt';

import type { WatchSession } from '../ble';
import { say, warn } from '../output';

export const askUsage = `battletime ask "<question>" [--timeout <seconds>]

  ask shows the question on the watch with Yes and No buttons and waits for a tap. It prints yes and
  exits 0, or prints no and exits 1. With no tap within --timeout seconds (60 unless given) it closes
  the question and exits 2; it exits 3 when it cannot ask at all.`;

// The exit codes, so a script can tell a No from no answer.
export const ASK_EXIT = { yes: 0, no: 1, timeout: 2, failed: 3 } as const;

export const ASK_TIMEOUT_MS = 60000;

// The prompt image from this build's bundle, or an error saying the bundle lacks it.
export function requirePromptImage(image: Uint8Array | null = bundledPromptImage()): Uint8Array {
  if (!image) {
    throw new Error(`This build bundles no ${PROMPT_IMAGE_FILE}, so it has nothing to ask with.`);
  }
  return image;
}

export async function ask(
  session: Pick<WatchSession, 'listen' | 'files' | 'packages'>,
  question: string,
  timeoutMs = ASK_TIMEOUT_MS,
  image: Uint8Array = requirePromptImage(),
): Promise<number> {
  if (!question.trim()) {
    throw new Error('Give the question to ask.');
  }
  // Listening starts before the prompt opens, so even the quickest tap is heard.
  let settle: (answer: boolean | null) => void = () => {};
  const answered = new Promise<boolean | null>((resolve) => (settle = resolve));
  session.listen((line) => {
    const message = decodeWatchLine(line);
    const answer = message ? promptAnswer(message) : null;
    if (answer !== null) settle(answer);
  });

  const packages = await session.packages();
  await runTemporaryApp(await session.files(), packages, image, { [PROMPT_TEXT_FILE]: question });
  say('Asked on the watch; waiting for a tap.');

  const timer = setTimeout(() => settle(null), timeoutMs);
  const answer = await answered;
  clearTimeout(timer);
  if (answer === null) {
    await packages.stop().catch(() => undefined);
    warn(`No answer within ${Math.round(timeoutMs / 1000)} s, so the question was closed.`);
    return ASK_EXIT.timeout;
  }
  say(answer ? 'yes' : 'no');
  return answer ? ASK_EXIT.yes : ASK_EXIT.no;
}
