// Music remote for the phone's player, merged from InfiniTime's Music and wasp-os's music_player.
// It has wasp-os's artist over a wrapped track title and InfiniTime's running position, swipe left
// and right to skip, and a second row of volume buttons that a swipe up brings in. It asks the phone
// for its player's state when it opens.

import { computed, useSignal } from "@preact/signals";
import {
  back,
  current_music,
  define_app,
  format_duration,
  music,
  music_control,
  now,
  persisted,
  request_music_state,
  HOUR,
  MINUTE,
  type Duration,
  type Instant,
  type MusicCommand,
} from "neotime";
import { icons } from "neotime/icons";
import { Bar, Button, Column, Icon, Label, Row, Screen } from "neotime/widgets";

// ============================================================================
// State
// ============================================================================

// The system keeps the artist, track and length in `current_music`. These follow the phone's reports
// so the button can change at once and a playing track's position can run on between reports.

const is_playing = persisted("is_playing", false);

/** How far into the track the phone last said it was. */
const position = persisted<Duration>("position", 0);

/** When `position` was true, so a playing track's position can run on from it. */
const position_at = persisted<Instant>("position_at", 0);

// ============================================================================
// Derived values
// ============================================================================

const current_position = computed(() =>
  is_playing.value
    ? Math.min(current_music.value.length, position.value + (now.value - position_at.value))
    : position.value,
);

/** Long tracks show hours and minutes, and anything past 99 hours shows as endless. */
const position_text = computed(() => {
  if (current_music.value.length === 0) return "--:--/--:--";
  if (current_music.value.length > 99 * HOUR) return "Inf/Inf";
  const pattern = current_music.value.length > 99 * MINUTE ? "h:mm" : "mm:ss";
  return `${format_duration(current_position.value, pattern)}/${format_duration(current_music.value.length, pattern)}`;
});

/** Whether the phone has reported a track; its text cannot be compared on the watch, so this uses the numbers. */
const has_track = computed(() => current_music.value.length > 0 || current_music.value.is_playing);

// ============================================================================
// Handlers
// ============================================================================

/** Each report from the phone resets the position the display runs on from. */
function receive() {
  is_playing.value = current_music.value.is_playing;
  position.value = current_music.value.position;
  position_at.value = now.value;
}

/** Both firmwares assume the phone obeys at once, so the button changes before it answers. */
function play_or_pause() {
  position.value = current_position.value;
  position_at.value = now.value;
  if (is_playing.value) {
    is_playing.value = false;
    music_control("pause");
  } else {
    is_playing.value = true;
    music_control("play");
  }
}

function send(command: MusicCommand) {
  music_control(command);
}

// ============================================================================
// Screen
// ============================================================================

function Music() {
  const shows_volume = useSignal(false);
  return (
    <Screen
      padding={8}
      gap={6}
      // This takes every swipe, so right skips back a track and down leaves once the volume row is hidden.
      onSwipe={(event) => {
        if (event.direction === "left") send("next");
        else if (event.direction === "right") send("previous");
        else if (event.direction === "up") shows_volume.value = true;
        else if (shows_volume.value) shows_volume.value = false;
        else back();
      }}
    >
      <Row justify="space_between">
        <Label font="small" color="muted">{position_text.value}</Label>
        <Icon icon={icons.music} size={20} color={is_playing.value ? "accent" : "muted"} />
      </Row>
      <Bar value={current_position.value} maximum={Math.max(1, current_music.value.length)} />
      <Column grow={1} justify="center" gap={2}>
        {has_track.value ? (
          <>
            <Label font="small" color="#b0b0b0" long_mode="scroll">{current_music.value.artist}</Label>
            <Label height={76}>{current_music.value.track}</Label>
          </>
        ) : (
          <Label color="muted" text_align="center">Nothing playing</Label>
        )}
      </Column>
      <Row gap={6} justify="space_between">
        <Button width={68} height={64} radius={20} onClick={() => send(shows_volume.value ? "volume_down" : "previous")}>
          <Icon icon={shows_volume.value ? icons.volume_down : icons.previous} />
        </Button>
        <Button width={68} height={64} radius={20} background="accent" onClick={play_or_pause}>
          <Icon icon={is_playing.value ? icons.pause : icons.play} />
        </Button>
        <Button width={68} height={64} radius={20} onClick={() => send(shows_volume.value ? "volume_up" : "next")}>
          <Icon icon={shows_volume.value ? icons.volume_up : icons.next} />
        </Button>
      </Row>
    </Screen>
  );
}

export default define_app({
  id: "music",
  name: "Music",
  group: "phone",
  icon: icons.music,
  screens: { main: Music },
  subscriptions: [music(receive, { only_while_open: true })],
  permissions: ["phone"],
  onOpen: request_music_state,
});
