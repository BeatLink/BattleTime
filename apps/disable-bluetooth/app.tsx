// Bluetooth switch, ported from wasp-os's disa_b_l_e (DisaBLE).
// It shows whether Bluetooth is on and has one large button to turn it off, to save power and keep
// the watch private, or on again; wasp-os had to reboot to turn it back on, which this does not.

import { computed } from "@preact/signals";
import { define_app, is_bluetooth_enabled, is_phone_connected, set_bluetooth } from "neotime";
import { Card } from "neotime/components";
import { icons } from "neotime/icons";
import { Button, Icon, Label, Screen } from "neotime/widgets";

// ============================================================================
// Derived values
// ============================================================================

const status_text = computed(() =>
  !is_bluetooth_enabled.value ? "Bluetooth is off" : is_phone_connected.value ? "Connected to phone" : "Bluetooth is on",
);

// ============================================================================
// Handlers
// ============================================================================

function toggle_bluetooth() {
  set_bluetooth(!is_bluetooth_enabled.value);
}

// ============================================================================
// Screen
// ============================================================================

function DisableBluetooth() {
  return (
    <Screen padding={8} gap={6}>
      <Card grow={1} align_items="center" justify="center" gap={6} padding={0}>
        <Icon icon={icons.bluetooth} size={40} color={is_bluetooth_enabled.value ? "accent" : "muted"} />
        <Label>{status_text.value}</Label>
      </Card>
      <Button
        grow={1}
        background={is_bluetooth_enabled.value ? "danger" : "accent"}
        onClick={toggle_bluetooth}
      >
        {is_bluetooth_enabled.value ? "Disable" : "Enable"}
      </Button>
    </Screen>
  );
}

export default define_app({
  id: "disable-bluetooth",
  name: "DisaBLE",
  group: "system",
  icon: icons.bluetooth,
  screens: { main: DisableBluetooth },
  permissions: ["settings"],
});
