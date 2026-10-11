// Metronome that beats on the motor, ported from InfiniTime's Metronome.
// Each beat is scheduled at its exact time from the start, a long buzz opening every bar and a
// short one for the other beats; tap the tempo to set it from the beat of a song.

import { computed, signal } from "@preact/signals";
import {
  at,
  clamp,
  define_app,
  format_number,
  keep_screen_on,
  now,
  persisted,
  vibrate,
  MINUTE,
  SECOND,
  type Instant,
} from "neotime";
import { icons } from "neotime/icons";
import { Arc, Button, ClickArea, Icon, Label, Row, Screen, Slider } from "neotime/widgets";

// ============================================================================
// State
// ============================================================================

const SLOWEST_TEMPO = 40;
const FASTEST_TEMPO = 220;
const LONGEST_BAR = 9;
const DOWNBEAT_BUZZ = 90;
const BEAT_BUZZ = 30;

/** Taps further apart than this start a new tempo instead of setting one. */
const TAP_WINDOW = 3 * SECOND;

const beats_per_minute = persisted("beats_per_minute", 120);

const beats_per_bar = persisted("beats_per_bar", 4);

/** When the first beat sounded, or null while stopped. */
const started_at = signal<Instant | null>(null);

/** Beats sounded since the start. */
const beats_played = signal(0);

/** When the tempo was last tapped, or 0. */
const tapped_at = signal<Instant>(0);

// ============================================================================
// Derived values
// ============================================================================

const is_running = computed(() => started_at.value !== null);

const next_beat = computed(() =>
  started_at.value === null
    ? null
    : started_at.value + Math.floor((beats_played.value * MINUTE) / beats_per_minute.value),
);

// ============================================================================
// Handlers
// ============================================================================

function beat() {
  vibrate(beats_played.value % beats_per_bar.value === 0 ? DOWNBEAT_BUZZ : BEAT_BUZZ);
  beats_played.value = beats_played.value + 1;
}

function start_or_stop() {
  if (started_at.value !== null) {
    stop();
  } else {
    started_at.value = now.value;
    beats_played.value = 0;
    keep_screen_on(true);
    beat();
  }
}

function stop() {
  started_at.value = null;
  keep_screen_on(false);
}

/** A new tempo keeps the beat going from the latest beat, so the bar does not jump. */
function set_tempo(tempo: number) {
  const tempo_in_range = clamp(tempo, SLOWEST_TEMPO, FASTEST_TEMPO);
  if (started_at.value !== null) {
    const beats_before_latest = beats_played.value - 1;
    const latest_beat = started_at.value + Math.floor((beats_before_latest * MINUTE) / beats_per_minute.value);
    started_at.value = latest_beat - Math.floor((beats_before_latest * MINUTE) / tempo_in_range);
  }
  beats_per_minute.value = tempo_in_range;
}

function tap_tempo() {
  const since_tap = now.value - tapped_at.value;
  if (tapped_at.value !== 0 && since_tap < TAP_WINDOW && since_tap > 0) {
    set_tempo(Math.floor(MINUTE / since_tap));
  }
  tapped_at.value = now.value;
}

function next_bar_length() {
  beats_per_bar.value = beats_per_bar.value >= LONGEST_BAR ? 1 : beats_per_bar.value + 1;
}

// ============================================================================
// Screen
// ============================================================================

function Metronome() {
  return (
    <Screen
      padding={4}
      align_items="center"
      gap={4}
      onButton={(event) => {
        if (event.kind === "press") start_or_stop();
      }}
    >
      <Arc
        diameter={150}
        thickness={10}
        start_angle={225}
        end_angle={135}
        value={beats_per_minute.value}
        minimum={SLOWEST_TEMPO}
        maximum={FASTEST_TEMPO}
      >
        <ClickArea onPress={tap_tempo}>
          <Label font="large" color="#b0b0b0" text_align="center">
            {format_number(beats_per_minute.value, 3)}
          </Label>
          <Label font="small" color="#b0b0b0" text_align="center">
            bpm
          </Label>
        </ClickArea>
      </Arc>
      <Slider
        width={220}
        value={beats_per_minute.value}
        minimum={SLOWEST_TEMPO}
        maximum={FASTEST_TEMPO}
        onChange={(event) => set_tempo(event.value)}
      />
      <Row gap={6} width={232}>
        <Button grow={1} height={50} onClick={next_bar_length}>
          {`${beats_per_bar.value} bpb`}
        </Button>
        <Button grow={1} height={50} background={is_running.value ? "surface" : "accent"} onClick={start_or_stop}>
          <Icon icon={is_running.value ? icons.pause : icons.play} />
        </Button>
      </Row>
    </Screen>
  );
}

export default define_app({
  id: "metronome",
  name: "Metronome",
  icon: icons.metronome,
  screens: { main: Metronome },
  subscriptions: [at(next_beat, beat)],
  onClose: stop,
});
