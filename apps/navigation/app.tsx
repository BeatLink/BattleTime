// Turn-by-turn directions from the phone, ported from InfiniTime's Navigation.
// It shows the next turn's arrow, the instruction, the distance to it and the route's progress.

import { computed, signal } from "@preact/signals";
import { current_navigation, define_app, navigation, request_navigation_state } from "neotime";
import { icon_named, icons } from "neotime/icons";
import { Bar, Icon, Label, Screen } from "neotime/widgets";

// ============================================================================
// State
// ============================================================================

/** The bar turns red once this much of the route is done. */
const ALMOST_THERE_PERCENT = 90;

/** Whether the phone has sent a step since the app opened; its text cannot be compared on the watch. */
const has_step = signal(false);

// ============================================================================
// Derived values
// ============================================================================

/** Metres below a kilometre, and kilometres to one decimal above it. */
const distance_text = computed(() =>
  current_navigation.value.distance < 1000
    ? `${current_navigation.value.distance} m`
    : `${Math.floor(current_navigation.value.distance / 1000)}.${Math.floor((current_navigation.value.distance % 1000) / 100)} km`,
);

const progress_color = computed(() =>
  current_navigation.value.progress_percent > ALMOST_THERE_PERCENT ? "#ff0000" : "#ffb000",
);

// ============================================================================
// Handlers
// ============================================================================

/** The system keeps the step itself; this only notes that one has come. */
function receive() {
  has_step.value = true;
}

function open() {
  has_step.value = false;
  request_navigation_state();
}

// ============================================================================
// Screen
// ============================================================================

function Navigation() {
  return (
    <Screen padding={8} align_items="center" gap={6}>
      <Icon icon={icon_named(`turn_${current_navigation.value.turn}`)} size={80} color="#00ffff" />
      <Label height={56} width={224} text_align="center">
        {has_step.value ? current_navigation.value.instruction : "Navigation"}
      </Label>
      <Label font="large" color="#00b000" text_align="center">
        {has_step.value ? distance_text.value : "--M"}
      </Label>
      <Bar width={200} height={20} value={current_navigation.value.progress_percent} color={progress_color.value} track_color="#222222" animate={500} />
    </Screen>
  );
}

export default define_app({
  id: "navigation",
  name: "Navigation",
  icon: icons.navigation,
  screens: { main: Navigation },
  subscriptions: [navigation(receive, { only_while_open: true })],
  permissions: ["phone"],
  onOpen: open,
});
