// The Alarm plugin's page: it shows the synced alarms and writes each change straight to the watch.
// The plugin's logic is the shared one (plugins/shared/state-main.ts), which sends `snapshot` and takes `write`.

import { alarmsFromState, alarmStateChanges, clockText, DAY_NAMES, repeatLabel, type WatchAlarm } from '../../../../src/protocol/alarms';
import { addAlarm, alarmRoom, removeAlarm, setEnabled, setTime, toggleDay } from '../edits';

type Snapshot = { app: { state: Record<string, unknown> | null } | null };

const send = (message: unknown) => parent.postMessage({ neotime: message }, '*');
const list = document.getElementById('alarms') as HTMLElement;
const problem = document.getElementById('problem') as HTMLElement;
const add = document.getElementById('add') as HTMLButtonElement;

// The state the last snapshot carried, which every change is made against.
let state: Record<string, unknown> | null = null;
let alarms: WatchAlarm[] = [];

function element<Tag extends keyof HTMLElementTagNameMap>(tag: Tag, text?: string, className?: string) {
  const made = document.createElement(tag);
  if (text !== undefined) made.textContent = text;
  if (className) made.className = className;
  return made;
}

// Shows the change at once and sends it; the plugin answers with the state as the watch now has it.
function change(next: WatchAlarm[] | null) {
  if (!next || !state) return;
  try {
    send({ write: alarmStateChanges(state, next) });
    alarms = next;
    problem.textContent = '';
  } catch (error) {
    problem.textContent = (error as Error).message;
  }
  render();
}

function alarmRow(alarm: WatchAlarm, place: number): HTMLElement {
  const row = element('div', undefined, 'alarm');

  const time = element('input');
  time.type = 'time';
  time.value = clockText(alarm);
  time.setAttribute('aria-label', `Alarm ${place + 1} time`);
  time.addEventListener('change', () => change(setTime(alarms, place, time.value)));

  const switchLabel = element('label', undefined, 'switch');
  const on = element('input');
  on.type = 'checkbox';
  on.checked = alarm.enabled;
  on.addEventListener('change', () => change(setEnabled(alarms, place, on.checked)));
  switchLabel.append(on, 'On');

  const days = element('div', undefined, 'days');
  DAY_NAMES.forEach((name, bit) => {
    const day = element('button', name.slice(0, 2));
    day.title = name;
    day.setAttribute('aria-pressed', String((alarm.days & (1 << bit)) !== 0));
    day.addEventListener('click', () => change(toggleDay(alarms, place, bit)));
    days.append(day);
  });

  const remove = element('button', 'Remove', 'remove');
  remove.addEventListener('click', () => change(removeAlarm(alarms, place)));

  row.append(time, switchLabel, days, remove, element('span', repeatLabel(alarm.days), 'repeat'));
  return row;
}

function render() {
  const room = alarmRoom(state);
  add.disabled = !state || alarms.length >= room;
  add.textContent = alarms.length >= room && state ? `The watch holds ${room} alarms` : 'Add an alarm';
  if (!state) {
    return;
  }
  list.replaceChildren(...alarms.map(alarmRow));
  if (alarms.length === 0) {
    list.append(element('p', 'No alarms are set.', 'note'));
  }
}

window.addEventListener('message', (event) => {
  const message = event.data && event.data.neotime;
  if (!message) return;
  if ('error' in message) {
    problem.textContent = message.error ?? '';
  }
  const snapshot = message.snapshot as Snapshot | undefined;
  if (!snapshot) return;
  state = snapshot.app?.state ?? null;
  try {
    alarms = alarmsFromState(state);
  } catch (error) {
    state = null;
    alarms = [];
    list.replaceChildren(element('p', snapshot.app ? (error as Error).message : 'Not synced yet. Connect the watch, or sync now.', 'note'));
  }
  render();
});

add.addEventListener('click', () => change(addAlarm(alarms, alarmRoom(state))));
(document.getElementById('sync') as HTMLElement).addEventListener('click', () => send({ syncNow: true }));
send({});
