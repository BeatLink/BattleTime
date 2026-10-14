// The phone side every app's plugin shares: it hands the page the app's synced state, and the page's changes to the watch.
// It runs in the companion's QuickJS (docs/plugins.md). The build script fills in __PLUGIN_ID__ and __PLUGIN_VIEWS__.

declare const neotime: {
  on(event: 'ui' | 'sync' | 'connect', handler: (data: any) => void): void;
  ui: { send(message: unknown): void };
  data: {
    get(source: string): any;
    list(prefix: string): string[];
    write(changes: object): Promise<unknown>;
    writeSettings(changes: object): Promise<unknown>;
    syncNow(): void;
  };
};
declare const __PLUGIN_ID__: string;
declare const __PLUGIN_VIEWS__: string[];

// How many days of steps, heart rate or battery the page is sent.
const STEP_DAYS_SHOWN = 14;

// The newest days of a log under `logs/<folder>/<year>/<MM>-<DD>`, with their slots.
function logDays(folder: string) {
  return neotime.data
    .list(`logs/${folder}`)
    .filter((source) => source.split('/').length === 4)
    .sort()
    .slice(-STEP_DAYS_SHOWN)
    .map((source) => ({ source, slots: neotime.data.get(source)?.slots ?? [] }));
}

// What the page shows, read afresh from the companion's copy.
function snapshot() {
  const shown: Record<string, unknown> = { app: neotime.data.get(`apps/${__PLUGIN_ID__}`) ?? null };
  if (__PLUGIN_VIEWS__.includes('settings')) {
    shown.settings = neotime.data.get('settings') ?? null;
  }
  if (__PLUGIN_VIEWS__.includes('steps')) {
    shown.steps = neotime.data
      .list('steps')
      .filter((source) => source.split('/').length === 3)
      .sort()
      .slice(-STEP_DAYS_SHOWN)
      .map((source) => ({ source, slots: neotime.data.get(source)?.slots ?? [] }));
  }
  for (const log of ['heart', 'battery']) {
    if (__PLUGIN_VIEWS__.includes(log)) {
      shown[log] = logDays(log);
    }
  }
  return shown;
}

function report() {
  neotime.ui.send({ snapshot: snapshot() });
}

// Sends the outcome of a write to the page, then the state as it now is.
async function settle(write: Promise<unknown>) {
  try {
    await write;
    neotime.ui.send({ error: null });
  } catch (error) {
    neotime.ui.send({ error: (error as Error).message });
  }
  report();
}

neotime.on('ui', (message: { write?: object; writeSettings?: object; syncNow?: boolean }) => {
  if (message.write) {
    return settle(neotime.data.write(message.write));
  }
  if (message.writeSettings) {
    return settle(neotime.data.writeSettings(message.writeSettings));
  }
  if (message.syncNow) {
    neotime.data.syncNow();
  }
  report();
});

neotime.on('sync', report);
neotime.on('connect', report);
