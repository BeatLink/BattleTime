// The Remote screen's editor: a button as plain text fields, and back into a checked button.

import {
  HTTP_METHODS,
  isLibraryIcon,
  type AndroidIntent,
  type BuiltinAction,
  type HttpMethod,
  type RemoteAction,
  type RemoteActionKind,
  type RemoteButton,
} from '@/protocol/remote';

export type AndroidIntentType = AndroidIntent['type'];

// Every field the editor shows, for every kind of action, so switching kinds keeps what was typed.
export type RemoteButtonForm = {
  icon: string;
  label: string;
  kind: RemoteActionKind;
  method: HttpMethod;
  url: string;
  // One "Name: value" per line.
  headers: string;
  body: string;
  command: string;
  intentType: AndroidIntentType;
  uri: string;
  packageName: string;
  className: string;
  intentAction: string;
  // One "key=value" per line.
  extras: string;
  builtin: BuiltinAction;
};

export const EMPTY_FORM: RemoteButtonForm = {
  icon: 'play',
  label: '',
  kind: 'builtin',
  method: 'GET',
  url: '',
  headers: '',
  body: '',
  command: '',
  intentType: 'deeplink',
  uri: '',
  packageName: '',
  className: '',
  intentAction: '',
  extras: '',
  builtin: 'playpause',
};

const joinPairs = (pairs: Record<string, string> | undefined, separator: string) =>
  Object.entries(pairs ?? {})
    .map(([key, value]) => `${key}${separator}${value}`)
    .join('\n');

// Reads lines of key and value split at the first `separator`, ignoring blank lines.
export function parsePairs(text: string, separator: string, what: string): Record<string, string> {
  const pairs: Record<string, string> = {};
  for (const line of text.split('\n')) {
    if (line.trim() === '') continue;
    const at = line.indexOf(separator);
    const key = at > 0 ? line.slice(0, at).trim() : '';
    if (!key) {
      throw new Error(`Write each ${what} as name${separator === ':' ? ': ' : '='}value, not "${line.trim()}".`);
    }
    pairs[key] = line.slice(at + 1).trim();
  }
  return pairs;
}

// The editor's fields for an existing button.
export function formFromButton(button: RemoteButton): RemoteButtonForm {
  const form: RemoteButtonForm = { ...EMPTY_FORM, icon: button.icon, label: button.label, kind: button.action.kind };
  const action = button.action;
  switch (action.kind) {
    case 'http':
      return { ...form, method: action.method, url: action.url, headers: joinPairs(action.headers, ': '), body: action.body ?? '' };
    case 'command':
      return { ...form, command: action.command };
    case 'builtin':
      return { ...form, builtin: action.name };
    case 'android': {
      const intent = action.intent;
      switch (intent.type) {
        case 'deeplink':
          return { ...form, intentType: 'deeplink', uri: intent.uri };
        case 'activity':
          return {
            ...form,
            intentType: 'activity',
            packageName: intent.packageName,
            className: intent.className,
            intentAction: intent.action ?? '',
            extras: joinPairs(intent.extras, '='),
          };
        case 'broadcast':
          return {
            ...form,
            intentType: 'broadcast',
            intentAction: intent.action,
            packageName: intent.packageName ?? '',
            extras: joinPairs(intent.extras, '='),
          };
      }
    }
  }
}

function needed(value: string, what: string): string {
  const trimmed = value.trim();
  if (!trimmed) {
    throw new Error(`Give the ${what}.`);
  }
  return trimmed;
}

function withPairs<Key extends string>(key: Key, pairs: Record<string, string>): Partial<Record<Key, Record<string, string>>> {
  return Object.keys(pairs).length ? ({ [key]: pairs } as Record<Key, Record<string, string>>) : {};
}

function intentFromForm(form: RemoteButtonForm): AndroidIntent {
  const extras = parsePairs(form.extras, '=', 'extra');
  switch (form.intentType) {
    case 'deeplink':
      return { type: 'deeplink', uri: needed(form.uri, 'link to open') };
    case 'activity':
      return {
        type: 'activity',
        packageName: needed(form.packageName, 'package'),
        className: needed(form.className, 'activity class'),
        ...(form.intentAction.trim() ? { action: form.intentAction.trim() } : {}),
        ...withPairs('extras', extras),
      };
    case 'broadcast':
      return {
        type: 'broadcast',
        action: needed(form.intentAction, 'broadcast action'),
        ...(form.packageName.trim() ? { packageName: form.packageName.trim() } : {}),
        ...withPairs('extras', extras),
      };
  }
}

function actionFromForm(form: RemoteButtonForm): RemoteAction {
  switch (form.kind) {
    case 'http': {
      const url = needed(form.url, 'address to call');
      if (!/^https?:\/\//i.test(url)) {
        throw new Error('The address starts with http:// or https://.');
      }
      if (!(HTTP_METHODS as readonly string[]).includes(form.method)) {
        throw new Error(`The method is one of ${HTTP_METHODS.join(', ')}.`);
      }
      return {
        kind: 'http',
        method: form.method,
        url,
        ...withPairs('headers', parsePairs(form.headers, ':', 'header')),
        ...(form.body && form.method !== 'GET' ? { body: form.body } : {}),
      };
    }
    case 'command':
      return { kind: 'command', command: needed(form.command, 'command to run') };
    case 'android':
      return { kind: 'android', intent: intentFromForm(form) };
    case 'builtin':
      return { kind: 'builtin', name: form.builtin };
  }
}

// The button the fields describe; throws with a sentence for the screen when a field is missing or malformed.
export function buttonFromForm(form: RemoteButtonForm): RemoteButton {
  if (!isLibraryIcon(form.icon)) {
    throw new Error(`The shared library has no icon called ${form.icon}.`);
  }
  return { icon: form.icon, label: form.label.trim(), action: actionFromForm(form) };
}

// Moves the item at `from` by `step` places, keeping it inside the list.
export function moveItem<Item>(items: readonly Item[], from: number, step: number): Item[] {
  const to = Math.min(items.length - 1, Math.max(0, from + step));
  const next = [...items];
  const [item] = next.splice(from, 1);
  next.splice(to, 0, item);
  return next;
}
