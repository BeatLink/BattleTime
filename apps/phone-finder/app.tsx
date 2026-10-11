// Phone finder, ported from wasp-os's phone_finder.
// A title card above a large button that makes the connected phone ring.

import { define_app, find_phone, is_phone_connected, show_toast } from "neotime";
import { icons } from "neotime/icons";
import { Button, Column, Icon, Label, Screen } from "neotime/widgets";

// ============================================================================
// Handlers
// ============================================================================

function ring_phone() {
  find_phone();
  show_toast("Ringing your phone");
}

// ============================================================================
// Screen
// ============================================================================

function PhoneFinder() {
  return (
    <Screen padding={4} gap={6}>
      <Column grow={1} align_items="center" justify="center" gap={6} background="surface" radius={12}>
        <Icon icon={icons.phone} size={40} />
        <Label>Find phone</Label>
        {!is_phone_connected.value && (
          <Label font="small" color="muted">
            Phone not connected
          </Label>
        )}
      </Column>
      <Button grow={1} background="accent" disabled={!is_phone_connected.value} onClick={ring_phone}>
        Ring
      </Button>
    </Screen>
  );
}

export default define_app({
  id: "phone-finder",
  name: "Finder",
  icon: icons.phone,
  screens: { main: PhoneFinder },
  permissions: ["phone"],
});
