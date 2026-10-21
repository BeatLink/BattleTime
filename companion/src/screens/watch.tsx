import { useEffect, useState } from 'preact/hooks';

import { Button } from '@/components/button';
import { Card } from '@/components/card';
import { DebugReportCard } from '@/components/debug-report-card';
import { Row } from '@/components/row';
import { Screen } from '@/components/screen';
import { ThemedText } from '@/components/themed-text';
import { theme } from '@/constants/theme';
import { useWatch } from '@/hooks/use-watch';
import { useRouter } from '@/navigation';
import type { DiagnosticCounter } from '@/protocol/battletime';
import { needsSetup } from '@/protocol/setup';
import { phoneBridge } from '@/services/phone';
import { deviceWord } from '@/services/phone/words';
import { describeWeatherSource } from '@/services/weather';
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

// The three media buttons share the row evenly.
const MEDIA_BUTTON = { flex: 1, paddingLeft: '8px', paddingRight: '8px' };

const stateLabel = {
  disconnected: 'Not connected',
  connecting: 'Connecting',
  connected: 'Connected',
} as const;

export default function WatchScreen() {
  const router = useRouter();
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
  const weatherSource = describeWeatherSource(
    settings.weatherPlace,
    settings.weatherFromDevice && phoneBridge.supportsLocation,
    deviceWord(phoneBridge.kind),
  );
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
  // The features that ride the UART message channel, which early BattleTime builds do not serve yet.
  const uartReady = watchReady && capabilities.uart;
  const showStatus = uartReady || (watchReady && capabilities.battery);
  const dotColor =
    connection === 'connected' ? theme.success : connection === 'connecting' ? theme.warning : theme.textSecondary;
  const remembered =
    !watch && settings.lastWatchId ? { id: settings.lastWatchId, name: settings.lastWatchName ?? 'Watch', rssi: null } : null;

  return (
    <Screen title="Watch">
      <Card>
        <div class="line">
          <span class="status-dot" style={{ backgroundColor: dotColor }} />
          <div class="grow">
            <ThemedText type="smallBold">{watch?.name ?? remembered?.name ?? 'No watch paired'}</ThemedText>
            <ThemedText type="small" themeColor="textSecondary">
              {inBootloader ? 'Connected, in bootloader' : stateLabel[connection]}
              {transportKind === 'mock' ? ' (mock watch)' : ''}
            </ThemedText>
          </div>
        </div>
        {lastError ? (
          <ThemedText type="small" themeColor="danger">
            {lastError}
          </ThemedText>
        ) : null}
        <div class="stack" style={{ marginTop: '8px' }}>
          {connected ? (
            <Button title="Disconnect" variant="secondary" onClick={() => disconnect()} />
          ) : (
            <>
              {remembered ? (
                <Button
                  title={`Reconnect to ${remembered.name}`}
                  busy={connection === 'connecting'}
                  onClick={() => connect(remembered)}
                />
              ) : null}
              <Button title="Find a watch" variant={remembered ? 'secondary' : 'primary'} onClick={() => router.push('/scan')} />
            </>
          )}
        </div>
      </Card>

      {inBootloader ? (
        <Card>
          <ThemedText type="smallBold" style={{ color: theme.warning }}>
            This watch is waiting in its bootloader.
          </ThemedText>
          <ThemedText type="small" themeColor="textSecondary">
            It is running no firmware, so nothing else here can reach it. Send it a firmware package
            from the Firmware tab, such as the BattleTime reloader, and it will restart into it.
          </ThemedText>
          <Button title="Go to Firmware" onClick={() => router.push('/firmware')} />
        </Card>
      ) : null}

      {findPhoneActive ? (
        <Card>
          <ThemedText type="smallBold" style={{ color: theme.accent }}>
            The watch is looking for this {deviceWord(phoneBridge.kind)}.
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
                onClick={() => router.push('/steps')}
              />
            </>
          ) : (
            <Row label={batteryLevel != null ? `Battery ${batteryLevel}%` : 'Battery'} detail={batteryLevel != null ? 'From the Battery service' : 'Not read yet'} />
          )}
          <Row
            label="Last synced"
            detail={lastSynced ? ago(lastSynced, now) : 'Not yet'}
            onClick={() => void refreshStatus().catch(() => {})}
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
            <div class="line" style={{ gap: '8px' }}>
              <Button title="Previous" variant="secondary" style={MEDIA_BUTTON} onClick={() => void phoneBridge.mediaCommand('previous')} />
              <Button
                title={nowPlaying.playing ? 'Pause' : 'Play'}
                style={MEDIA_BUTTON}
                onClick={() => void phoneBridge.mediaCommand(nowPlaying.playing ? 'pause' : 'play')}
              />
              <Button title="Next" variant="secondary" style={MEDIA_BUTTON} onClick={() => void phoneBridge.mediaCommand('next')} />
            </div>
          ) : null}
        </Card>
      ) : null}

      {watchReady && capabilities.diagnostics ? (
        <Card title="Diagnostics">
          {counters?.map((counter) => (
            <Row key={counter.key} label={counter.label} detail={counter.value.toLocaleString()} />
          ))}
          {diagnostics.error ? (
            <ThemedText type="small" themeColor="danger">
              {diagnostics.error}
            </ThemedText>
          ) : null}
          <Button
            title={counters ? 'Read again' : 'Read the counters'}
            variant="secondary"
            disabled={diagnostics.busy !== null}
            onClick={() =>
              void diagnostics.run('Reading', readDiagnostics).then((read) => read && setCounters(read))
            }
          />
        </Card>
      ) : null}

      {watchReady && capabilities.diagnostics ? <DebugReportCard /> : null}

      {canInstall && setupNeeded ? (
        <Card>
          <ThemedText type="smallBold" style={{ color: theme.accent }}>
            This watch has no face yet.
          </ThemedText>
          <ThemedText type="small" themeColor="textSecondary">
            BattleTime starts with no apps. Install a watch face, the launcher and the apps from this{' '}
            {deviceWord(phoneBridge.kind)}.
          </ThemedText>
          <Button title="Set up the watch" onClick={() => router.push('/setup')} />
        </Card>
      ) : null}

      {uartReady || (watchReady && capabilities.files) ? (
        <Card title="On the watch">
          {uartReady ? (
            <>
              <Row label="Steps" detail="The last week, day by day and hour by hour" onClick={() => router.push('/steps')} />
              <Row label="Alarms" detail="Set times and the days they repeat on" onClick={() => router.push('/alarms')} />
              <Row
                label="Watch settings"
                detail="Theme, brightness, timeout, chimes, units and step goal"
                onClick={() => router.push('/watch-settings')}
              />
            </>
          ) : null}
          {canInstall ? (
            <>
              <Row label="Apps" detail="Install, hide or remove apps and faces" onClick={() => router.push('/apps')} />
              <Row label="Plugins" detail="Watch apps with a part that runs here" onClick={() => router.push('/plugins')} />
              <Row
                label="Quick apps"
                detail="The apps a swipe left or right on the watch face steps through"
                onClick={() => router.push('/quick-apps')}
              />
            </>
          ) : null}
          {capabilities.files ? (
            <>
              <Row label="Notes" detail="Show a text file on the watch, kept up to date" onClick={() => router.push('/notes')} />
              <Row label="QR code" detail="Show a link, some text or a Wi-Fi network as a code" onClick={() => router.push('/qr-code')} />
              <Row label="Survival" detail="Choose the emergency guides the watch carries" onClick={() => router.push('/survival')} />
              <Row label="Launcher" detail="Group the apps and choose how the launcher shows them" onClick={() => router.push('/launcher')} />
              <Row label="Remote" detail="Buttons on the watch that run actions here" onClick={() => router.push('/remote')} />
              <Row label="Files" detail="Browse, send and delete files, and check free space" onClick={() => router.push('/files')} />
              <Row label="Backup" detail="Save the watch's settings and app state, and put them back" onClick={() => router.push('/backup')} />
            </>
          ) : null}
        </Card>
      ) : null}

      {/* The store is browsed and filled without a watch; its install buttons wait for one. */}
      <Card>
        <Row label="Store" detail="Every app, face and plugin to install, and more from GitHub" onClick={() => router.push('/store')} />
      </Card>

      {uartReady ? (
        <Card title="Quick actions">
          <Row
            label="Find watch"
            detail="Vibrate the watch until it is tapped"
            onClick={() => send({ t: 'find', n: true })}
          />
          <Row
            label="Test notification"
            detail="Send a sample message to the watch"
            onClick={() =>
              send({
                t: 'notify',
                id: Date.now() % 100000,
                src: 'BattleTime Companion',
                title: `Hello from your ${deviceWord(phoneBridge.kind)}`,
                body: 'Notifications are working.',
              })
            }
          />
          <Row
            label="Vibrate"
            detail="One short pulse"
            onClick={() => send({ t: 'vibrate', n: 1 })}
          />
          {weatherSource ? (
            <Row
              label="Send weather"
              detail={`Current conditions ${settings.weatherFromDevice ? '' : 'in '}${weatherSource}`}
              onClick={() => void sendWeather().catch(() => {})}
            />
          ) : null}
        </Card>
      ) : null}
    </Screen>
  );
}
