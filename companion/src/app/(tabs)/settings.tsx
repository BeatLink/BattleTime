import Constants from 'expo-constants';
import { useState } from 'react';

import { Button } from '@/components/button';
import { Card } from '@/components/card';
import { ErrorNote, SavedTextRow, TextRow } from '@/components/fields';
import { Row, ToggleRow } from '@/components/row';
import { Screen } from '@/components/screen';
import { ThemedText } from '@/components/themed-text';
import { useWatch } from '@/hooks/use-watch';
import { MAX_RESPONSE, parseHostList } from '@/services/http-proxy';
import { findPlace } from '@/services/weather';
import { useTask } from '@/state/use-task';

const TRANSPORT_LABELS = {
  ble: 'Bluetooth, on this phone',
  electron: 'Bluetooth, through the desktop app',
  mock: 'Mock watch',
} as const;

function transportLabel(kind: keyof typeof TRANSPORT_LABELS): string {
  return TRANSPORT_LABELS[kind];
}

export default function SettingsScreen() {
  const { settingsLoaded } = useWatch();
  // The text boxes start from the saved settings, so they wait until those are read.
  return settingsLoaded ? <SettingsForm /> : <Screen title="Settings" />;
}

function SettingsForm() {
  const { settings, updateSettings, transportKind } = useWatch();
  const task = useTask();
  const [place, setPlace] = useState(settings.weatherQuery);
  const hosts = parseHostList(settings.httpAllowedHosts);

  return (
    <Screen title="Settings">
      <Card title="Connection">
        <ToggleRow
          label="Reconnect automatically"
          detail="Reconnect to the last watch when the app opens"
          value={settings.autoReconnect}
          onValueChange={(value) => updateSettings({ autoReconnect: value })}
        />
        <Row
          label="Forget watch"
          detail={settings.lastWatchName ?? 'No watch remembered'}
          onPress={settings.lastWatchId ? () => updateSettings({ lastWatchId: null, lastWatchName: null }) : undefined}
        />
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
        />
        <Button
          title="Find this place"
          variant="secondary"
          disabled={place.trim() === ''}
          busy={task.busy !== null}
          onPress={() =>
            void task.run('Looking up the place', async () => {
              const found = await findPlace(place);
              await updateSettings({ weatherQuery: place, weatherPlace: found });
            })
          }
        />
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
          secureTextEntry
          onSave={(text) => updateSettings({ githubToken: text.trim() })}
        />
      </Card>

      <Card title="About">
        <Row label="Version" detail={Constants.expoConfig?.version ?? 'dev'} />
        <Row label="Transport" detail={transportLabel(transportKind)} />
        <Row label="Firmware" detail="wasp-os on the PineTime" />
      </Card>
    </Screen>
  );
}
