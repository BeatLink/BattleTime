// Watch data from the terminal: settings, alarms, steps, weather, files and backups.
// Each command runs the same protocol code as the screen that does the same job.

import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { basename, dirname } from 'node:path';

import type { WatchSettingValues } from '@/protocol/gadgetbridge';
import { createBackup, parseBackup, restoreBackup } from '@/protocol/backup';
import { deleteFile, listDirectory, readFile, readMemory } from '@/protocol/files';
import { encodeSetTime } from '@/protocol/clock';
import { encodeForWatch, decodeWatchLine } from '@/protocol/gadgetbridge';
import { readAlarms, readWatchSettings, writeAlarms, writeWatchSettings } from '@/protocol/requests';
import { readStepHistory } from '@/protocol/steps';
import { readAbi, sendFile } from '@/protocol/transfer';
import { answerHttpRequest, parseHostList } from '@/services/http-proxy';
import { currentWeather, findPlace } from '@/services/weather';

import type { WatchSession } from '../ble';
import { ProgressBar, say } from '../output';

export const watchUsage = `waspos time
waspos settings [key=value ...]
waspos alarms [HH:MM[/days][/off] ...|none]
waspos steps [days]
waspos weather <place>
waspos files [ls [dir]|get <path> [dest]|put <file> <path>|rm <path>|mem]
waspos backup <file> [--logs]
waspos restore <file>
waspos listen [python] [--allow <hosts>] [--wait <seconds>]

  time sets the watch clock from this computer's. settings with no arguments prints them; brightness=3 face=<path> and the
  like change them. alarms with no arguments lists them, otherwise replaces
  them all: 07:30/12345 rings on weekdays (1 is Monday), 06:00/67/off is a
  weekend alarm turned off, and a bare time rings once. listen stays
  connected and prints what the watch sends, answering its apps' http
  requests for the hosts given; it runs the line given first, and stops
  after --wait seconds or at Ctrl+C.`;

const DAYS = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'];

function describeDays(rep: number): string {
  return rep === 0 ? 'once' : DAYS.filter((_, bit) => rep & (1 << bit)).join(' ');
}

// Turn a key=value argument into a setting, reading numbers and booleans as such.
function parseSetting(arg: string): Partial<WatchSettingValues> {
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
  } else if (raw.startsWith('[')) {
    value = JSON.parse(raw);
  }
  return { [key]: value } as Partial<WatchSettingValues>;
}

export async function setTime(session: WatchSession) {
  const now = new Date();
  await session.send(encodeSetTime(now));
  say(`Set the watch clock to ${now.toLocaleString()}.`);
}

export async function settings(session: WatchSession, args: string[]) {
  const reply = args.length
    ? await writeWatchSettings(session.requests, Object.assign({}, ...args.map(parseSetting)))
    : await readWatchSettings(session.requests);
  const { t: _t, faces, ...values } = reply;
  for (const [key, value] of Object.entries(values)) {
    say(`${key.padEnd(14)} ${JSON.stringify(value)}`);
  }
  say('faces:');
  for (const [path, label] of faces) {
    say(`  ${path === reply.face ? '*' : ' '} ${label.padEnd(16)} ${path}`);
  }
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
  const channel = session.packageChannel;
  switch (action) {
    case 'ls': {
      for (const entry of await listDirectory(channel, first ?? '')) {
        say(`${entry.directory ? 'dir ' : String(entry.size).padStart(6) + ' '} ${entry.path}${entry.directory ? '/' : ''}`);
      }
      return;
    }
    case 'get': {
      if (!first) throw new Error('Give the file to fetch.');
      const bar = new ProgressBar();
      const data = await readFile(channel, first, (p) => bar.update(p.sent, p.total, p.file));
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
      await sendFile(channel, second, data, {
        abi: await readAbi(channel),
        onProgress: (p) => bar.update(p.sent, p.total, p.file),
      });
      bar.done();
      say(`Sent ${data.length} bytes to ${second}.`);
      return;
    }
    case 'rm':
      if (!first) throw new Error('Give the file to delete.');
      await deleteFile(channel, first);
      say(`Deleted ${first}.`);
      return;
    case 'mem': {
      const memory = await readMemory(channel);
      say(`${memory.free} bytes free, ${memory.alloc} in use.`);
      return;
    }
    default:
      throw new Error(`Unknown files action: ${action}`);
  }
}

export async function backup(session: WatchSession, file: string, stepLogs: boolean) {
  const bar = new ProgressBar();
  const result = await createBackup(session.packageChannel, {
    stepLogs,
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
  await restoreBackup(session.packageChannel, saved, {
    abi: await readAbi(session.packageChannel),
    onProgress: (done, total, path) => bar.update(done, total, path),
  });
  bar.done();
  say(`Restored ${Object.keys(saved.files).length} files. Restart the watch with \`waspos reset\` to use them.`);
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
    await session.send(`${line}\r\n`);
  }
  await new Promise<void>((resolve) => {
    process.once('SIGINT', () => resolve());
    if (waitMs) {
      setTimeout(resolve, waitMs);
    }
  });
}
