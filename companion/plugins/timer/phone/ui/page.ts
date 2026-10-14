// The Timer plugin's page: it shows the synced timer and sets the length the watch starts from.
// The plugin's logic is the shared one (plugins/shared/state-main.ts), which sends `snapshot` and takes `write`.

import { chosenDuration, durationText, statusText, timerStatus, watchNow, type TimerState } from '../status';

type Snapshot = { app: { state: TimerState | null } | null };

const send = (message: unknown) => parent.postMessage({ neotime: message }, '*');
const byId = <Kind extends HTMLElement>(id: string) => document.getElementById(id) as Kind;
const chosen = byId<HTMLElement>('chosen');
const status = byId<HTMLElement>('status');
const problem = byId<HTMLElement>('problem');
const form = byId<HTMLFieldSetElement>('set');
const minutes = byId<HTMLInputElement>('minutes');
const seconds = byId<HTMLInputElement>('seconds');

let state: TimerState | null = null;

function render() {
  if (!state) return;
  chosen.textContent = durationText(state.chosen_duration ?? 0);
  status.textContent = statusText(timerStatus(state, watchNow()));
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
  form.disabled = !state;
  if (!state) {
    status.textContent = snapshot.app
      ? 'The timer has not saved anything yet; open it on the watch once.'
      : 'Not synced yet. Connect the watch, or sync now.';
    return;
  }
  const length = Math.round((state.chosen_duration ?? 0) / 1000);
  minutes.value = String(Math.floor(length / 60));
  seconds.value = String(length % 60).padStart(2, '0');
  render();
});

byId<HTMLElement>('save').addEventListener('click', () => {
  const length = chosenDuration(minutes.value, seconds.value);
  minutes.classList.toggle('wrong', length === null);
  seconds.classList.toggle('wrong', length === null);
  if (length !== null) {
    send({ write: { chosen_duration: length } });
  }
});
byId<HTMLElement>('sync').addEventListener('click', () => send({ syncNow: true }));
// The time left counts down between syncs.
setInterval(render, 1000);
send({});
