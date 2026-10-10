import { useCallback, useEffect, useState } from 'react';
import { AppState } from 'react-native';

import { Button } from '@/components/button';
import { Card } from '@/components/card';
import { Row, ToggleRow } from '@/components/row';
import { Screen } from '@/components/screen';
import { ThemedText } from '@/components/themed-text';
import { useWatch } from '@/hooks/use-watch';
import { phoneBridge } from '@/services/phone';

export default function NotificationsScreen() {
  const { settings, updateSettings } = useWatch();
  const [access, setAccess] = useState<boolean | null>(null);
  const [quiet, setQuiet] = useState(false);

  // The user grants access in the system settings, so check again whenever the app comes back.
  const check = useCallback(() => {
    if (!phoneBridge.available) {
      return;
    }
    void phoneBridge.hasNotificationAccess().catch(() => false).then(setAccess);
    void phoneBridge.isDoNotDisturb().catch(() => false).then(setQuiet);
  }, []);

  useEffect(() => {
    check();
    const subscription = AppState.addEventListener('change', (state) => {
      if (state === 'active') {
        check();
      }
    });
    return () => subscription.remove();
  }, [check]);

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
          label="Calls"
          detail="Show incoming calls, and answer or reject them from the watch"
          value={settings.forwardCalls}
          onValueChange={async (value) => {
            await updateSettings({ forwardCalls: value });
            if (value && phoneBridge.available) {
              await phoneBridge.requestCallPermissions().catch(() => false);
            }
          }}
        />
        <ToggleRow
          label="Now playing"
          detail="Send track info and accept media controls"
          value={settings.forwardMusic}
          onValueChange={(value) => updateSettings({ forwardMusic: value })}
        />
        <ToggleRow
          label="Weather"
          detail={
            settings.weatherPlace
              ? `Send conditions in ${settings.weatherPlace.name} every half hour`
              : 'Choose a place in Settings first'
          }
          value={settings.forwardWeather}
          onValueChange={(value) => updateSettings({ forwardWeather: value })}
        />
        <ToggleRow
          label="Respect Do Not Disturb"
          detail="Hold back notifications and calls while the phone is in Do Not Disturb"
          value={settings.respectDoNotDisturb}
          onValueChange={(value) => updateSettings({ respectDoNotDisturb: value })}
        />
      </Card>
      <Card title="Status">
        {phoneBridge.available ? (
          <>
            <Row
              label="Notification access"
              detail={
                access === null
                  ? 'Checking'
                  : access
                    ? 'Granted, so notifications and now playing can be read'
                    : 'Not granted, so nothing can be forwarded yet'
              }
            />
            {access === false ? (
              <Button title="Grant access" onPress={() => void phoneBridge.openNotificationAccessSettings()} />
            ) : null}
            <Row label="Do Not Disturb" detail={quiet ? 'On' : 'Off'} />
          </>
        ) : (
          <ThemedText type="small" themeColor="textSecondary">
            Reading notifications, calls and media needs the Android app. iOS offers no way for an app
            to read other apps&apos; notifications, and a browser or the desktop app has none to read.
          </ThemedText>
        )}
      </Card>
    </Screen>
  );
}
