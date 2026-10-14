import { useEffect } from 'preact/hooks';

import { Button } from '@/components/button';
import { Card } from '@/components/card';
import { Row } from '@/components/row';
import { Screen } from '@/components/screen';
import { Spinner } from '@/components/spinner';
import { ThemedText } from '@/components/themed-text';
import { useWatch } from '@/hooks/use-watch';
import { useRouter } from '@/navigation';

export default function ScanScreen() {
  const router = useRouter();
  const { scanning, found, lastError, startScan, stopScan, connect } = useWatch();

  useEffect(() => {
    void startScan();
    return () => {
      void stopScan();
    };
  }, [startScan, stopScan]);

  return (
    <Screen title="Nearby watches">
      <Card>
        {found.length === 0 ? (
          <div class="stack" style={{ alignItems: 'center', padding: '24px 0' }}>
            {scanning ? <Spinner /> : null}
            <ThemedText type="small" themeColor="textSecondary">
              {scanning
                ? 'Looking for watches, and for any waiting in a bootloader.'
                : 'No watches found.'}
            </ThemedText>
          </div>
        ) : (
          found.map((watch) => (
            <Row
              key={watch.id}
              label={watch.bootloader ? `${watch.name} (bootloader)` : watch.name}
              detail={watch.rssi != null ? `${watch.id}  ·  ${watch.rssi} dBm` : watch.id}
              onClick={async () => {
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
        onClick={() => (scanning ? stopScan() : startScan())}
      />
    </Screen>
  );
}
