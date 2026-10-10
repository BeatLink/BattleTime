import { useState } from 'react';
import { ActivityIndicator, StyleSheet, View } from 'react-native';

import { Button } from '@/components/button';
import { Card } from '@/components/card';
import { Row } from '@/components/row';
import { Screen } from '@/components/screen';
import { ThemedText } from '@/components/themed-text';
import { Radius, Spacing } from '@/constants/theme';
import { useTheme } from '@/hooks/use-theme';
import { useWatch } from '@/hooks/use-watch';
import { downloadFirmware, listFirmwareBuilds, type FirmwareBuild } from '@/services/github-firmware';
import { useFirmware } from '@/state/use-firmware';
import { useTask } from '@/state/use-task';

function kilobytes(bytes: number): string {
  return `${Math.round(bytes / 1024)} kB`;
}

export default function FirmwareScreen() {
  const theme = useTheme();
  const { connection, dfuLink, sendRaw, settings } = useWatch();
  const connected = connection === 'connected';
  const firmware = useFirmware(dfuLink, sendRaw);
  const github = useTask();
  const [builds, setBuilds] = useState<FirmwareBuild[] | null>(null);
  const token = settings.githubToken || undefined;

  const percent = firmware.total > 0 ? Math.round((firmware.sent / firmware.total) * 100) : 0;

  return (
    <Screen title="Firmware">
      <Card>
        <ThemedText type="small" themeColor="textSecondary">
          A firmware update is sent as a Nordic DFU package, the zip the wasp-os build produces. The
          watch restarts into its bootloader, takes the new image and restarts again.
        </ThemedText>
      </Card>

      <Card title="Package">
        {firmware.firmware ? (
          <>
            <Row label={firmware.firmware.name} detail={kilobytes(firmware.total)} />
            {firmware.firmware.images.map((image) => (
              <Row
                key={image.part}
                label={image.part}
                detail={kilobytes(image.image.length)}
              />
            ))}
          </>
        ) : (
          <ThemedText type="small" themeColor="textSecondary">
            No package chosen yet.
          </ThemedText>
        )}
        <Button
          title={firmware.firmware ? 'Choose another package' : 'Choose a package'}
          variant="secondary"
          disabled={firmware.flashing}
          onPress={() => firmware.choose()}
        />
      </Card>

      <Card title={`Builds from ${settings.githubRepo}`}>
        {builds === null ? (
          <ThemedText type="small" themeColor="textSecondary">
            Releases and recent Actions builds for the {settings.firmwareBoard}. Actions builds need a
            GitHub token, set in Settings.
          </ThemedText>
        ) : builds.length === 0 ? (
          <ThemedText type="small" themeColor="textSecondary">
            No builds for the {settings.firmwareBoard} were found.
          </ThemedText>
        ) : (
          builds.slice(0, 15).map((build) => (
            <Row
              key={build.id}
              label={build.title}
              detail={`${build.detail}, ${new Date(build.date).toLocaleDateString()}, ${kilobytes(build.sizeBytes)}${
                build.needsToken && !token ? ', needs a token' : ''
              }`}
              onPress={
                firmware.flashing
                  ? undefined
                  : () =>
                      void github.run(`Downloading ${build.title}`, async () =>
                        firmware.load(await downloadFirmware(build, { token })),
                      )
              }
            />
          ))
        )}
        {github.busy ? (
          <ThemedText type="small" themeColor="textSecondary">
            {github.busy}...
          </ThemedText>
        ) : null}
        {github.error ? (
          <ThemedText type="small" style={{ color: theme.danger }}>
            {github.error}
          </ThemedText>
        ) : null}
        <Button
          title={builds ? 'Look again' : 'Look for builds'}
          variant="secondary"
          busy={github.busy === 'Looking for builds'}
          disabled={firmware.flashing}
          onPress={() =>
            void github.run('Looking for builds', async () =>
              setBuilds(await listFirmwareBuilds(settings.githubRepo, settings.firmwareBoard, { token })),
            )
          }
        />
      </Card>

      {firmware.flashing || firmware.status ? (
        <Card title="Progress">
          <View style={styles.status}>
            {firmware.flashing ? <ActivityIndicator color={theme.accent} /> : null}
            <ThemedText type="small" style={styles.statusText}>
              {firmware.status}
            </ThemedText>
          </View>
          <View style={[styles.track, { backgroundColor: theme.backgroundSelected }]}>
            <View
              style={[styles.fill, { backgroundColor: theme.accent, width: `${percent}%` }]}
            />
          </View>
          <ThemedText type="small" themeColor="textSecondary">
            {kilobytes(firmware.sent)} of {kilobytes(firmware.total)} ({percent}%)
          </ThemedText>
        </Card>
      ) : null}

      {firmware.error ? (
        <Card>
          <ThemedText type="small" style={{ color: theme.danger }}>
            {firmware.error}
          </ThemedText>
          <Button title="Dismiss" variant="secondary" onPress={firmware.clearError} />
        </Card>
      ) : null}

      <Card>
        {connected ? null : (
          <ThemedText type="small" themeColor="textSecondary">
            Connect a watch before sending firmware to it.
          </ThemedText>
        )}
        <ThemedText type="small" themeColor="textSecondary">
          Keep the watch close and charged. An update that is cut short leaves the watch in its
          bootloader, where it can be sent the package again.
        </ThemedText>
        <Button
          title={firmware.finished ? 'Send again' : 'Send to the watch'}
          variant="danger"
          busy={firmware.flashing}
          disabled={!connected || !firmware.firmware}
          onPress={() => firmware.flash()}
        />
      </Card>
    </Screen>
  );
}

const styles = StyleSheet.create({
  status: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: Spacing.two,
  },
  statusText: {
    flex: 1,
  },
  track: {
    height: 8,
    borderRadius: Radius.small,
    overflow: 'hidden',
  },
  fill: {
    height: '100%',
  },
});
