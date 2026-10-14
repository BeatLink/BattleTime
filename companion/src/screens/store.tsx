import { useCallback, useEffect, useMemo, useState } from 'preact/hooks';

import { Button } from '@/components/button';
import { Card } from '@/components/card';
import { ErrorNote, TextRow } from '@/components/fields';
import { Row, ToggleRow } from '@/components/row';
import { Screen } from '@/components/screen';
import { ThemedText } from '@/components/themed-text';
import { useWatch } from '@/hooks/use-watch';
import type { WatchApp } from '@/protocol/packages';
import {
  AppStore,
  installEntry,
  installState,
  offerFromFile,
  sourceText,
  uninstallEntry,
  type InstallState,
  type StoreEntry,
  type StoreKind,
  type StoreOffer,
} from '@/services/app-store';
import { pickFiles } from '@/services/device-files';
import { fetchStoreRelease, listStoreReleases, type StoreRelease } from '@/services/github-store';
import { pluginBridge, Plugins, type PluginStatus } from '@/services/plugins';
import { appStorage } from '@/state/storage';
import { useTask } from '@/state/use-task';

const SECTIONS: { kind: StoreKind; title: string }[] = [
  { kind: 'app', title: 'Apps' },
  { kind: 'face', title: 'Watch faces' },
  { kind: 'plugin', title: 'Plugins' },
];

const STATE_TEXT: Record<InstallState, string> = {
  installed: 'Installed',
  'not-installed': 'Not installed',
  'plugin-elsewhere': 'Needs the app',
};

function kilobytes(bytes: number): string {
  return `${(bytes / 1024).toFixed(1)} kB`;
}

