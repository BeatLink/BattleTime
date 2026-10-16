import { afterEach, describe, expect, it } from '@jest/globals';

import { mkdtempSync } from 'node:fs';
import { createServer, type Server, type Socket } from 'node:net';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { WatchSession } from './ble';
import { DaemonClient, DaemonLink, SOCKET_NAME, socketPath } from './daemon';

type Request = { id: number; command: string; args: Record<string, unknown> };

// A stand-in for the app's socket that answers from `answers` and records every request.
class FakeApp {
  readonly requests: Request[] = [];
  readonly path = join(mkdtempSync(join(tmpdir(), 'battletime-daemon-')), SOCKET_NAME);
  private readonly server: Server;
  private client: Socket | null = null;

  constructor(
    private readonly answers: Record<string, (args: Record<string, unknown>) => unknown>,
    private readonly silent = false,
  ) {
    this.server = createServer((socket) => {
      this.client = socket;
      let buffered = '';
      socket.setEncoding('utf8');
      socket.on('data', (chunk: string) => {
        buffered += chunk;
        const lines = buffered.split('\n');
        buffered = lines.pop() ?? '';
        for (const line of lines) this.answer(socket, JSON.parse(line) as Request);
      });
    });
  }

  listen(): Promise<void> {
    return new Promise((resolve) => this.server.listen(this.path, resolve));
  }

  push(event: string, payload: unknown) {
    this.client?.write(`${JSON.stringify({ event, payload })}\n`);
  }

  close(): Promise<void> {
    this.client?.destroy();
    return new Promise((resolve) => this.server.close(() => resolve()));
  }

  commands(): string[] {
    return this.requests.map((request) => request.command);
  }

  private answer(socket: Socket, request: Request) {
    this.requests.push(request);
    if (this.silent) {
      return;
    }
    const answer = this.answers[request.command];
    const reply = answer
      ? { id: request.id, ok: true, value: answer(request.args) ?? null }
      : { id: request.id, ok: false, error: `Unknown command: ${request.command}` };
    socket.write(`${JSON.stringify(reply)}\n`);
  }
}

const apps: FakeApp[] = [];

async function startApp(answers: FakeApp['answers'], silent = false): Promise<FakeApp> {
  const app = new FakeApp(answers, silent);
  await app.listen();
  apps.push(app);
  return app;
}

afterEach(async () => {
  await Promise.all(apps.splice(0).map((app) => app.close()));
});

function connectedTo(id: string, mode = 'application') {
  return () => ({ connection: 'connected', mode, watch: { id, name: 'BattleTime' } });
}

describe('socketPath', () => {
  it('prefers the given socket, then the runtime folder, then the app data folder', () => {
    expect(socketPath({ BATTLETIME_SOCKET: '/x.sock', XDG_RUNTIME_DIR: '/run/user/1' })).toBe('/x.sock');
    expect(socketPath({ XDG_RUNTIME_DIR: '/run/user/1' })).toBe('/run/user/1/battletime-companion.sock');
    expect(socketPath({ XDG_DATA_HOME: '/data' })).toBe('/data/com.battletime.companion/battletime-companion.sock');
  });
});

describe('DaemonClient', () => {
  it('gives null when no app listens', async () => {
    expect(await DaemonClient.open(join(tmpdir(), 'no-such-battletime.sock'))).toBeNull();
  });

  it('gives null when the app never answers', async () => {
    const app = await startApp({}, true);
    expect(await DaemonClient.open(app.path, 50)).toBeNull();
  });

  it('passes a refusal on as an error', async () => {
    const app = await startApp({ daemon_state: () => ({}) });
    const client = (await DaemonClient.open(app.path))!;
    await expect(client.call('nope')).rejects.toThrow('Unknown command: nope');
    client.close();
  });
});

describe('a session through the app', () => {
  async function sessionOn(app: FakeApp): Promise<WatchSession> {
    const client = (await DaemonClient.open(app.path))!;
    return new WatchSession(false, (emit) => new DaemonLink(client, emit), true);
  }

  it('uses the link the app already has to the watch, and leaves it up on close', async () => {
    const app = await startApp({ daemon_state: connectedTo('AA:BB') });
    const session = await sessionOn(app);
    await session.connect('AA:BB');
    expect(session.bootloader).toBe(false);
    await session.close();
    expect(app.commands()).toEqual(['daemon_state', 'daemon_state']);
  });

  it('asks the app to connect to another watch, and learns its mode', async () => {
    let watch = 'AA:BB';
    let mode = 'application';
    const app = await startApp({
      daemon_state: () => connectedTo(watch, mode)(),
      daemon_connect: (args) => {
        watch = String(args.id);
        mode = 'bootloader';
      },
    });
    const session = await sessionOn(app);
    await session.connect('CC:DD', 'Spare');
    expect(session.bootloader).toBe(true);
    expect(app.requests.find((request) => request.command === 'daemon_connect')?.args).toEqual({
      id: 'CC:DD',
      name: 'Spare',
    });
    await session.close();
  });

  it('hears the watch through the app and writes and reads through it', async () => {
    const app = await startApp({
      daemon_state: connectedTo('AA:BB'),
      ble_write: () => null,
      ble_gatt_has: () => true,
      ble_gatt_read: () => [1, 2, 3],
      ble_gatt_write: () => null,
      ble_gatt_subscribe: () => null,
      ble_gatt_unsubscribe: () => null,
    });
    const session = await sessionOn(app);
    await session.connect('AA:BB');

    const lines: string[] = [];
    session.listen((line) => lines.push(line));
    app.push('daemon:state', {});
    app.push('ble:event', { name: 'line', payload: 'hello\n' });

    const notified: number[][] = [];
    const stop = await session.subscribe('s', 'c', (value) => notified.push(Array.from(value)));
    app.push('ble:event', { name: 'notify', payload: { key: 's/c', value: [9] } });

    await session.send('hi');
    expect(await session.hasCharacteristic('s', 'c')).toBe(true);
    expect(Array.from(await session.read('s', 'c'))).toEqual([1, 2, 3]);
    await session.write('s', 'c', Uint8Array.from([4]), 'command');
    expect(lines).toEqual(['hello']);
    expect(notified).toEqual([[9]]);
    stop();

    const write = app.requests.find((request) => request.command === 'ble_gatt_write');
    expect(write?.args).toEqual({ service: 's', characteristic: 'c', data: [4], mode: 'command' });
    await session.close();
  });

  it('takes a turn with the file service and holds the app back during an update', async () => {
    const app = await startApp({
      daemon_state: connectedTo('AA:BB'),
      files_begin: () => 5,
      files_end: () => null,
      daemon_hold: () => null,
    });
    const session = await sessionOn(app);
    expect(await session.exclusive!(async () => 'done')).toBe('done');
    expect(await session.holding(async () => 'flashed')).toBe('flashed');
    expect(app.requests.slice(1)).toEqual([
      { id: 2, command: 'files_begin', args: {} },
      { id: 3, command: 'files_end', args: { lease: 5 } },
      { id: 4, command: 'daemon_hold', args: { held: true } },
      { id: 5, command: 'daemon_hold', args: { held: false } },
    ]);
    await session.close();
  });

  it('a session on a link without turns, like BlueZ, takes none', () => {
    const plain = { hold: undefined } as unknown as ConstructorParameters<typeof WatchSession>[1];
    expect(new WatchSession(false, () => plain as never).exclusive).toBeUndefined();
  });
});
