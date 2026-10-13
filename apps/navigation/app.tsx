// Turn-by-turn directions from the phone, ported from InfiniTime's Navigation.
// It shows the next turn's arrow, the instruction, the distance to it in the wearer's units, and the
// route's progress.

import { computed, signal } from "@preact/signals";
import { current_navigation, define_app, format_fixed, navigation, request_navigation_state, settings } from "neotime";
import { icon_named, icons } from "neotime/icons";
import { Bar, Icon, Label, Screen } from "neotime/widgets";

// ============================================================================
// State
// ============================================================================

/** Below a tenth of a mile, about 161 metres, imperial distances are in feet. */
const TENTH_OF_A_MILE_IN_METRES = 161;

/** The bar turns red once this much of the route is done. */
const ALMOST_THERE_PERCENT = 90;

/** Whether the phone has sent a step since the app opened; its text cannot be compared on the watch. */
const has_step = signal(false);

// ============================================================================
// Derived values
// ============================================================================

/** Metres below a kilometre, and kilometres to one decimal above it. */
function metric_text(metres: number): string {
  return metres < 1000 ? `${metres} m` : `${format_fixed(Math.floor(metres / 100), 1)} km`;
}

/** Feet below a tenth of a mile, and miles to one decimal above it. */
function imperial_text(metres: number): string {
  if (metres < TENTH_OF_A_MILE_IN_METRES) return `${Math.floor((metres * 3281) / 1000)} ft`;
  const tenths_of_a_mile = Math.floor((metres * 10) / 1609);
  return `${format_fixed(tenths_of_a_mile, 1)} mi`;
}

const distance_text = computed(() =>
  settings.uses_metric_units.value
    ? metric_text(current_navigation.value.distance)
    : imperial_text(current_navigation.value.distance),
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
  group: "phone",
  icon: icons.navigation,
  screens: { main: Navigation },
  subscriptions: [navigation(receive, { only_while_open: true })],
  permissions: ["phone"],
  onOpen: open,
});
