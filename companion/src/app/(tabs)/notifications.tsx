import { Card } from '@/components/card';
import { ToggleRow } from '@/components/row';
import { Screen } from '@/components/screen';
import { ThemedText } from '@/components/themed-text';
import { useWatch } from '@/hooks/use-watch';

export default function NotificationsScreen() {
  const { settings, updateSettings } = useWatch();

  return (
    <Screen title="Notifications">
      <Card title="Forwarding">
        <ToggleRow
          label="Forward notifications"
          detail="Mirror phone notifications to the watch"
          value={settings.forwardNotifications}
          onValueChange={(value) => updateSettings({ forwardNotifications: value })}
        />
        <ToggleRow
          label="Now playing"
          detail="Send track info and accept media controls"
          value={settings.forwardMusic}
          onValueChange={(value) => updateSettings({ forwardMusic: value })}
        />
        <ToggleRow
          label="Weather"
          detail="Push a current conditions report"
          value={settings.forwardWeather}
          onValueChange={(value) => updateSettings({ forwardWeather: value })}
        />
      </Card>
      <Card title="Status">
        <ThemedText type="small" themeColor="textSecondary">
          Reading phone notifications needs a native listener that is not part of this scaffold yet. See
          src/services/notifications/README.md for the plan.
        </ThemedText>
      </Card>
    </Screen>
  );
}
