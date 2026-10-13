// Watch data from the terminal: settings, alarms, steps, weather, files and backups.
// Each command runs the same protocol code as the screen that does the same job.

import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { basename, dirname } from 'node:path';

import type { SettingsMessage } from '@/protocol/gadgetbridge';
import { createBackup, parseBackup, restoreBackup } from '@/protocol/backup';
import { setWatchTime } from '@/protocol/neotime';
import { NOTE_FONTS, NOTE_PATH, removeNote, syncNote, type NoteFont } from '@/protocol/notes';
import { installImage, memoryText, PackagesError } from '@/protocol/packages';
import { readQuickApps, resetQuickApps, writeQuickApps } from '@/protocol/quick-apps';
import { runSetup, setupItems } from '@/protocol/setup';
import { encodeForWatch, decodeWatchLine } from '@/protocol/gadgetbridge';
import {
  fittedRecoverySlot,
  moveSlots,
  readAlarms,
  readSlots,
  readStatus,
  readWatchSettings,
  SLOT_PAGE_BYTES,
  writeAlarms,
  writeWatchSettings,
  type SlotsReport,
} from '@/protocol/requests';
import type { SlotRange } from '@/protocol/gadgetbridge';
import { readStepHistory } from '@/protocol/steps';
import { answerHttpRequest, parseHostList } from '@/services/http-proxy';
import { currentWeather, findPlace } from '@/services/weather';

import type { WatchSession } from '../ble';
import { ProgressBar, say } from '../output';

export const watchUsage = `neotime status
neotime time
neotime settings [key=value ...]
neotime alarms [HH:MM[/days][/off] ...|none]
neotime steps [days]
neotime weather <place>
neotime files [ls [dir]|get <path> [dest]|put <file> <path>|rm <path>|df]
neotime note <file> [small|normal|medium|large]|remove
neotime apps [list|install <image>|remove <id>|hide <id>|show <id>|setup]
neotime quick-apps [list|set <id> ...|reset]
neotime backup <file>
neotime restore <file>
neotime listen [line] [--allow <hosts>] [--wait <seconds>]
neotime slots [fit|recovery <start> <size>|update <start>]

  status prints the battery and today's steps. time sets the watch clock from this computer's. settings with no arguments prints them;
  brightness=3, theme=ocean, theme_colors='{"accent":"#ff8000"}' and the like
  change them. alarms with no arguments lists them, otherwise replaces
  them all: 07:30/12345 rings on weekdays (1 is Monday), 06:00/67/off is a
  weekend alarm turned off, and a bare time rings once. files uses NeoTime's
  file service; paths are absolute, such as /settings. note sends a text file
  for the watch's Notes app to show, in the font given, or removes it. apps installs a .ntapp
  or .ntlib image, list shows each app with the memory it needs, hide and show
  turn one off and on unless the watch refuses, saying why, and setup installs the
  face, launcher, apps and shared library this tool bundles onto a new watch. quick-apps
  lists the apps a swipe left or right on the face steps through, set replaces them with
  the ids given in order, and reset goes back to the watch's default. listen stays
  connected and prints what the watch sends, answering its apps' http
  requests for the hosts given; it runs the line given first, and stops
  after --wait seconds or at Ctrl+C. slots shows where the update and recovery
  slots sit in the external flash; fit shrinks the recovery slot to its image,
  and recovery or update moves a slot, in bytes (0x... works). The watch copies
  the recovery image along, then restarts.`;

const DAYS = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'];

function describeDays(rep: number): string {
  return rep === 0 ? 'once' : DAYS.filter((_, bit) => rep & (1 << bit)).join(' ');
}

