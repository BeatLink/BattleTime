import { useRouter } from 'expo-router';
import { useEffect } from 'react';
import { ActivityIndicator, StyleSheet, View } from 'react-native';

import { Button } from '@/components/button';
import { Card } from '@/components/card';
import { Row } from '@/components/row';
import { Screen } from '@/components/screen';
import { ThemedText } from '@/components/themed-text';
import { Spacing } from '@/constants/theme';
import { useWatch } from '@/hooks/use-watch';

export default function ScanScreen() {
  const router = useRouter();
  const { scanning, found, lastError, startScan, stopScan, connect } = useWatch();

  useEffect(() => {
    startScan();
    return () => {
      stopScan();
    };
  }, [startScan, stopScan]);

  return (
    <Screen title="Nearby watches">
      <Card>
        {found.length === 0 ? (
          <View style={styles.empty}>
            {scanning ? <ActivityIndicator /> : null}
            <ThemedText type="small" themeColor="textSecondary">
              {scanning
                ? 'Looking for watches, and for any waiting in a bootloader.'
                : 'No watches found.'}
            </ThemedText>
          </View>
        ) : (
          found.map((watch) => (
            <Row
              key={watch.id}
              label={watch.bootloader ? `${watch.name} (bootloader)` : watch.name}
              detail={watch.rssi != null ? `${watch.id}  ·  ${watch.rssi} dBm` : watch.id}
              onPress={async () => {
                await connect(watch);
                router.back();
              }}
            />
          ))
        )}
        {lastError ? (
          <ThemedText type="small" themeColor="danger">
            {lastError}
          </ThemedText>
        ) : null}
      </Card>
      <Button
        title={scanning ? 'Stop scanning' : 'Scan again'}
        variant="secondary"
        onPress={() => (scanning ? stopScan() : startScan())}
      />
    </Screen>
  );
}

const styles = StyleSheet.create({
  empty: {
    alignItems: 'center',
    gap: Spacing.two,
    paddingVertical: Spacing.four,
  },
});
