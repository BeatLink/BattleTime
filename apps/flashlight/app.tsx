// Flashlight, merged from InfiniTime's FlashLight and wasp-os's flashlight.
// From InfiniTime: a tap turns the light on and off, swipes left and right set the brightness, and
// three marks show the level.
// From wasp-os: it opens already lit at full brightness, and a red light that keeps night vision,
// chosen here by swiping up or down.

import { computed, signal } from "@preact/signals";
import { define_app, keep_screen_on, persisted, set_brightness, type Color, type SwipeEvent } from "neotime";
import { icons } from "neotime/icons";
import { Column, Icon, Row, Screen } from "neotime/widgets";

// ============================================================================
// State
// ============================================================================

const LOWEST_LEVEL = 1;
const HIGHEST_LEVEL = 3;
const LEVEL_PLACES = [1, 2, 3];

const is_on = signal(true);

/** The backlight level while the light is on, 1 to 3. */
const level = persisted("level", HIGHEST_LEVEL);

const is_red = persisted("is_red", false);

// ============================================================================
// Derived values
// ============================================================================

const background = computed<Color>(() => (!is_on.value ? "#000000" : is_red.value ? "#ff0000" : "#ffffff"));

const foreground = computed<Color>(() => (is_on.value ? "#606060" : "#ffffff"));

// ============================================================================
// Handlers
// ============================================================================

function apply_brightness() {
  set_brightness(is_on.value ? level.value : LOWEST_LEVEL);
}

function toggle() {
  is_on.value = !is_on.value;
  apply_brightness();
}

function swipe(event: SwipeEvent) {
  if (event.direction === "left") {
    level.value = Math.max(LOWEST_LEVEL, level.value - 1);
  } else if (event.direction === "right") {
    level.value = Math.min(HIGHEST_LEVEL, level.value + 1);
  } else {
    is_red.value = !is_red.value;
  }
  apply_brightness();
}

function open() {
  is_on.value = true;
  keep_screen_on(true);
  apply_brightness();
}

// ============================================================================
// Screen
// ============================================================================

function LevelMark(props: { place: number }) {
  return (
    <Column
      width={15}
      height={10}
      border_width={2}
      border_color={foreground.value}
      background={level.value >= props.place ? foreground.value : background.value}
    />
  );
}

function Flashlight() {
  return (
    <Screen background={background.value} align_items="center" justify="center" gap={8} onClick={toggle} onSwipe={swipe}>
      <Icon icon={icons.flashlight} size={56} color={foreground.value} />
      <Row gap={8}>
        {LEVEL_PLACES.map((place) => (
          <LevelMark place={place} />
        ))}
      </Row>
    </Screen>
  );
}

export default define_app({
  id: "flashlight",
  name: "Flashlight",
  group: "tools",
  icon: icons.flashlight,
  screens: { main: Flashlight },
  onOpen: open,
});
