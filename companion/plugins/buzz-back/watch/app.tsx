// Buzz back, the example plugin's watch app: one button that asks the companion to buzz the watch.
// The plugin in ../phone answers on the "buzz-back" channel; its settings page chooses how many buzzes.

import { define_app, send_to_phone, signal, vibrate_pattern } from "battletime";
import { icons } from "battletime/icons";
import { Button, Column, Label, Screen } from "battletime/widgets";

/** How many times the button was pressed since the app opened. */
const presses = signal(0);

function press() {
  vibrate_pattern("tap");
  presses.value = presses.value + 1;
  send_to_phone("buzz-back", { presses: presses.value });
}

function BuzzBack() {
  return (
    <Screen padding={16}>
      <Column grow={1} justify="center" gap={12}>
        <Button height={120} radius={24} background="surface" onClick={press}>
          <Label font="large" color="accent" text_align="center">
            Buzz me
          </Label>
        </Button>
        <Label color="muted" text_align="center">
          {presses.value === 0 ? "The phone buzzes back" : `Pressed ${presses.value} times`}
        </Label>
      </Column>
    </Screen>
  );
}

export default define_app({
  id: "buzz-back",
  name: "Buzz back",
  group: "tools",
  icon: icons.phone,
  screens: { main: BuzzBack },
});
