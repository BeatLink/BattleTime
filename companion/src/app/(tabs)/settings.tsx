import Constants from 'expo-constants';

import { Card } from '@/components/card';
import { Row, ToggleRow } from '@/components/row';
import { Screen } from '@/components/screen';
import { useWatch } from '@/hooks/use-watch';

const TRANSPORT_LABELS = {
  ble: 'Bluetooth, on this phone',
  electron: 'Bluetooth, through the desktop app',
  mock: 'Mock watch',
} as const;

function transportLabel(kind: keyof typeof TRANSPORT_LABELS): string {
  return TRANSPORT_LABELS[kind];
}

export default function SettingsScreen() {
  const { settings, updateSettings, transportKind } = useWatch();

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
      <Card title="About">
        <Row label="Version" detail={Constants.expoConfig?.version ?? 'dev'} />
        <Row label="Transport" detail={transportLabel(transportKind)} />
        <Row label="Firmware" detail="wasp-os on the PineTime" />
      </Card>
    </Screen>
  );
}
