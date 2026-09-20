import Constants from 'expo-constants';

import { Card } from '@/components/card';
import { Row, ToggleRow } from '@/components/row';
import { Screen } from '@/components/screen';
import { useWatch } from '@/hooks/use-watch';

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
        <Row label="Transport" detail={transportKind === 'ble' ? 'Bluetooth Low Energy' : 'Mock watch'} />
        <Row label="Firmware" detail="NeoTime, a wasp-os fork for the PineTime" />
      </Card>
    </Screen>
  );
}
