/**
 * @jest-environment jsdom
 */
import { afterEach, describe, expect, it, jest } from '@jest/globals';

import { chooseFiles, pickFile, pickFiles, saveFile } from './device-files';

// ============================================================================
// Helpers
// ============================================================================

// Catches the file input the chooser makes, instead of opening a real chooser.
function catchChooser() {
  const inputs: HTMLInputElement[] = [];
  jest.spyOn(HTMLInputElement.prototype, 'click').mockImplementation(function (this: HTMLInputElement) {
    inputs.push(this);
  });
  return inputs;
}

// Puts files in an input as the chooser would and announces them.
function choose(input: HTMLInputElement, files: File[]) {
  Object.defineProperty(input, 'files', { value: files, configurable: true });
  input.dispatchEvent(new Event('change'));
}

// A chosen file with its bytes, since jsdom's File cannot give them back.
function fakeFile(name: string, text: string): File {
  const bytes = Uint8Array.from(text, (character) => character.charCodeAt(0));
  return { name, arrayBuffer: async () => bytes.buffer } as unknown as File;
}

afterEach(() => {
  jest.restoreAllMocks();
  jest.useRealTimers();
});

// ============================================================================
// Choosing
// ============================================================================

describe('choosing files', () => {
  it('opens a chooser for the given kinds and gives what is chosen', async () => {
    const inputs = catchChooser();
    const chosen = chooseFiles({ accept: '.zip', multiple: true });
    expect(inputs).toHaveLength(1);
    expect(inputs[0].type).toBe('file');
    expect(inputs[0].accept).toBe('.zip');
    expect(inputs[0].multiple).toBe(true);
    const file = new File(['PK'], 'fw.zip');
    choose(inputs[0], [file]);
    await expect(chosen).resolves.toEqual([file]);
  });

  it('gives nothing when the chooser is cancelled', async () => {
    const inputs = catchChooser();
    const chosen = chooseFiles();
    inputs[0].dispatchEvent(new Event('cancel'));
    await expect(chosen).resolves.toEqual([]);
  });

  it('counts a return to the window with nothing chosen as a cancel', async () => {
    jest.useFakeTimers();
    const inputs = catchChooser();
    const chosen = chooseFiles();
    window.dispatchEvent(new Event('focus'));
    jest.advanceTimersByTime(1000);
    await expect(chosen).resolves.toEqual([]);
    expect(inputs).toHaveLength(1);
  });

  it('reads the chosen file, or gives null when there is none', async () => {
    const inputs = catchChooser();
    const one = pickFile('.txt');
    choose(inputs[0], [fakeFile('note.txt', 'hi')]);
    const picked = await one;
    expect(picked?.name).toBe('note.txt');
    expect(Array.from(picked?.data ?? [])).toEqual([104, 105]);

    const none = pickFile();
    inputs[1].dispatchEvent(new Event('cancel'));
    await expect(none).resolves.toBeNull();
  });

  it('reads every chosen file', async () => {
    const inputs = catchChooser();
    const many = pickFiles();
    choose(inputs[0], [fakeFile('a.ntapp', 'a'), fakeFile('b.ntapp', 'bc')]);
    const picked = await many;
    expect(picked.map((file) => [file.name, file.data.length])).toEqual([
      ['a.ntapp', 1],
      ['b.ntapp', 2],
    ]);
  });
});

// ============================================================================
// Saving
// ============================================================================

describe('saving a file', () => {
  it('hands the contents to a download under the name, then lets the link go', () => {
    jest.useFakeTimers();
    const blobs: Blob[] = [];
    const createObjectURL = jest.fn((blob: Blob) => {
      blobs.push(blob);
      return 'blob:saved';
    });
    const revokeObjectURL = jest.fn();
    Object.assign(URL, { createObjectURL, revokeObjectURL });
    const clicked: HTMLAnchorElement[] = [];
    jest.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(function (this: HTMLAnchorElement) {
      clicked.push(this);
    });

    saveFile('report.json', '{}', 'application/json');

    expect(clicked[0].download).toBe('report.json');
    expect(clicked[0].href).toBe('blob:saved');
    expect(blobs[0].type).toBe('application/json');
    expect(revokeObjectURL).not.toHaveBeenCalled();
    jest.advanceTimersByTime(1000);
    expect(revokeObjectURL).toHaveBeenCalledWith('blob:saved');
  });

  it('saves bytes as they are', () => {
    const blobs: Blob[] = [];
    Object.assign(URL, {
      createObjectURL: (blob: Blob) => {
        blobs.push(blob);
        return 'blob:bytes';
      },
      revokeObjectURL: () => {},
    });
    jest.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => {});
    saveFile('dump.bin', new Uint8Array([1, 2, 3]));
    expect(blobs[0].size).toBe(3);
    expect(blobs[0].type).toBe('application/octet-stream');
  });
});
