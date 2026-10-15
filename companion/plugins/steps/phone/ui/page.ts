// The Steps plugin's page: bar charts of the last fortnight and of today's slots, from the synced step logs.
// The plugin's logic is the shared one (plugins/shared/state-main.ts) with the `steps` view, which sends the last 14 days.

import { bars, fortnight, todaySlots, type SyncedDay } from '../chart';

type Snapshot = { steps?: SyncedDay[] };

const SVG = 'http://www.w3.org/2000/svg';
const WIDTH = 280;
const LABEL_ROOM = 14;

const send = (message: unknown) => parent.postMessage({ battletime: message }, '*');
const byId = (id: string) => document.getElementById(id) as unknown as HTMLElement;

function shape(tag: string, attributes: Record<string, string | number>, text?: string): SVGElement {
  const made = document.createElementNS(SVG, tag);
  for (const [name, value] of Object.entries(attributes)) made.setAttribute(name, String(value));
  if (text !== undefined) made.textContent = text;
  return made;
}

// Bars on a baseline with a label under each that has one; a tap on a bar reads it out.
function drawChart(target: HTMLElement, values: number[], labels: string[], height: number, onPick?: (index: number) => void) {
  const plot = height - LABEL_ROOM;
  const drawn = bars(values, WIDTH, plot, values.length > 50 ? 0.3 : 3);
  const parts: SVGElement[] = [shape('line', { x1: 0, x2: WIDTH, y1: plot, y2: plot, class: 'base' })];
  drawn.forEach((bar, index) => {
    const rect = shape('rect', { x: bar.x, y: bar.y, width: bar.width, height: bar.height, rx: values.length > 50 ? 0 : 2 });
    if (onPick) {
      // A clear rectangle the bar's full height makes a short bar easy to tap.
      const hit = shape('rect', { x: bar.x, y: 0, width: bar.width, height: plot, opacity: 0 });
      hit.addEventListener('click', () => {
        target.querySelectorAll('rect.picked').forEach((other) => other.classList.remove('picked'));
        rect.classList.add('picked');
        onPick(index);
      });
      parts.push(rect, hit);
    } else {
      parts.push(rect);
    }
    if (labels[index]) {
      parts.push(shape('text', { x: bar.x + bar.width / 2, y: height - 2 }, labels[index]));
    }
  });
  target.replaceChildren(...parts);
}

window.addEventListener('message', (event) => {
  const message = event.data && event.data.battletime;
  if (!message) return;
  if ('error' in message) byId('problem').textContent = message.error ?? '';
  const snapshot = message.snapshot as Snapshot | undefined;
  if (!snapshot) return;
  const days = snapshot.steps ?? [];
  const today = new Date();
  const fourteen = fortnight(days, today);
  byId('today').textContent = fourteen[fourteen.length - 1].total.toLocaleString();
  const readout = byId('readout');
  drawChart(byId('fortnight'), fourteen.map((day) => day.total), fourteen.map((day) => day.label), 120, (index) => {
    const day = fourteen[index];
    readout.textContent = day.logged ? `${day.detail}: ${day.total.toLocaleString()} steps` : `${day.detail}: no log on the watch`;
  });
  // Labels every three hours, at the slot each hour starts on.
  const hourLabels = Array.from({ length: 240 }, (_, slot) => (slot % 30 === 0 ? String(slot / 10).padStart(2, '0') : ''));
  drawChart(byId('slots'), todaySlots(days, today), hourLabels, 100);
  if (days.length === 0) {
    readout.textContent = 'Not synced yet. Connect the watch, or sync now.';
  }
});

byId('sync').addEventListener('click', () => send({ syncNow: true }));
send({});
