// Turn-by-turn directions from the phone, ported from InfiniTime's Navigation.
// It shows the next turn's arrow, the instruction, the distance to it and the route's progress.

import { computed } from "@preact/signals";
import {
  define_app,
  navigation,
  persisted,
  type NavigationEvent,
  type NavigationTurn,
} from "neotime";
import { icon_named, icons } from "neotime/icons";
import { Bar, Icon, Label, Screen } from "neotime/widgets";

// ============================================================================
// State
// ============================================================================

/** The bar turns red once this much of the route is done. */
const ALMOST_THERE_PERCENT = 90;

const turn = persisted<NavigationTurn>("turn", "arrive");

/** The phone's words for the next turn, empty until it sends one. */
const instruction = persisted("instruction", "");

const distance = persisted("distance", "");

const progress_percent = persisted("progress_percent", 0);

// ============================================================================
// Derived values
// ============================================================================

const instruction_text = computed(() => (instruction.value === "" ? "Navigation" : instruction.value));

const distance_text = computed(() => (distance.value === "" ? "--M" : distance.value));

const progress_color = computed(() => (progress_percent.value > ALMOST_THERE_PERCENT ? "#ff0000" : "#ffb000"));

// ============================================================================
// Handlers
// ============================================================================

function receive(event: NavigationEvent) {
  turn.value = event.turn;
  instruction.value = event.instruction;
  distance.value = event.distance;
  progress_percent.value = event.progress_percent;
}

// ============================================================================
// Screen
// ============================================================================

function Navigation() {
  return (
    <Screen padding={6} align_items="center" gap={4}>
      <Icon icon={icon_named(`turn_${turn.value}`)} size={80} color="#00ffff" />
      <Label height={72} width={228} text_align="center">
        {instruction_text.value}
      </Label>
      <Label font="large" color="#00b000" text_align="center">
        {distance_text.value}
      </Label>
      <Bar width={200} height={20} value={progress_percent.value} color={progress_color.value} track_color="#222222" animate={500} />
    </Screen>
  );
}

export default define_app({
  id: "navigation",
  name: "Navigation",
  icon: icons.navigation,
  screens: { main: Navigation },
  subscriptions: [navigation(receive)],
  permissions: ["background"],
});
