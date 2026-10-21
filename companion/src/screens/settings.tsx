import { useState } from 'preact/hooks';

import { BatteryLimitsRow } from '@/components/battery-limits-row';
import { Button } from '@/components/button';
import { Card } from '@/components/card';
import { ChoiceRow, ErrorNote, SavedTextRow, TextRow } from '@/components/fields';
import { Row, ToggleRow } from '@/components/row';
import { Screen } from '@/components/screen';
import { ThemedText } from '@/components/themed-text';
import { useWatch } from '@/hooks/use-watch';
import { useRouter } from '@/navigation';
import { MAX_RESPONSE, parseHostList } from '@/services/http-proxy';
import { phoneBridge } from '@/services/phone';
import { deviceWord } from '@/services/phone/words';
import { SYNC_INTERVAL_CHOICES, syncBridge } from '@/services/sync';
import { findPlace } from '@/services/weather';
import { useTask } from '@/state/use-task';

const TRANSPORT_LABELS = {
  tauri: 'Bluetooth, through the app',
  mock: 'Mock watch, in a plain browser',
} as const;

function transportLabel(kind: keyof typeof TRANSPORT_LABELS): string {
  return TRANSPORT_LABELS[kind];
}

export default function SettingsScreen() {
  const { settingsLoaded } = useWatch();
  // The text boxes start from the saved settings, so they wait until those are read.
  return settingsLoaded ? <SettingsForm /> : <Screen title="Settings" />;
}

// The desktop app's own way to quit, since closing its window leaves it running.
function quitCompanion(): Promise<unknown> {
  const desktop = (globalThis as { battletime?: { desktop?: { quit?: () => Promise<unknown> } } }).battletime?.desktop;
  return desktop?.quit?.() ?? Promise.resolve();
}

function SettingsForm() {
  const { settings, updateSettings, transportKind } = useWatch();
  const router = useRouter();
  const task = useTask();
  const [place, setPlace] = useState(settings.weatherQuery);
  const [located, setLocated] = useState<string | null>(null);
  const hosts = parseHostList(settings.httpAllowedHosts);

  return (
    <Screen title="Settings">
      <Card title="Connection">
        <ToggleRow
          label="Reconnect automatically"
          detail="Bring the last watch back whenever it drops or comes into range"
          value={settings.autoReconnect}
          onValueChange={(value) => updateSettings({ autoReconnect: value })}
        />
        {phoneBridge.kind === 'computer' ? (
          <>
            <ToggleRow
              label="Keep running in the background"
              detail="Stay connected to the watch after the window closes"
              value={settings.runInBackground}
              onValueChange={(value) => updateSettings({ runInBackground: value })}
            />
            <ToggleRow
              label="Start with the computer"
              detail="Connect to the watch at login, without opening the window"
              value={settings.startWithComputer}
              onValueChange={(value) => updateSettings({ startWithComputer: value })}
            />
            <Row label="Quit" detail="Close the window and stop keeping the watch" onClick={() => void quitCompanion()} />
          </>
        ) : null}
        <Row
          label="Forget watch"
          detail={settings.lastWatchName ?? 'No watch remembered'}
          onClick={settings.lastWatchId ? () => updateSettings({ lastWatchId: null, lastWatchName: null }) : undefined}
        />
        {phoneBridge.kind === 'phone' ? <BatteryLimitsRow bridge={phoneBridge} /> : null}
      </Card>

      <Card title="Safety">
        <Row label="SOS" detail="Whom the watch's SOS calls and texts, and whether the phone records" onClick={() => router.push('/sos')} />
      </Card>

      <Card title="Sync">
        <ChoiceRow<number>
          label="Sync every"
          detail={
            syncBridge()
              ? 'How often every setting and record on the watch is read again while it is connected'
              : 'How often the battery and step count are read again while the watch is connected'
          }
          options={SYNC_INTERVAL_CHOICES}
          value={settings.syncIntervalSeconds}
          onChange={(value) => updateSettings({ syncIntervalSeconds: value })}
        />
        {syncBridge() ? (
          <Button
            title="Sync now"
            busy={task.busy !== null}
            onClick={() => void task.run('Asking for a sync', async () => syncBridge()?.now())}
          />
        ) : null}
      </Card>

      <Card title="Weather">
        <ThemedText type="small" themeColor="textSecondary">
          Current conditions come from Open-Meteo every half hour while the watch is connected, when
          Weather is on in Notifications.
        </ThemedText>
        <TextRow
          label="Place"
          detail={settings.weatherPlace ? `Reporting for ${settings.weatherPlace.name}` : 'No place chosen'}
          placeholder="A town or city"
          value={place}
          onChangeText={setPlace}
          autoCapitalize="words"
          autoComplete="address-level2"
        />
        <Button
          title="Find this place"
          variant="secondary"
          disabled={place.trim() === ''}
          busy={task.busy !== null}
          onClick={() =>
            void task.run('Looking up the place', async () => {
              const found = await findPlace(place);
              await updateSettings({ weatherQuery: place, weatherPlace: found });
            })
          }
        />
        {phoneBridge.supportsLocation ? (
          <ToggleRow
            label={`Use where this ${deviceWord(phoneBridge.kind)} is`}
            detail={located ?? 'Asks the location service each time, instead of using the place above'}
            value={settings.weatherFromDevice}
            onValueChange={(value) => {
              void updateSettings({ weatherFromDevice: value });
              if (value) {
                void task.run('Finding where this is', async () => {
                  const here = await phoneBridge.locate();
                  setLocated(`Found ${here.name}, ${here.latitude.toFixed(2)}, ${here.longitude.toFixed(2)}`);
                });
              }
            }}
          />
        ) : null}
        <ErrorNote error={task.error} onDismiss={task.clearError} />
      </Card>

      <Card title="Internet for watch apps">
        <ToggleRow
          label="Let watch apps fetch pages"
          detail={`Only from the hosts below, and only the first ${MAX_RESPONSE} characters`}
          value={settings.httpEnabled}
          onValueChange={(value) => updateSettings({ httpEnabled: value })}
        />
        <SavedTextRow
          label="Allowed hosts"
          detail={hosts.length ? `${hosts.length} allowed, with their subdomains` : 'None, so every request is refused'}
          placeholder="api.example.com, other.org"
          value={settings.httpAllowedHosts}
          onSave={(text) => updateSettings({ httpAllowedHosts: text })}
        />
      </Card>

      <Card title="Firmware downloads">
        <SavedTextRow
          label="Repository"
          detail="Where the Firmware tab lists builds from"
          value={settings.githubRepo}
          onSave={(text) => updateSettings({ githubRepo: text.trim() })}
        />
        <SavedTextRow
          label="Board"
          detail="Builds are named after it, for example pinetime, p8 or k9"
          value={settings.firmwareBoard}
          onSave={(text) => updateSettings({ firmwareBoard: text.trim().toLowerCase() })}
        />
        <SavedTextRow
          label="GitHub token"
          detail="Needed for Actions builds; a token with no scopes is enough. Kept on this device."
          value={settings.githubToken}
          password
          onSave={(text) => updateSettings({ githubToken: text.trim() })}
        />
      </Card>

      <Card title="About">
        <Row label="Version" detail={__APP_VERSION__} />
        <Row label="Transport" detail={transportLabel(transportKind)} />
        <Row label="Firmware" detail="BattleTime on the PineTime" />
      </Card>
    </Screen>
  );
}
