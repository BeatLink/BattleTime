// Quick settings, from InfiniTime's QuickSettings screen and Android's pull-down shade.
// A second swipe down from the notifications pulls it down. The time and battery sit at the top,
// over six buttons: brightness, the flashlight, do not disturb, Bluetooth, the always-on screen
// and the full Settings app. As InfiniTime's, the do not disturb button steps through three
// states: off, silent, and sleep, which also stops every wake gesture and the always-on screen.
// A swipe up or the side button puts it away.

import {
  battery_percent,
  brightness,
  computed,
  define_app,
  format_time,
  is_bluetooth_enabled,
  is_charging,
  is_do_not_disturb,
  now,
  open_app,
  set_bluetooth,
  set_do_not_disturb,
  set_setting,
  settings,
  type Color,
} from "neotime";
import { icons, type LibraryIcon } from "neotime/icons";
import { Button, Column, Grid, Icon, Label, Row, Screen } from "neotime/widgets";

// ============================================================================
// Layout
// ============================================================================

const BRIGHTNESS_NAMES = ["Low", "Medium", "High"];

const HIGHEST_BRIGHTNESS = 3;

// ============================================================================
// Derived values
// ============================================================================

const battery_text = computed(() => `${battery_percent.value}%`);

const brightness_name = computed(() => BRIGHTNESS_NAMES[brightness.value - 1]);

const silence_name = computed(() => (settings.sleep_mode.value ? "Sleep" : "Silent"));

const silence_icon = computed(() =>
  settings.sleep_mode.value ? icons.moon : is_do_not_disturb.value ? icons.bell_off : icons.bell,
);

// ============================================================================
// Handlers
// ============================================================================

export function next_brightness() {
  set_setting("brightness", (brightness.value % HIGHEST_BRIGHTNESS) + 1);
}

/** Steps from off to silent to sleep and back to off; turning do not disturb off ends sleep mode too. */
export function next_silence() {
  if (settings.sleep_mode.value) set_do_not_disturb(false);
  else if (is_do_not_disturb.value) set_setting("sleep_mode", 1);
  else set_do_not_disturb(true);
}

export function toggle_bluetooth() {
  set_bluetooth(!is_bluetooth_enabled.value);
}

export function toggle_always_on_screen() {
  set_setting("always_on_screen", settings.always_on_screen.value ? 0 : 1);
}

// ============================================================================
// Screen
// ============================================================================

/** One button: an icon over its name, lit in the accent colour while its setting is on. */
function Tile(props: { icon: LibraryIcon; name: string; is_on: boolean; onClick: () => void }) {
  const background: Color = props.is_on ? "accent" : "surface";
  return (
    <Button background={background} onClick={props.onClick}>
      <Column grow={1} align_items="center" justify="center" gap={2}>
        <Icon icon={props.icon} size={24} />
        <Label font="small" long_mode="clip">{props.name}</Label>
      </Column>
    </Button>
  );
}

function QuickSettings() {
  return (
    <Screen padding={8} gap={8}>
      <Row gap={8} align_items="center">
        <Label font="large" grow={1}>{format_time(now.value, "clock")}</Label>
        <Icon icon={is_charging.value ? icons.charging : icons.battery} size={20} color="muted" />
        <Label color="muted">{battery_text.value}</Label>
      </Row>
      <Grid grow={1} columns={2} gap={6}>
        <Tile icon={icons.sun} name={brightness_name.value} is_on={false} onClick={next_brightness} />
        <Tile icon={icons.flashlight} name="Light" is_on={false} onClick={() => open_app("flashlight")} />
        <Tile icon={silence_icon.value} name={silence_name.value} is_on={is_do_not_disturb.value} onClick={next_silence} />
        <Tile icon={icons.bluetooth} name="Bluetooth" is_on={is_bluetooth_enabled.value} onClick={toggle_bluetooth} />
        <Tile
          icon={icons.clock}
          name="Always on"
          is_on={settings.always_on_screen.value}
          onClick={toggle_always_on_screen}
        />
        <Tile icon={icons.settings} name="Settings" is_on={false} onClick={() => open_app("settings")} />
      </Grid>
    </Screen>
  );
}

export default define_app({
  id: "quick-settings",
  name: "Quick settings",
  icon: icons.settings,
  screens: { main: QuickSettings },
});
