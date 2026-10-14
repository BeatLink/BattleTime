import { describe, expect, it, jest } from '@jest/globals';

import { MockLegacyDfuTarget } from '@/dfu/mock-legacy-target';
import { FileService } from '@/protocol/file-service';
import { PackagesClient } from '@/protocol/packages';
import { testImage } from '@/test-support/app-images';

import { ask, ASK_EXIT, requirePromptImage } from './commands/ask';

// What ask needs from a session: the file service, the packages client and the lines the watch sends.
class FakeSession extends MockLegacyDfuTarget {
  onLine: ((line: string) => void) | null = null;
  private filesOpened: Promise<FileService> | null = null;
  private packagesOpened: Promise<PackagesClient> | null = null;
  constructor() {
    super('AA:BB:CC:DD:EE:00', 'neotime', true);
  }
  listen(onLine: (line: string) => void) {
    this.onLine = onLine;
  }
  files() {
    this.filesOpened ??= FileService.open(this);
    return this.filesOpened;
  }
  packages() {
    this.packagesOpened ??= PackagesClient.open(this);
    return this.packagesOpened;
  }
}

const PROMPT = testImage('prompt', 'Prompt');

// Waits until the mock watch has opened the prompt, then taps the answer given.
async function tapWhenOpen(session: FakeSession, answer: number) {
  while (session.packagesService.openTemporaryApp !== 'prompt') {
    await new Promise((resolve) => setTimeout(resolve, 1));
  }
  session.onLine?.('some console output');
  session.onLine?.(JSON.stringify({ t: 'app', ch: 'remote', d: { button: 1 } }));
  session.onLine?.(JSON.stringify({ t: 'app', ch: 'prompt', d: { answer } }));
}

function captureOutput() {
  const said: string[] = [];
  const out = jest.spyOn(process.stdout, 'write').mockImplementation((text) => {
    said.push(String(text).trimEnd());
    return true;
  });
  const err = jest.spyOn(process.stderr, 'write').mockImplementation((text) => {
    said.push(String(text).trimEnd());
    return true;
  });
  return {
    said,
    restore: () => {
      out.mockRestore();
      err.mockRestore();
    },
  };
}

describe('the ask command', () => {
  it('writes the question for the prompt app and prints yes for a Yes tap', async () => {
    const session = new FakeSession();
    const output = captureOutput();
    const [code] = await Promise.all([ask(session, 'Lunch now?', 5000, PROMPT), tapWhenOpen(session, 1)]);
    output.restore();
    expect(code).toBe(ASK_EXIT.yes);
    expect(output.said).toContain('yes');
    expect(new TextDecoder().decode(session.fileService.files.get('/temp/prompt/text'))).toBe('Lunch now?');
  });

  it('prints no for a No tap, even one that comes before the prompt has opened', async () => {
    const session = new FakeSession();
    const output = captureOutput();
    const asking = ask(session, 'Deploy?', 5000, PROMPT);
    session.onLine?.(JSON.stringify({ t: 'app', ch: 'prompt', d: { answer: 0 } }));
    const code = await asking;
    output.restore();
    expect(code).toBe(ASK_EXIT.no);
    expect(output.said).toContain('no');
  });

  it('closes the prompt and says so when nobody answers in time', async () => {
    const session = new FakeSession();
    const output = captureOutput();
    const code = await ask(session, 'Anyone?', 20, PROMPT);
    output.restore();
    expect(code).toBe(ASK_EXIT.timeout);
    expect(session.packagesService.openTemporaryApp).toBeNull();
    expect(output.said.join('\n')).toContain('No answer within 0 s');
  });

  it('needs a question and the bundled prompt image', async () => {
    const session = new FakeSession();
    await expect(ask(session, '  ', 20, PROMPT)).rejects.toThrow('Give the question');
    expect(() => requirePromptImage(null)).toThrow('bundles no prompt.ntapp');
    expect(requirePromptImage(PROMPT)).toBe(PROMPT);
  });
});