// The store: what the companion keeps ready to install, installing and uninstalling it on the watch,
// and adding more from files and GitHub.
export default function StoreScreen() {
  const { files, packages, watchReady, settings } = useWatch();
  const store = useMemo(() => new AppStore(appStorage), []);
  const plugins = useMemo(() => {
    const bridge = pluginBridge();
    return bridge ? new Plugins(bridge) : null;
  }, []);
  const task = useTask();
  const [entries, setEntries] = useState<StoreEntry[]>([]);
  const [hidden, setHidden] = useState(0);
  const [onWatch, setOnWatch] = useState<WatchApp[] | null>(null);
  const [pluginList, setPluginList] = useState<PluginStatus[] | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [armed, setArmed] = useState<'uninstall' | 'remove' | null>(null);
  const [progress, setProgress] = useState<string | null>(null);
  const [repo, setRepo] = useState(settings.githubRepo);
  const [releases, setReleases] = useState<StoreRelease[] | null>(null);
  const [offers, setOffers] = useState<StoreOffer[] | null>(null);
  const [chosen, setChosen] = useState<Set<string>>(new Set());
  const token = settings.githubToken || undefined;

  const readStore = useCallback(async () => {
    setEntries(await store.entries());
    setHidden(await store.hiddenCount());
  }, [store]);

  const readInstalled = useCallback(async () => {
    setOnWatch(watchReady ? (await (await packages()).listing()).apps : null);
    setPluginList(plugins ? await plugins.list() : null);
  }, [packages, plugins, watchReady]);

  useEffect(() => {
    void task.run('Reading the store', readStore);
    // Read the store once when the screen opens.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    void task.run('Listing what is installed', readInstalled);
    // List again whenever the watch connects or goes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [watchReady]);

  const chosenEntry = entries.find((entry) => entry.key === selected) ?? null;
  const stateOf = (entry: StoreEntry): InstallState | null =>
    entry.kind !== 'plugin' && onWatch === null ? null : installState(entry, onWatch ?? [], pluginList);

  const act = (label: string, action: () => Promise<void>) =>
    task.run(label, async () => {
      try {
        await action();
      } finally {
        setArmed(null);
        await readStore();
        await readInstalled();
      }
    });

  const install = (entry: StoreEntry) =>
    act(`Installing ${entry.name}`, async () => {
      await installEntry(entry, await store.data(entry.key), { files: await files(), packages: await packages(), plugins }, (step) =>
        setProgress(`${step.file}: ${Math.round((step.sent / Math.max(1, step.total)) * 100)}%`),
      );
      setProgress(`Installed ${entry.name}.`);
    });

  const uninstall = (entry: StoreEntry) =>
    act(`Uninstalling ${entry.name}`, async () => {
      await uninstallEntry(entry, { files: await files(), packages: await packages(), plugins });
      setProgress(`Uninstalled ${entry.name}.`);
    });

  const addFiles = () =>
    act('Adding', async () => {
      const picked = await pickFiles('.ntapp,.ntplugin,.zip');
      if (picked.length === 0) return;
      const added: StoreOffer[] = picked.map((file) => offerFromFile(file.data, { type: 'file', name: file.name }));
      await store.add(added);
      setProgress(`Added ${added.map((offer) => offer.entry.name).join(', ')}.`);
    });

  const lookOnGitHub = () =>
    task.run('Looking on GitHub', async () => {
      setOffers(null);
      setReleases(await listStoreReleases(repo, { token }));
    });

  const openRelease = (release: StoreRelease) =>
    task.run(`Downloading ${release.title}`, async () => {
      const found = await fetchStoreRelease(repo, release, { token });
      setOffers(found);
      setChosen(new Set(found.map((offer) => offer.entry.key)));
    });

  const addChosen = () =>
    act('Adding', async () => {
      const picked = (offers ?? []).filter((offer) => chosen.has(offer.entry.key));
      await store.add(picked);
      setOffers(null);
      setReleases(null);
      setProgress(`Added ${picked.length} to the store.`);
    });

  const busy = task.busy !== null;

  return (
    <Screen title="Store">
      <Card>
        <ThemedText type="small" themeColor="textSecondary">
          The store holds the apps, watch faces and plugins this companion can install. It starts with every
          app NeoTime ships, and you can add more from files or from a GitHub repository&apos;s releases, or
          take out what you do not want. Plugins run in the app, which also installs the watch apps they
          carry.
        </ThemedText>
        {!watchReady ? (
          <ThemedText type="small" themeColor="textSecondary">
            Connect a watch to install apps and faces.
          </ThemedText>
        ) : null}
      </Card>

      {chosenEntry ? (
        <Card title={chosenEntry.name}>
          {chosenEntry.description ? <ThemedText type="small">{chosenEntry.description}</ThemedText> : null}
          <ThemedText type="small" themeColor="textSecondary" class="pre-wrap">
            {[chosenEntry.id, chosenEntry.version, kilobytes(chosenEntry.bytes), sourceText(chosenEntry.source)]
              .filter(Boolean)
              .join(' · ')}
            {chosenEntry.watch.length > 0 ? `\nCarries the watch app ${chosenEntry.watch.join(', ')}.` : ''}
          </ThemedText>
          <Button
            title={stateOf(chosenEntry) === 'installed' ? 'Install again' : 'Install'}
            disabled={busy || !watchReady || stateOf(chosenEntry) === 'plugin-elsewhere'}
            onClick={() => void install(chosenEntry)}
          />
          {stateOf(chosenEntry) === 'installed' ? (
            <Button
              title={armed === 'uninstall' ? 'Tap again to uninstall it and what it saved' : 'Uninstall'}
              variant="danger"
              disabled={busy || !watchReady}
              onClick={() => (armed === 'uninstall' ? void uninstall(chosenEntry) : setArmed('uninstall'))}
            />
          ) : null}
          <Button
            title={armed === 'remove' ? 'Tap again to take it out of the store' : 'Remove from the store'}
            variant="secondary"
            disabled={busy}
            onClick={() => {
              if (armed !== 'remove') {
                setArmed('remove');
                return;
              }
              void act(`Removing ${chosenEntry.name}`, async () => {
                await store.remove(chosenEntry.key);
                setSelected(null);
              });
            }}
          />
        </Card>
      ) : null}

      {SECTIONS.map(({ kind, title }) => {
        const shown = entries.filter((entry) => entry.kind === kind);
        if (shown.length === 0) return null;
        return (
          <Card key={kind} title={title}>
            {shown.map((entry) => {
              const state = stateOf(entry);
              return (
                <Row
                  key={entry.key}
                  label={entry.name}
                  detail={[state ? STATE_TEXT[state] : null, kilobytes(entry.bytes), sourceText(entry.source)]
                    .filter(Boolean)
                    .join(' · ')}
                  onClick={() => {
                    setSelected(entry.key === selected ? null : entry.key);
                    setArmed(null);
                  }}
                />
              );
            })}
          </Card>
        );
      })}

      <Card title="Add to the store">
        <Button title="Add from files" variant="secondary" disabled={busy} onClick={() => void addFiles()} />
        <TextRow
          label="GitHub repository"
          detail="Releases with neotime-store.zip, .ntapp or .ntplugin files; with a token, Actions builds too."
          value={repo}
          onChangeText={setRepo}
          placeholder="owner/name"
        />
        <Button title="Look on GitHub" variant="secondary" disabled={busy} onClick={() => void lookOnGitHub()} />
        {releases?.length === 0 ? (
          <ThemedText type="small" themeColor="textSecondary">
            No release or build there carries apps or plugins.
          </ThemedText>
        ) : null}
        {offers === null
          ? releases?.map((release) => (
              <Row
                key={release.id}
                label={release.title}
                detail={`${release.source === 'actions' ? 'Actions build' : 'Release'} · ${release.date.slice(0, 10)}`}
                onClick={() => void openRelease(release)}
              />
            ))
          : null}
        {offers?.map((offer) => (
          <ToggleRow
            key={offer.entry.key}
            label={offer.entry.name}
            detail={`${offer.entry.kind} · ${kilobytes(offer.entry.bytes)}`}
            value={chosen.has(offer.entry.key)}
            disabled={busy}
            onValueChange={(on) => {
              const next = new Set(chosen);
              if (on) next.add(offer.entry.key);
              else next.delete(offer.entry.key);
              setChosen(next);
            }}
          />
        ))}
        {offers ? (
          <Button
            title={`Add ${chosen.size} to the store`}
            disabled={busy || chosen.size === 0}
            onClick={() => void addChosen()}
          />
        ) : null}
        {hidden > 0 ? (
          <Button
            title={`Bring back the ${hidden} bundled ${hidden === 1 ? 'entry' : 'entries'} you removed`}
            variant="secondary"
            disabled={busy}
            onClick={() => void act('Restoring', () => store.restoreBundled())}
          />
        ) : null}
      </Card>

      {(progress ?? task.busy) ? (
        <ThemedText type="small" themeColor="textSecondary">
          {task.busy ? `${task.busy}... ${progress ?? ''}` : progress}
        </ThemedText>
      ) : null}
      <ErrorNote error={task.error} onDismiss={task.clearError} />
    </Screen>
  );
}
