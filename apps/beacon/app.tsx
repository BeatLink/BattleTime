// Beacon that flashes the heart rate sensor's LED, ported from wasp-os's beacon app.
// A switch turns it on and off, two sliders set the intensity and the frequency, and a strip of
// dashes previews the pattern. The sensor times the flashes itself, so it costs little power,
// and it keeps flashing with the screen off for as long as the app stays in front.

import { computed, signal } from "@preact/signals";
import { define_app, persisted, range, set_heart_rate_led } from "neotime";
import { Card } from "neotime/components";
import { icons } from "neotime/icons";
import { CellGrid, Label, Row, Screen, Slider, Switch } from "neotime/widgets";

// ============================================================================
// State
// ============================================================================

/** The sensor's four LED drive levels, 0 the dimmest. */
const HIGHEST_INTENSITY = 3;

/** The sensor's eight wait times between flashes, in milliseconds, slowest first. */
const WAIT_TIMES = [800, 400, 200, 100, 75, 50, 12, 0];
const HIGHEST_FREQUENCY = 7;

/** The LED's green, used for the preview dashes. */
const LED_COLOR = "#20fc20";

const PREVIEW_COLUMNS = 25;
const PREVIEW_ROWS = 4;

/** The order the preview's rows light as the intensity rises, so a dash grows from the middle. */
const ROW_LIGHTING_RANK = [1, 0, 2, 3];

/** Whether the LED is flashing; the runtime turns it off when the app closes. */
const is_on = signal(false);

const intensity = persisted("intensity", 0);

const frequency = persisted("frequency", 0);

// ============================================================================
// Derived values
// ============================================================================

/** One dash in every few columns, closer together as the frequency rises, as thick as the intensity. */
const preview_cells = computed(() =>
  range(PREVIEW_COLUMNS * PREVIEW_ROWS).map((cell) =>
    (cell % PREVIEW_COLUMNS) % (HIGHEST_FREQUENCY + 1 - frequency.value) === 0 &&
    ROW_LIGHTING_RANK[Math.floor(cell / PREVIEW_COLUMNS)] <= intensity.value
      ? 1
      : 0,
  ),
);

// ============================================================================
// Handlers
// ============================================================================

/** Sends the switch and both sliders to the sensor. */
function apply_led() {
  set_heart_rate_led(is_on.value, intensity.value, WAIT_TIMES[frequency.value]);
}

function turn_off() {
  is_on.value = false;
}

// ============================================================================
// Screen
// ============================================================================

function Beacon() {
  return (
    <Screen padding={4} gap={4}>
      <Card>
        <Row justify="space_between" align_items="center">
          <Label>Beacon</Label>
          <Switch value={is_on} onChange={apply_led} />
        </Row>
      </Card>
      <Card>
        <Label font="small" color="muted">Intensity</Label>
        <Slider value={intensity} minimum={0} maximum={HIGHEST_INTENSITY} onChange={apply_led} />
      </Card>
      <Card>
        <Label font="small" color="muted">Frequency</Label>
        <Slider value={frequency} minimum={0} maximum={HIGHEST_FREQUENCY} onChange={apply_led} />
      </Card>
      <Card align_items="center" padding={6}>
        <CellGrid
          columns={PREVIEW_COLUMNS}
          rows={PREVIEW_ROWS}
          cell_size={8}
          cells={preview_cells.value}
          palette={["surface", LED_COLOR]}
        />
      </Card>
    </Screen>
  );
}

export default define_app({
  id: "beacon",
  name: "Beacon",
  icon: icons.flashlight,
  screens: { main: Beacon },
  permissions: ["heart_rate"],
  onClose: turn_off,
});
