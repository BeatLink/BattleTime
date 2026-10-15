// Prompt: a question from the phone, answered Yes or No on the watch.
// The companion app runs it as a temporary app: it writes the question to /temp/prompt/text and
// opens the app without installing it. A tap tells the phone the answer and closes the app.

import { back, define_app, send_to_phone, vibrate_pattern } from "battletime";
import { icons } from "battletime/icons";
import { Button, Row, Screen, TextFile } from "battletime/widgets";

// ============================================================================
// Answering
// ============================================================================

function answer(is_yes: boolean) {
  vibrate_pattern("tap");
  send_to_phone("prompt", { answer: is_yes ? 1 : 0 });
  back();
}

// ============================================================================
// Screen
// ============================================================================

function Prompt() {
  return (
    <Screen padding={8} gap={8}>
      <TextFile path="/temp/prompt/text" placeholder="No question from the phone." grow={1} padding={4} />
      <Row gap={8} height={64}>
        <Button grow={1} background="surface" radius={16} onClick={() => answer(false)}>
          No
        </Button>
        <Button grow={1} background="accent" radius={16} onClick={() => answer(true)}>
          Yes
        </Button>
      </Row>
    </Screen>
  );
}

export default define_app({
  id: "prompt",
  name: "Prompt",
  group: "phone",
  icon: icons.phone,
  screens: { main: Prompt },
});
