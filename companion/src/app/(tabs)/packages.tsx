import { useRouter } from 'expo-router';
import { ActivityIndicator, StyleSheet, Switch, View } from 'react-native';

import { Button } from '@/components/button';
import { Card } from '@/components/card';
import { Row } from '@/components/row';
import { Screen } from '@/components/screen';
import { ThemedText } from '@/components/themed-text';
import { Spacing } from '@/constants/theme';
import { useTheme } from '@/hooks/use-theme';
import { useWatch } from '@/hooks/use-watch';
import { isUpdate, type CatalogEntry } from '@/packages/catalog';
import { usePackages } from '@/state/use-packages';

export default function PackagesScreen() {
  const router = useRouter();
  const theme = useTheme();
  const { connection, packageChannel } = useWatch();
  const connected = connection === 'connected';
  const packages = usePackages(packageChannel, connected);

  if (!connected) {
    return (
      <Screen title="Apps">
        <Card>
          <ThemedText type="small" themeColor="textSecondary">
            Connect a watch to install apps onto it.
          </ThemedText>
        </Card>
      </Screen>
    );
  }

  return (
    <Screen title="Apps">
      {packages.busy ? (
        <Card>
          <View style={styles.busy}>
            <ActivityIndicator color={theme.accent} />
            <View style={styles.busyText}>
              <ThemedText type="small">{packages.busy}</ThemedText>
              {packages.progress ? (
                <ThemedText type="small" themeColor="textSecondary">
                  {Math.round((packages.progress.sent / packages.progress.total) * 100)}% of{' '}
                  {packages.progress.file.split('/').pop()}
                </ThemedText>
              ) : null}
            </View>
          </View>
        </Card>
      ) : null}

      {packages.error ? (
        <Card>
          <ThemedText type="small" style={{ color: theme.danger }}>
            {packages.error}
          </ThemedText>
          <Button title="Try again" variant="secondary" onPress={() => packages.refresh()} />
        </Card>
      ) : null}

      <Card title="Available">
        {packages.catalog.map((entry) => (
          <PackageRow
            key={entry.name}
            entry={entry}
            disabled={packages.busy !== null}
            onInstall={() => packages.install(entry)}
            onUninstall={() => packages.uninstall(entry.name)}
            onToggle={(value) => packages.toggle(entry.name, value)}
            onConfigure={() => router.push({ pathname: '/configure', params: { name: entry.name } })}
          />
        ))}
      </Card>

      {packages.strays.length > 0 ? (
        <Card title="On the watch only">
          {packages.strays.map((entry) => (
            <Row
              key={entry.name}
              label={entry.name}
              detail={`Version ${entry.version}, not shipped with this app`}
              right={
                <Button
                  title="Remove"
                  variant="secondary"
                  disabled={packages.busy !== null}
                  onPress={() => packages.uninstall(entry.name)}
                />
              }
            />
          ))}
        </Card>
      ) : null}
    </Screen>
  );
}

type PackageRowProps = {
  entry: CatalogEntry;
  disabled: boolean;
  onInstall: () => void;
  onUninstall: () => void;
  onToggle: (value: boolean) => void;
  onConfigure: () => void;
};

function PackageRow({
  entry,
  disabled,
  onInstall,
  onUninstall,
  onToggle,
  onConfigure,
}: PackageRowProps) {
  const theme = useTheme();

  if (entry.blockedReason) {
    return (
      <Row
        label={entry.label}
        detail={entry.blockedReason}
        right={<ThemedText type="small" style={{ color: theme.warning }}>Incompatible</ThemedText>}
      />
    );
  }

  if (!entry.installed) {
    return (
      <Row
        label={entry.label}
        detail={`Version ${entry.version}, not installed`}
        right={<Button title="Install" disabled={disabled} onPress={onInstall} />}
      />
    );
  }

  const hasSettings = (entry.bundle.meta.config?.length ?? 0) > 0;

  return (
    <View style={styles.installed}>
      <Row
        label={entry.label}
        detail={
          isUpdate(entry)
            ? `Version ${entry.installedVersion} installed, ${entry.version} available`
            : `Version ${entry.version} installed`
        }
        right={
          <Switch
            value={entry.enabled}
            disabled={disabled}
            onValueChange={onToggle}
            trackColor={{ true: theme.accent, false: theme.backgroundSelected }}
            thumbColor={theme.text}
          />
        }
      />
      <View style={styles.actions}>
        {isUpdate(entry) ? (
          <Button title="Update" disabled={disabled} onPress={onInstall} />
        ) : null}
        {hasSettings ? (
          <Button title="Settings" variant="secondary" disabled={disabled} onPress={onConfigure} />
        ) : null}
        <Button title="Remove" variant="secondary" disabled={disabled} onPress={onUninstall} />
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  busy: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: Spacing.three,
  },
  busyText: {
    flex: 1,
    gap: Spacing.half,
  },
  installed: {
    gap: Spacing.two,
  },
  actions: {
    flexDirection: 'row',
    gap: Spacing.two,
    flexWrap: 'wrap',
  },
});
