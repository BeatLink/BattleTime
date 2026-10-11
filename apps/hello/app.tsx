// Hello world, from wasp-os's hello: the smallest app, one card with an icon and a greeting.

import { define_app } from "neotime";
import { Card } from "neotime/components";
import { icons } from "neotime/icons";
import { Icon, Label, Screen } from "neotime/widgets";

// ============================================================================
// Screen
// ============================================================================

const GREETING = "Hello, world!";

function Hello() {
  return (
    <Screen padding={6}>
      <Card grow={1} gap={16}>
        <Icon icon={icons.check} size={48} align="center" margin={24} />
        <Label text_align="center">{GREETING}</Label>
      </Card>
    </Screen>
  );
}

export default define_app({
  id: "hello",
  name: "Hello",
  icon: icons.check,
  screens: { main: Hello },
});
