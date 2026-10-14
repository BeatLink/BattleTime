// Files on this device: choosing them with the system's file chooser, and saving what the app made as a download.

// ============================================================================
// Choosing files
// ============================================================================

export type PickedFile = { name: string; data: Uint8Array };

// How long after the window gets focus back a chooser with no files counts as cancelled.
const CANCEL_GRACE_MS = 1000;

// Opens the file chooser and gives the files chosen, or none when it is cancelled.
export function chooseFiles(options: { accept?: string; multiple?: boolean } = {}): Promise<File[]> {
  return new Promise((resolve) => {
    const input = document.createElement('input');
    input.type = 'file';
    if (options.accept) input.accept = options.accept;
    input.multiple = options.multiple ?? false;
    let settled = false;
    const finish = (files: File[]) => {
      if (settled) return;
      settled = true;
      window.removeEventListener('focus', onFocus);
      resolve(files);
    };
    // Some web views never send the cancel event, so a return to the window with nothing chosen counts as one.
    const onFocus = () => setTimeout(() => finish(Array.from(input.files ?? [])), CANCEL_GRACE_MS);
    input.addEventListener('change', () => finish(Array.from(input.files ?? [])));
    input.addEventListener('cancel', () => finish([]));
    window.addEventListener('focus', onFocus);
    input.click();
  });
}

// Reads a chosen file's bytes.
export async function readChosenFile(file: File): Promise<PickedFile> {
  return { name: file.name, data: new Uint8Array(await file.arrayBuffer()) };
}

// Opens the file chooser for one file and reads it, or gives null when it is cancelled.
export async function pickFile(accept?: string): Promise<PickedFile | null> {
  const [file] = await chooseFiles({ accept });
  return file ? readChosenFile(file) : null;
}

// Opens the file chooser for any number of files and reads them all.
export async function pickFiles(accept?: string): Promise<PickedFile[]> {
  return Promise.all((await chooseFiles({ accept, multiple: true })).map(readChosenFile));
}

// ============================================================================
// Saving files
// ============================================================================

// Hands bytes or text to the web view's own download, under the given name.
export function saveFile(name: string, contents: Uint8Array | string, type = 'application/octet-stream'): void {
  const body = typeof contents === 'string' ? contents : contents.slice().buffer;
  const url = URL.createObjectURL(new Blob([body], { type }));
  const link = document.createElement('a');
  link.href = url;
  link.download = name;
  link.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}