// Turn a key=value argument into a setting, reading numbers and booleans as such.
function parseSetting(arg: string): Omit<SettingsMessage, 't'> {
  const equals = arg.indexOf('=');
  if (equals < 1) {
    throw new Error(`Give settings as key=value, not ${arg}.`);
  }
  const key = arg.slice(0, equals);
  const raw = arg.slice(equals + 1);
  let value: unknown = raw;
  if (raw === 'true' || raw === 'false') {
    value = raw === 'true';
  } else if (/^-?\d+$/.test(raw)) {
    value = Number(raw);
  } else if (/^[[{]/.test(raw)) {
    value = JSON.parse(raw);
  }
  return { [key]: value } as Omit<SettingsMessage, 't'>;
}

export async function status(session: WatchSession) {
  const reply = await readStatus(session.requests);
  say(`Battery ${reply.bat}% (${(reply.mv / 1000).toFixed(2)} V)${reply.charging ? ', charging' : ''}`);
  say(reply.steps === null ? 'No step counter.' : `${reply.steps} steps today.`);
}

function hexRange([start, size]: SlotRange): string {
  return `0x${start.toString(16).padStart(6, '0')} to 0x${(start + size).toString(16).padStart(6, '0')}, ${size / 1024} KB`;
}

export function describeSlots(report: SlotsReport): string[] {
  return [
    `update     ${hexRange(report.update)}`,
    `recovery   ${hexRange(report.recovery)}${report.image ? `, image ${report.image} bytes` : ', empty'}`,
    `slots end by 0x${report.area.toString(16)}; the update slot is ${(report.firmware + SLOT_PAGE_BYTES) / 1024} KB`,
  ];
}

// The move the arguments ask for, worked out against where the slots are now; null when nothing would change.
export function slotsMove(args: string[], report: SlotsReport): { update?: SlotRange; recovery?: SlotRange } | null {
  const number = (text: string | undefined, what: string) => {
    const value = Number(text);
    if (text === undefined || !Number.isInteger(value) || value < 0) {
      throw new Error(`Give the ${what} in bytes, such as 0x65000, not ${text ?? 'nothing'}.`);
    }
    return value;
  };
  const same = (first: SlotRange, second: SlotRange) => first[0] === second[0] && first[1] === second[1];
  let move: { update?: SlotRange; recovery?: SlotRange };
  switch (args[0]) {
    case 'fit':
      move = { recovery: fittedRecoverySlot(report) };
      break;
    case 'recovery':
      move = { recovery: [number(args[1], 'start'), number(args[2], 'size')] };
      break;
    case 'update':
      move = { update: [number(args[1], 'start'), report.firmware + SLOT_PAGE_BYTES] };
      break;
    default:
      throw new Error('Use slots, slots fit, slots recovery <start> <size> or slots update <start>.');
  }
  const unchanged =
    (!move.recovery || same(move.recovery, report.recovery)) && (!move.update || same(move.update, report.update));
  return unchanged ? null : move;
}

export async function slots(session: WatchSession, args: string[]) {
  const report = await readSlots(session.requests);
  if (!args.length) {
    describeSlots(report).forEach((line) => say(line));
    return;
  }
  const move = slotsMove(args, report);
  if (!move) {
    say('The slots are already there.');
    return;
  }
  const moved = await moveSlots(session.requests, move);
  describeSlots(moved).forEach((line) => say(line));
  say('The watch restarts to use them.');
}

export async function setTime(session: WatchSession) {
  const now = new Date();
  await setWatchTime(session, now);
  say(`Set the watch clock to ${now.toLocaleString()}.`);
}

export async function settings(session: WatchSession, args: string[]) {
  const reply = args.length
    ? await writeWatchSettings(session.requests, Object.assign({}, ...args.map(parseSetting)))
    : await readWatchSettings(session.requests);
  const { t: _t, themes, ...values } = reply;
  for (const [key, value] of Object.entries(values)) {
    say(`${key.padEnd(14)} ${JSON.stringify(value)}`);
  }
  say(`themes         ${themes.join(', ')}`);
}

// HH:MM, then optionally /<day digits> and /off.
export function parseAlarm(arg: string) {
  const match = /^(\d{1,2}):(\d{2})(?:\/([1-7]+))?(\/off)?$/.exec(arg);
  if (!match) {
    throw new Error(`Give alarms as HH:MM[/days][/off], not ${arg}.`);
  }
  const rep = [...(match[3] ?? '')].reduce((mask, digit) => mask | (1 << (Number(digit) - 1)), 0);
  return { h: Number(match[1]), m: Number(match[2]), on: !match[4], rep };
}

export async function alarms(session: WatchSession, args: string[]) {
  const list =
    args.length === 0
      ? await readAlarms(session.requests)
      : await writeAlarms(session.requests, args[0] === 'none' ? [] : args.map(parseAlarm));
  if (list.length === 0) {
    say('No alarms.');
  }
  for (const alarm of list) {
    const time = `${String(alarm.h).padStart(2, '0')}:${String(alarm.m).padStart(2, '0')}`;
    say(`${time}  ${alarm.on ? 'on ' : 'off'}  ${describeDays(alarm.rep)}`);
  }
}

export async function steps(session: WatchSession, days: number) {
  const history = await readStepHistory(session.requests, days);
  const top = Math.max(1, ...history.map((day) => day.total));
  for (const day of history) {
    const bar = '#'.repeat(Math.round((day.total / top) * 30));
    const label = day.date.toDateString().slice(0, 10);
    say(`${label}  ${day.hours.length ? String(day.total).padStart(6) : '     -'}  ${bar}`);
  }
}

export async function weather(session: WatchSession, place: string) {
  const report = await currentWeather(await findPlace(place));
  await session.send(encodeForWatch(report));
  say(`Sent ${report.txt}, ${Math.round(report.temp - 273.15)} C in ${report.loc}.`);
}

export async function files(session: WatchSession, args: string[]) {
  const [action = 'ls', first, second] = args;
  const service = await session.files();
  switch (action) {
    case 'ls': {
      for (const entry of await service.list(first ?? '/')) {
        say(`${entry.directory ? 'dir ' : String(entry.size).padStart(6) + ' '} ${entry.path}${entry.directory ? '/' : ''}`);
      }
      return;
    }
    case 'get': {
      if (!first) throw new Error('Give the file to fetch.');
      const bar = new ProgressBar();
      const data = await service.read(first, (p) => bar.update(p.sent, p.total, p.file));
      bar.done();
      const dest = second ?? basename(first);
      writeFileSync(dest, data);
      say(`Saved ${data.length} bytes to ${dest}.`);
      return;
    }
    case 'put': {
      if (!first || !second) throw new Error('Give the local file and the path on the watch.');
      const data = new Uint8Array(readFileSync(first));
      const bar = new ProgressBar();
      await service.write(second, data, (p) => bar.update(p.sent, p.total, p.file));
      bar.done();
      say(`Sent ${data.length} bytes to ${second}.`);
      return;
    }
    case 'rm':
      if (!first) throw new Error('Give the file to delete.');
      await service.remove(first);
      say(`Deleted ${first}.`);
      return;
    case 'df': {
      const storage = await service.storage();
      say(`${storage.free} bytes free.`);
      return;
    }
    default:
      throw new Error(`Unknown files action: ${action}`);
  }
}

export async function apps(session: WatchSession, args: string[]) {
  const [action = 'list', first] = args;
  const packages = await session.packages();
  switch (action) {
    case 'list': {
      const listing = await packages.listing();
      for (const app of listing.apps) {
        const need = app.need === null ? '' : memoryText(app.need);
        say(`${app.id.padEnd(16)} ${app.kind.padEnd(5)} ${app.enabled ? '      ' : 'hidden'} ${need.padStart(8)}  ${app.name}`);
      }
      if (listing.room !== null) say(`Each app may use up to ${memoryText(listing.room)} while it is open.`);
      return;
    }
    case 'install': {
      if (!first) throw new Error('Give the compiled .ntapp or .ntlib image.');
      const bar = new ProgressBar();
      const header = await installImage(await session.files(), packages, new Uint8Array(readFileSync(first)), (p) =>
        bar.update(p.sent, p.total, p.file),
      );
      bar.done();
      say(`Installed ${header.name}${header.kind === 'library' ? ' as the shared library' : ` as ${header.id}`}.`);
      return;
    }
    case 'remove':
    case 'hide':
    case 'show':
      if (!first) throw new Error('Give the id of the app.');
      if (action === 'remove') {
        await packages.remove(first);
        say(`Removed ${first}.`);
        return;
      }
      try {
        const answer = await packages.enable(first, action === 'show');
        const need = answer.need === null ? '' : `; it needs ${memoryText(answer.need)}`;
        say(`${action === 'hide' ? 'Hid' : 'Showed'} ${first}${need}.`);
      } catch (error) {
        if (error instanceof PackagesError && error.why !== null) {
          throw new Error(`The watch refused to ${action} ${first}: ${error.message}.`);
        }
        throw error;
      }
      return;
    case 'setup': {
      const items = setupItems();
      const bar = new ProgressBar();
      await runSetup(await session.files(), packages, items, (step) =>
        bar.update(step.sent, step.total, `${step.index + 1}/${step.count} ${step.item.header.name}`),
      );
      bar.done();
      say(`Installed ${items.length} images. The watch shows its face now.`);
      return;
    }
    default:
      throw new Error(`Unknown apps action: ${action}`);
  }
}

export async function backup(session: WatchSession, file: string) {
  const bar = new ProgressBar();
  const result = await createBackup(await session.files(), {
    onProgress: (done, total, path) => bar.update(done, total, path),
  });
  bar.done();
  mkdirSync(dirname(file), { recursive: true });
  writeFileSync(file, `${JSON.stringify(result, null, 1)}\n`);
  say(`Saved ${Object.keys(result.files).length} files to ${file}.`);
}

export async function restore(session: WatchSession, file: string) {
  const saved = parseBackup(readFileSync(file, 'utf8'));
  const bar = new ProgressBar();
  await restoreBackup(await session.files(), saved, {
    onProgress: (done, total, path) => bar.update(done, total, path),
  });
  bar.done();
  say(`Restored ${Object.keys(saved.files).length} files. Restart the watch to use them.`);
}

// Stay connected, print what the watch says and answer its http requests, until interrupted or out of time.
export async function listen(
  session: WatchSession,
  allow: string | undefined,
  line: string | undefined,
  waitMs: number | undefined,
) {
  const hosts = parseHostList(allow ?? '');
  session.listen((line) => {
    say(line);
    const message = decodeWatchLine(line);
    if (message?.t === 'http' && 'url' in message) {
      void answerHttpRequest(message, hosts).then((reply) => {
        say(`  -> ${reply.err ? `refused: ${reply.err}` : `${reply.resp?.length ?? 0} characters`}`);
        return session.send(encodeForWatch(reply));
      });
    }
  });
  say(`Listening${hosts.length ? `, answering http for ${hosts.join(', ')}` : ''}. Press Ctrl+C to stop.`);
  if (line) {
    await session.send(`${line}\n`);
  }
  await new Promise<void>((resolve) => {
    process.once('SIGINT', () => resolve());
    if (waitMs) {
      setTimeout(resolve, waitMs);
    }
  });
}

// Sends a text file for the watch's Notes app, or takes it off with `remove`.
// The watch face's quick-app ring: list it, replace it, or go back to the default.
export async function quickApps(session: WatchSession, args: string[]) {
  const [action = 'list', ...ids] = args;
  const service = await session.files();
  switch (action) {
    case 'list': {
      const onWatch = await readQuickApps(service);
      const installed = new Set((await (await session.packages()).apps()).map((app) => app.id));
      onWatch.ids.forEach((id, index) => say(`${String(index + 1).padStart(2)}. ${id}${installed.has(id) ? '' : '  (not installed, skipped)'}`));
      if (onWatch.ids.length === 0) say('The ring is empty.');
      if (!onWatch.fromFile) say("This is the watch's default ring.");
      return;
    }
    case 'set':
      if (ids.length === 0) throw new Error('Give the app ids in ring order, or use reset.');
      await writeQuickApps(service, ids);
      say(`The ring is now ${ids.join(', ')}.`);
      return;
    case 'reset':
      await resetQuickApps(service);
      say("The watch is back on its default ring.");
      return;
    default:
      throw new Error(`quick-apps takes list, set or reset, not ${action}.`);
  }
}

export async function note(session: WatchSession, args: string[]) {
  const [file, font = 'normal'] = args;
  if (!file) {
    throw new Error('Give the text file to show, or remove.');
  }
  const service = await session.files();
  if (file === 'remove') {
    await removeNote(service);
    say('Removed the note.');
    return;
  }
  if (!(NOTE_FONTS as readonly string[]).includes(font)) {
    throw new Error(`The font is one of ${NOTE_FONTS.join(', ')}, not ${font}.`);
  }
  const bytes = new Uint8Array(readFileSync(file));
  await syncNote(service, bytes, font as NoteFont, session.deviceId, null, false);
  say(`Sent ${basename(file)} (${bytes.length} bytes) to ${NOTE_PATH}, in the ${font} font.`);
}
