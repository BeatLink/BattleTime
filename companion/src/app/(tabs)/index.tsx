import { useRouter } from 'expo-router';
import { StyleSheet, View } from 'react-native';

import { Button } from '@/components/button';
import { Card } from '@/components/card';
import { Row } from '@/components/row';
import { Screen } from '@/components/screen';
import { ThemedText } from '@/components/themed-text';
import { Spacing } from '@/constants/theme';
import { useTheme } from '@/hooks/use-theme';
import { useWatch } from '@/hooks/use-watch';

const stateLabel = {
  disconnected: 'Not connected',
  connecting: 'Connecting',
  connected: 'Connected',
} as const;

export default function WatchScreen() {
  const router = useRouter();
  const theme = useTheme();
  const {
    connection,
    watchMode,
    watchReady,
    watch,
    settings,
    transportKind,
    lastError,
    findPhoneActive,
    connect,
    disconnect,
    send,
  } = useWatch();

  const connected = connection === 'connected';
  const inBootloader = connected && watchMode === 'bootloader';
  const dotColor =
    connection === 'connected' ? theme.success : connection === 'connecting' ? theme.warning : theme.textSecondary;
  const remembered =
    !watch && settings.lastWatchId ? { id: settings.lastWatchId, name: settings.lastWatchName ?? 'Watch', rssi: null } : null;

  return (
    <Screen title="Watch">
      <Card>
        <View style={styles.status}>
          <View style={[styles.dot, { backgroundColor: dotColor }]} />
          <View style={styles.statusText}>
            <ThemedText type="smallBold">{watch?.name ?? remembered?.name ?? 'No watch paired'}</ThemedText>
            <ThemedText type="small" themeColor="textSecondary">
              {inBootloader ? 'Connected, in bootloader' : stateLabel[connection]}
              {transportKind === 'mock' ? ' (mock watch)' : ''}
            </ThemedText>
          </View>
        </View>
        {lastError ? (
          <ThemedText type="small" style={{ color: theme.danger }}>
            {lastError}
          </ThemedText>
        ) : null}
        <View style={styles.actions}>
          {connected ? (
            <Button title="Disconnect" variant="secondary" onPress={() => disconnect()} />
          ) : (
            <>
              {remembered ? (
                <Button
                  title={`Reconnect to ${remembered.name}`}
                  busy={connection === 'connecting'}
                  onPress={() => connect(remembered)}
                />
              ) : null}
              <Button title="Find a watch" variant={remembered ? 'secondary' : 'primary'} onPress={() => router.push('/scan')} />
            </>
          )}
        </View>
      </Card>

      {inBootloader ? (
        <Card>
          <ThemedText type="smallBold" style={{ color: theme.warning }}>
            This watch is waiting in its bootloader.
          </ThemedText>
          <ThemedText type="small" themeColor="textSecondary">
            It is running no firmware, so nothing else here can reach it. Send it a firmware package
            from the Firmware tab and it will restart into it.
          </ThemedText>
          <Button title="Go to Firmware" onPress={() => router.push('/firmware')} />
        </Card>
      ) : null}

      {findPhoneActive ? (
        <Card>
          <ThemedText type="smallBold" style={{ color: theme.accent }}>
            The watch is looking for this phone.
          </ThemedText>
        </Card>
      ) : null}

      {watchReady ? (
        <Card title="Quick actions">
          <Row
            label="Find watch"
            detail="Vibrate the watch until it is tapped"
            onPress={() => send({ t: 'find', n: true })}
          />
          <Row
            label="Test notification"
            detail="Send a sample message to the watch"
            onPress={() =>
              send({
                t: 'notify',
                id: Date.now() % 100000,
                src: 'WaspOS Companion',
                title: 'Hello from your phone',
                body: 'Notifications are working.',
              })
            }
          />
          <Row
            label="Vibrate"
            detail="One short pulse"
            onPress={() => send({ t: 'vibrate', n: 1 })}
          />
        </Card>
      ) : null}
    </Screen>
  );
}

const styles = StyleSheet.create({
  status: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: Spacing.three,
  },
  statusText: {
    flex: 1,
  },
  dot: {
    width: 12,
    height: 12,
    borderRadius: 6,
  },
  actions: {
    gap: Spacing.two,
    marginTop: Spacing.two,
  },
});
