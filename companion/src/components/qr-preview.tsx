import { useMemo } from 'react';
import { StyleSheet, View } from 'react-native';

import { darkRuns } from '@/protocol/qr-code';

// The quiet zone the watch paints around the code, in modules.
const QUIET_MODULES = 2;

// The largest side the preview takes, in pixels.
type QrPreviewProps = { bitmap: Uint8Array; size?: number };

// Draws a packed /qr/code bitmap black on white, with a white margin as the watch does.
export function QrPreview({ bitmap, size = 240 }: QrPreviewProps) {
  const runs = useMemo(() => darkRuns(bitmap), [bitmap]);
  const modules = bitmap[0] + 2 * QUIET_MODULES;
  // Whole pixels a module, so neighbouring rows meet without hairline gaps.
  const moduleSize = Math.max(1, Math.floor(size / modules));
  const side = moduleSize * modules;
  return (
    <View
      style={[styles.paper, { width: side, height: side }]}
      accessibilityRole="image"
      accessibilityLabel={`QR code, ${bitmap[0]} modules wide`}>
      {runs.map((run) => (
        <View
          key={`${run.row}:${run.column}`}
          style={[
            styles.ink,
            {
              left: (run.column + QUIET_MODULES) * moduleSize,
              top: (run.row + QUIET_MODULES) * moduleSize,
              width: run.length * moduleSize,
              height: moduleSize,
            },
          ]}
        />
      ))}
    </View>
  );
}

const styles = StyleSheet.create({
  paper: {
    backgroundColor: '#ffffff',
    alignSelf: 'center',
  },
  ink: {
    position: 'absolute',
    backgroundColor: '#000000',
  },
});
