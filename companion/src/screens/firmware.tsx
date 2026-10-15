import { useState } from 'preact/hooks';

import { Button } from '@/components/button';
import { Card } from '@/components/card';
import { Row } from '@/components/row';
import { Screen } from '@/components/screen';
import { Spinner } from '@/components/spinner';
import { ThemedText } from '@/components/themed-text';
import { theme } from '@/constants/theme';
import { useWatch } from '@/hooks/use-watch';
import { downloadFirmware, listFirmwareBuilds, type FirmwareBuild } from '@/services/github-firmware';
import { useFirmware } from '@/state/use-firmware';
import { useTask } from '@/state/use-task';

function kilobytes(bytes: number): string {
  return `${Math.round(bytes / 1024)} kB`;
}

export default function FirmwareScreen() {
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
          A firmware update is sent as a Nordic DFU package, the zip BattleTime&apos;s tools/package_dfu.py
          makes. BattleTime takes it while running, restarts into it, and keeps it once this app
          reconnects. A watch still on wasp-os takes the BattleTime reloader through its bootloader.
        </ThemedText>
      </Card>

      <Card title="Package">
        {firmware.firmware ? (
          <Row label={firmware.firmware.name} detail={kilobytes(firmware.total)} />
        ) : (
          <ThemedText type="small" themeColor="textSecondary">
            No package chosen yet.
          </ThemedText>
        )}
        <Button
          title={firmware.firmware ? 'Choose another package' : 'Choose a package'}
          variant="secondary"
          disabled={firmware.flashing}
          onClick={() => firmware.choose()}
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
              onClick={
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
          <ThemedText type="small" themeColor="danger">
            {github.error}
          </ThemedText>
        ) : null}
        <Button
          title={builds ? 'Look again' : 'Look for builds'}
          variant="secondary"
          busy={github.busy === 'Looking for builds'}
          disabled={firmware.flashing}
          onClick={() =>
            void github.run('Looking for builds', async () =>
              setBuilds(await listFirmwareBuilds(settings.githubRepo, settings.firmwareBoard, { token })),
            )
          }
        />
      </Card>

      {firmware.flashing || firmware.status ? (
        <Card title="Progress">
          <div class="line" style={{ gap: 8 }}>
            {firmware.flashing ? <Spinner color={theme.accent} /> : null}
            <ThemedText type="small" class="grow">
              {firmware.status}
            </ThemedText>
          </div>
          <div class="progress-track">
            <div class="progress-fill" style={{ width: `${percent}%` }} />
          </div>
          <ThemedText type="small" themeColor="textSecondary">
            {kilobytes(firmware.sent)} of {kilobytes(firmware.total)} ({percent}%)
          </ThemedText>
        </Card>
      ) : null}

      {firmware.error ? (
        <Card>
          <ThemedText type="small" themeColor="danger">
            {firmware.error}
          </ThemedText>
          <Button title="Dismiss" variant="secondary" onClick={firmware.clearError} />
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
          onClick={() => firmware.flash()}
        />
      </Card>
    </Screen>
  );
}
