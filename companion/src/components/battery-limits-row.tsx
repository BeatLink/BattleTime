import { useEffect, useState } from 'preact/hooks';

import type { PhoneBridge } from '@/services/phone';

import { Row } from './row';

type BatteryLimitsRowProps = {
  bridge: Pick<PhoneBridge, 'isIgnoringBatteryOptimizations' | 'requestIgnoreBatteryOptimizations'>;
};

// Android may stop the app in the background unless its battery limits are lifted, which this row offers to do.
export function BatteryLimitsRow({ bridge }: BatteryLimitsRowProps) {
  const [lifted, setLifted] = useState<boolean | null>(null);
  useEffect(() => {
    bridge.isIgnoringBatteryOptimizations().then(setLifted, () => setLifted(false));
  }, [bridge]);

  const ask = () => {
    bridge.requestIgnoreBatteryOptimizations().then(setLifted, () => {});
  };
  return (
    <Row
      label="Battery limits"
      detail={
        lifted === null
          ? 'Checking'
          : lifted
            ? 'Lifted, so Android lets the app keep the watch in the background'
            : 'On, so Android may stop the app in the background; tap to lift them'
      }
      onClick={lifted === false ? ask : undefined}
    />
  );
}
