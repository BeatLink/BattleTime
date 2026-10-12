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
import type { DiagnosticCounter } from '@/protocol/neotime';
import { needsSetup } from '@/protocol/setup';
import { phoneBridge } from '@/services/phone';
import { useTask } from '@/state/use-task';

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
    capabilities,
    batteryLevel,
    readDiagnostics,
    packages,
  } = useWatch();
  // Whether the watch has no face yet, read once each time one with the packages characteristic connects.
  const [setupNeeded, setSetupNeeded] = useState(false);
  const canInstall = watchReady && capabilities.packages && capabilities.files;
  useEffect(() => {
    if (!canInstall) {
      return;
    }
    let cancelled = false;
    void packages()
      .then((client) => client.apps())
      .then((apps) => !cancelled && setSetupNeeded(needsSetup(apps)))
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [canInstall, packages]);
  const diagnostics = useTask();
  const [counters, setCounters] = useState<DiagnosticCounter[] | null>(null);
  // A clock for the "last synced" line, moved on twice a minute.
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 30000);
    return () => clearInterval(timer);
  }, []);

  const connected = connection === 'connected';
  const inBootloader = connected && watchMode === 'bootloader';
  // The features that ride the UART message channel, which early NeoTime builds do not serve yet.
  const uartReady = watchReady && capabilities.uart;
  const showStatus = uartReady || (watchReady && capabilities.battery);
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
            from the Firmware tab, such as the NeoTime reloader, and it will restart into it.
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

      {watchReady && !capabilities.uart ? (
        <Card>
          <ThemedText type="small" themeColor="textSecondary">
            This firmware serves updates, the clock and diagnostics. Notifications, steps, alarms and
            watch settings appear here once it serves the UART message channel.
          </ThemedText>
        </Card>
      ) : null}

      {showStatus ? (
        <Card title="Status">
          {uartReady ? (
            <>
              <Row
                label={status ? `Battery ${status.bat}%${status.charging ? ', charging' : ''}` : 'Battery'}
                detail={status ? `${(status.mv / 1000).toFixed(2)} V` : 'Not read yet'}
              />
              <Row
                label={status?.steps != null ? `${status.steps.toLocaleString()} steps today` : 'Steps today'}
                detail={status && status.steps == null ? 'This watch has no step counter' : 'As the step counter has it now'}
                onPress={() => router.push('/steps')}
              />
            </>
          ) : (
            <Row label={batteryLevel != null ? `Battery ${batteryLevel}%` : 'Battery'} detail={batteryLevel != null ? 'From the Battery service' : 'Not read yet'} />
          )}
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

      {watchReady && capabilities.diagnostics ? (
        <Card title="Diagnostics">
          {counters?.map((counter) => (
            <Row key={counter.key} label={counter.label} detail={counter.value.toLocaleString()} />
          ))}
          {diagnostics.error ? (
            <ThemedText type="small" style={{ color: theme.danger }}>
              {diagnostics.error}
            </ThemedText>
          ) : null}
          <Button
            title={counters ? 'Read again' : 'Read the counters'}
            variant="secondary"
            disabled={diagnostics.busy !== null}
            onPress={() =>
              void diagnostics.run('Reading', readDiagnostics).then((read) => read && setCounters(read))
            }
          />
        </Card>
      ) : null}

      {canInstall && setupNeeded ? (
        <Card>
          <ThemedText type="smallBold" style={{ color: theme.accent }}>
            This watch has no face yet.
          </ThemedText>
          <ThemedText type="small" themeColor="textSecondary">
            NeoTime starts with no apps. Install the shared icons, a watch face, the launcher and the
            apps from this phone.
          </ThemedText>
          <Button title="Set up the watch" onPress={() => router.push('/setup')} />
        </Card>
      ) : null}

      {uartReady || (watchReady && capabilities.files) ? (
        <Card title="On the watch">
          {uartReady ? (
            <>
              <Row label="Steps" detail="The last week, day by day and hour by hour" onPress={() => router.push('/steps')} />
              <Row label="Alarms" detail="Set times and the days they repeat on" onPress={() => router.push('/alarms')} />
              <Row
                label="Watch settings"
                detail="Theme, brightness, timeout, chimes, units and step goal"
                onPress={() => router.push('/watch-settings')}
              />
            </>
          ) : null}
          {canInstall ? (
            <Row label="Apps" detail="Install, hide or remove apps and faces" onPress={() => router.push('/apps')} />
          ) : null}
          {capabilities.files ? (
            <>
              <Row label="Files" detail="Browse, send and delete files, and check free space" onPress={() => router.push('/files')} />
              <Row label="Backup" detail="Save the watch's settings and app state, and put them back" onPress={() => router.push('/backup')} />
            </>
          ) : null}
        </Card>
      ) : null}

      {uartReady ? (
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
                src: 'NeoTime Companion',
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
