import { useRouter } from 'expo-router';
import { useEffect, useState } from 'react';
import { StyleSheet, View } from 'react-native';

import { Button } from '@/components/button';
import { Card } from '@/components/card';
import { Row } from '@/components/row';
import { Screen } from '@/components/screen';
import { ThemedText } from '@/components/themed-text';
import { Spacing } from '@/constants/theme';
import { useTheme } from '@/hooks/use-theme';
import { useWatch } from '@/hooks/use-watch';
import { phoneBridge } from '@/services/phone';

// How long ago a moment was, in the coarse words a status line needs.
function ago(at: number, now: number): string {
  const minutes = Math.floor((now - at) / 60000);
  if (minutes < 1) return 'just now';
  if (minutes < 60) return `${minutes} min ago`;
  const hours = Math.floor(minutes / 60);
  return hours < 24 ? `${hours} h ago` : new Date(at).toLocaleString();
}

function duration(seconds: number): string {
  return `${Math.floor(seconds / 60)}:${String(Math.floor(seconds % 60)).padStart(2, '0')}`;
}

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
    sendWeather,
    status,
    lastSynced,
    refreshStatus,
    nowPlaying,
  } = useWatch();
  // A clock for the "last synced" line, moved on twice a minute.
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 30000);
    return () => clearInterval(timer);
  }, []);

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
        <Card title="Status">
          <Row
            label={status ? `Battery ${status.battery}%${status.charging ? ', charging' : ''}` : 'Battery'}
            detail={status ? `${(status.mv / 1000).toFixed(2)} V` : 'Not read yet'}
          />
          <Row
            label={status?.steps != null ? `${status.steps.toLocaleString()} steps today` : 'Steps today'}
            detail={status && status.steps == null ? 'This watch has no step counter' : 'As the step counter has it now'}
            onPress={() => router.push('/steps')}
          />
          <Row
            label="Last synced"
            detail={lastSynced ? ago(lastSynced, now) : 'Not yet'}
            onPress={() => void refreshStatus().catch(() => {})}
            right={
              <ThemedText type="smallBold" style={{ color: theme.accent }}>
                Sync now
              </ThemedText>
            }
          />
        </Card>
      ) : null}

      {nowPlaying && (nowPlaying.track || nowPlaying.artist) ? (
        <Card title="Now playing">
          <Row
            label={nowPlaying.track ?? 'Unknown track'}
            detail={[nowPlaying.artist, nowPlaying.album, nowPlaying.durationSec ? duration(nowPlaying.durationSec) : null]
              .filter(Boolean)
              .join(' · ')}
          />
          {phoneBridge.available ? (
            <View style={styles.media}>
              <Button title="Previous" variant="secondary" style={styles.mediaButton} onPress={() => void phoneBridge.mediaCommand('previous')} />
              <Button
                title={nowPlaying.playing ? 'Pause' : 'Play'}
                style={styles.mediaButton}
                onPress={() => void phoneBridge.mediaCommand(nowPlaying.playing ? 'pause' : 'play')}
              />
              <Button title="Next" variant="secondary" style={styles.mediaButton} onPress={() => void phoneBridge.mediaCommand('next')} />
            </View>
          ) : null}
        </Card>
      ) : null}

      {watchReady ? (
        <Card title="On the watch">
          <Row label="Steps" detail="The last week, day by day and hour by hour" onPress={() => router.push('/steps')} />
          <Row label="Alarms" detail="Set times and the days they repeat on" onPress={() => router.push('/alarms')} />
          <Row
            label="Watch settings"
            detail="Brightness, face, timeout, units, step goal and theme"
            onPress={() => router.push('/watch-settings')}
          />
          <Row label="Files" detail="Browse, send and delete files, and check memory" onPress={() => router.push('/files')} />
          <Row label="Backup" detail="Save the watch's settings and alarms, and put them back" onPress={() => router.push('/backup')} />
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
          {settings.weatherPlace ? (
            <Row
              label="Send weather"
              detail={`Current conditions in ${settings.weatherPlace.name}`}
              onPress={() => void sendWeather().catch(() => {})}
            />
          ) : null}
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
  media: {
    flexDirection: 'row',
    gap: Spacing.two,
  },
  mediaButton: {
    flex: 1,
    paddingHorizontal: Spacing.two,
  },
});
