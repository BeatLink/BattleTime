// Watch data from the terminal: settings, alarms, steps, weather, files and backups.
// Each command runs the same protocol code as the screen that does the same job.

import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { basename, dirname } from 'node:path';

import type { SettingsMessage } from '@/protocol/gadgetbridge';
import { createBackup, parseBackup, restoreBackup } from '@/protocol/backup';
import { setWatchTime } from '@/protocol/neotime';
import { installImage } from '@/protocol/packages';
import { runSetup, setupItems } from '@/protocol/setup';
import { encodeForWatch, decodeWatchLine } from '@/protocol/gadgetbridge';
import { readAlarms, readStatus, readWatchSettings, writeAlarms, writeWatchSettings } from '@/protocol/requests';
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
neotime apps [list|install <image>|remove <id>|hide <id>|show <id>|setup]
neotime backup <file>
neotime restore <file>
neotime listen [line] [--allow <hosts>] [--wait <seconds>]

  status prints the battery and today's steps. time sets the watch clock from this computer's. settings with no arguments prints them;
  brightness=3, theme=ocean, theme_colors='{"accent":"#ff8000"}' and the like
  change them. alarms with no arguments lists them, otherwise replaces
  them all: 07:30/12345 rings on weekdays (1 is Monday), 06:00/67/off is a
  weekend alarm turned off, and a bare time rings once. files uses NeoTime's
  file service; paths are absolute, such as /settings. apps installs a .ntapp
  or .ntlib image, and setup installs the face, launcher, apps and shared
  library this tool bundles onto a new watch. listen stays
  connected and prints what the watch sends, answering its apps' http
  requests for the hosts given; it runs the line given first, and stops
  after --wait seconds or at Ctrl+C.`;

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
    case 'list':
      for (const app of await packages.apps()) {
        say(`${app.id.padEnd(16)} ${app.kind.padEnd(5)} ${app.enabled ? '      ' : 'hidden'} ${app.name}`);
      }
      return;
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
      if (action === 'remove') await packages.remove(first);
      else await packages.enable(first, action === 'show');
      say(`${action === 'remove' ? 'Removed' : action === 'hide' ? 'Hid' : 'Showed'} ${first}.`);
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
