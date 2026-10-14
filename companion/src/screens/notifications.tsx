import { useCallback, useEffect, useState } from 'preact/hooks';

import { Button } from '@/components/button';
import { Card } from '@/components/card';
import { Row, ToggleRow } from '@/components/row';
import { Screen } from '@/components/screen';
import { ThemedText } from '@/components/themed-text';
import { useWatch } from '@/hooks/use-watch';
import { phoneBridge } from '@/services/phone';
import { deviceWord } from '@/services/phone/words';
import { describeWeatherSource } from '@/services/weather';

export default function NotificationsScreen() {
  const { settings, updateSettings } = useWatch();
  const [access, setAccess] = useState<boolean | null>(null);
  const [quiet, setQuiet] = useState(false);
  const device = deviceWord(phoneBridge.kind);
  const weatherSource = describeWeatherSource(
    settings.weatherPlace,
    settings.weatherFromDevice && phoneBridge.supportsLocation,
    device,
  );

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
    const onVisible = () => {
      if (document.visibilityState === 'visible') {
        check();
      }
    };
    document.addEventListener('visibilitychange', onVisible);
    return () => document.removeEventListener('visibilitychange', onVisible);
  }, [check]);

  return (
    <Screen title="Notifications">
      <Card title="Forwarding">
        <ToggleRow
          label="Forward notifications"
          detail={`Mirror ${device} notifications to the watch`}
          value={settings.forwardNotifications}
          onValueChange={(value) => updateSettings({ forwardNotifications: value })}
        />
        {phoneBridge.supportsCalls || !phoneBridge.available ? (
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
        ) : null}
        <ToggleRow
          label="Now playing"
          detail="Send track info and accept media controls"
          value={settings.forwardMusic}
          onValueChange={(value) => updateSettings({ forwardMusic: value })}
        />
        <ToggleRow
          label="Weather"
          detail={
            weatherSource
              ? `Send conditions for ${weatherSource} every half hour`
              : 'Choose a place in Settings first'
          }
          value={settings.forwardWeather}
          onValueChange={(value) => updateSettings({ forwardWeather: value })}
        />
        <ToggleRow
          label="Respect Do Not Disturb"
          detail={`Hold back ${phoneBridge.supportsCalls ? 'notifications and calls' : 'notifications'} while the ${device} is in Do Not Disturb`}
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
                    : phoneBridge.kind === 'computer'
                      ? 'The desktop would not let notifications be read; media players still work'
                      : 'Not granted, so nothing can be forwarded yet'
              }
            />
            {access === false && phoneBridge.kind === 'phone' ? (
              <Button title="Grant access" onClick={() => void phoneBridge.openNotificationAccessSettings()} />
            ) : null}
            <Row label="Do Not Disturb" detail={quiet ? 'On' : 'Off'} />
          </>
        ) : (
          <ThemedText type="small" themeColor="textSecondary">
            Reading notifications, calls and media needs the app, on Android or the desktop; a plain
            browser has none to read.
          </ThemedText>
        )}
      </Card>
    </Screen>
  );
}
