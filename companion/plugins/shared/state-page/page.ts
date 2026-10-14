// The settings page every app's plugin shares: the app's synced state, editable, and the watch's settings, steps, heart rate or battery when the plugin shows them.
// The build script bundles it with __PLUGIN_NAME__ and __PLUGIN_FIELDS__ filled in.

import { change, display, logDays, parse, rows, stepDays, type Cell, type FieldNotes, type SyncedKinds } from '../state-view';

declare const __PLUGIN_NAME__: string;
declare const __PLUGIN_FIELDS__: FieldNotes;

type Snapshot = {
  app: { hidden: boolean; state: Record<string, unknown> | null; kinds?: SyncedKinds } | null;
  settings?: Record<string, unknown> | null;
  steps?: { source: string; slots: number[] }[];
  heart?: { source: string; slots: number[] }[];
  battery?: { source: string; slots: (number | null)[] }[];
};

const send = (message: unknown) => parent.postMessage({ neotime: message }, '*');
const content = document.getElementById('content') as HTMLElement;
const problem = document.getElementById('problem') as HTMLElement;

function element(tag: string, text?: string, className?: string): HTMLElement {
  const made = document.createElement(tag);
  if (text !== undefined) {
    made.textContent = text;
  }
  if (className) {
    made.className = className;
  }
  return made;
}

// ============================================================================
// Editing one value
// ============================================================================

function editor(cell: Cell, onChange: (value: number | boolean) => void): HTMLElement {
  if (cell.kind === 'switch') {
    const box = element('input') as HTMLInputElement;
    box.type = 'checkbox';
    box.checked = cell.value === true;
    box.addEventListener('change', () => onChange(box.checked));
    return box;
  }
  const field = element('input') as HTMLInputElement;
  field.value = display(cell);
  // Times are shown for reading, since the watch sets them as it runs.
  field.readOnly = cell.kind === 'time' || cell.kind === 'fixed';
  field.addEventListener('change', () => {
    const value = parse(cell, field.value);
    if (value === null) {
      field.classList.add('wrong');
      return;
    }
    field.classList.remove('wrong');
    onChange(value);
  });
  return field;
}

// ============================================================================
// Sections
// ============================================================================

function appSection(app: Snapshot['app']): HTMLElement {
  const section = element('section');
  section.append(element('h2', __PLUGIN_NAME__));
  if (!app) {
    section.append(element('p', 'Not synced yet. Connect the watch, or sync now.', 'note'));
    return section;
  }
  if (app.hidden) {
    section.append(element('p', 'Hidden from the launcher on the watch.', 'note'));
  }
  const state = app.state;
  if (!state) {
    section.append(element('p', 'This app keeps nothing between uses.', 'note'));
    return section;
  }
  for (const row of rows(state, __PLUGIN_FIELDS__, app.kinds)) {
    const line = element('div', undefined, 'row');
    line.append(element('span', row.label, 'label'));
    for (const cell of row.cells) {
      const part = element('label', undefined, 'cell');
      if (row.cells.length > 1) {
        part.append(element('small', cell.label));
      }
      part.append(editor(cell, (value) => send({ write: change(state, cell, value) })));
      line.append(part);
    }
    section.append(line);
  }
  return section;
}

function settingsSection(settings: Record<string, unknown> | null | undefined): HTMLElement {
  const section = element('section');
  section.append(element('h2', 'Watch settings'));
  for (const [key, value] of Object.entries(settings ?? {})) {
    if (typeof value !== 'number' && typeof value !== 'boolean') {
      continue;
    }
    const cell: Cell = { key, label: key, kind: typeof value === 'boolean' ? 'switch' : 'number', value };
    const line = element('div', undefined, 'row');
    line.append(element('span', __PLUGIN_FIELDS__[key]?.label ?? key.replace(/_/g, ' '), 'label'));
    line.append(editor(cell, (next) => send({ writeSettings: { [key]: next } })));
    section.append(line);
  }
  return section;
}

function stepsSection(days: Snapshot['steps']): HTMLElement {
  const section = element('section');
  section.append(element('h2', 'Steps by day'));
  for (const day of stepDays(days ?? [])) {
    const line = element('div', undefined, 'row');
    line.append(element('span', day.date, 'label'), element('span', day.total.toLocaleString()));
    section.append(line);
  }
  return section;
}

// Each day of a heart rate or battery log as its average, with the lowest and highest after it.
function logSection(title: string, unit: string, days: { source: string; slots: (number | null)[] }[], empty?: number) {
  const section = element('section');
  section.append(element('h2', title));
  const shown = logDays(days, empty);
  if (shown.length === 0) {
    section.append(element('p', 'Nothing logged yet.', 'note'));
  }
  for (const day of shown) {
    const line = element('div', undefined, 'row');
    line.append(
      element('span', day.date, 'label'),
      element('span', `${day.average}${unit}`),
      element('small', `${day.lowest}${unit} to ${day.highest}${unit}`, 'note'),
    );
    section.append(line);
  }
  return section;
}

// ============================================================================
// Messages from the plugin
// ============================================================================

window.addEventListener('message', (event) => {
  const message = event.data && event.data.neotime;
  if (!message) {
    return;
  }
  if ('error' in message) {
    problem.textContent = message.error ?? '';
  }
  const snapshot = message.snapshot as Snapshot | undefined;
  if (!snapshot) {
    return;
  }
  content.replaceChildren(appSection(snapshot.app));
  if ('settings' in snapshot) {
    content.append(settingsSection(snapshot.settings));
  }
  if ('steps' in snapshot) {
    content.append(stepsSection(snapshot.steps));
  }
  if ('heart' in snapshot) {
    content.append(logSection('Heart rate by day', ' bpm', snapshot.heart ?? [], 0));
  }
  if ('battery' in snapshot) {
    content.append(logSection('Battery by day', ' %', snapshot.battery ?? []));
  }
});

(document.getElementById('sync') as HTMLElement).addEventListener('click', () => send({ syncNow: true }));
send({});
