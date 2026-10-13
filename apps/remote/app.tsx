// Remote: up to nine buttons the companion app sets up, each telling the phone which one was tapped.
// The phone writes /remote/buttons: the format version (1), the button count, then each button's
// shared library icon by its place in the library. The phone runs each button's action itself.

import { computed, signal } from "@preact/signals";
import { define_app, file_bytes, range, send_to_phone, vibrate_pattern } from "neotime";
import { icons, library_icon } from "neotime/icons";
import { Button, Column, Icon, Label, Screen } from "neotime/widgets";

// ============================================================================
// The button file
// ============================================================================

const BUTTONS_PATH = "/remote/buttons";
const FILE_FORMAT_VERSION = 1;
const MOST_BUTTONS = 9;
/** The version and the count come before the icons. */
const HEADER_BYTES = 2;
const FILE_BYTES = HEADER_BYTES + MOST_BUTTONS;

/** The file as the phone wrote it, with zeros past its end or when it is missing. */
const button_file = signal([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);

/** How many buttons to show; none for a missing file or one in another format. */
const button_count = computed(() =>
  button_file.value[0] === FILE_FORMAT_VERSION ? Math.min(button_file.value[1], MOST_BUTTONS) : 0,
);

function read_buttons() {
  button_file.value = file_bytes(BUTTONS_PATH, FILE_BYTES);
}

function press(button: number) {
  vibrate_pattern("tap");
  send_to_phone("remote", { button });
}

// ============================================================================
// Layout
// ============================================================================

const SCREEN_PADDING = 8;
const GAP = 8;
const INSIDE_SIZE = 240 - 2 * SCREEN_PADDING;

/** One button fills the screen, two sit side by side, up to four make a 2 by 2 grid, and more use three columns. */
const columns = computed(() => (button_count.value <= 1 ? 1 : button_count.value <= 4 ? 2 : 3));
const rows = computed(() => (button_count.value <= 2 ? 1 : button_count.value <= 6 ? 2 : 3));
const button_width = computed(() => Math.trunc((INSIDE_SIZE - (columns.value - 1) * GAP) / columns.value));
const button_height = computed(() => Math.trunc((INSIDE_SIZE - (rows.value - 1) * GAP) / rows.value));
const icon_size = computed(() => (button_count.value <= 4 ? 64 : 48));

// ============================================================================
// Screen
// ============================================================================

function Remote() {
  return (
    <Screen padding={SCREEN_PADDING}>
      {button_count.value === 0 && (
        <Column grow={1} justify="center">
          <Label color="muted" text_align="center">
            Choose buttons in the NeoTime companion app.
          </Label>
        </Column>
      )}
      {range(MOST_BUTTONS).map(
        (button) =>
          button < button_count.value && (
            <Button
              x={(button % columns.value) * (button_width.value + GAP)}
              y={Math.trunc(button / columns.value) * (button_height.value + GAP)}
              width={button_width.value}
              height={button_height.value}
              radius={16}
              background="surface"
              onClick={() => press(button)}
            >
              <Icon icon={library_icon(button_file.value[HEADER_BYTES + button])} size={icon_size.value} color="accent" />
            </Button>
          ),
      )}
    </Screen>
  );
}

export default define_app({
  id: "remote",
  name: "Remote",
  group: "phone",
  icon: icons.phone,
  screens: { main: Remote },
  permissions: ["phone_messages"],
  onOpen: read_buttons,
  onScreenOn: read_buttons,
});
