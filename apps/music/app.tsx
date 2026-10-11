// Music remote for the phone's player, merged from InfiniTime's Music and wasp-os's music_player.
// It has wasp-os's artist over a wrapped track title and InfiniTime's running position, swipe left
// and right to skip, and a second row of volume buttons that a swipe up brings in.

import { computed, useSignal } from "@preact/signals";
import {
  back,
  define_app,
  format_duration,
  music,
  music_control,
  now,
  persisted,
  HOUR,
  MINUTE,
  type Duration,
  type Instant,
  type MusicCommand,
  type MusicEvent,
} from "neotime";
import { icons } from "neotime/icons";
import { Bar, Button, Column, Icon, Label, Row, Screen } from "neotime/widgets";

// ============================================================================
// State
// ============================================================================

const is_playing = persisted("is_playing", false);

const artist = persisted("artist", "");

const track = persisted("track", "");

/** How far into the track the phone last said it was. */
const position = persisted<Duration>("position", 0);

/** When `position` was true, so a playing track's position can run on from it. */
const position_at = persisted<Instant>("position_at", 0);

const track_length = persisted<Duration>("track_length", 0);

// ============================================================================
// Derived values
// ============================================================================

const current_position = computed(() =>
  is_playing.value
    ? Math.min(track_length.value, position.value + (now.value - position_at.value))
    : position.value,
);

/** Long tracks show hours and minutes, and anything past 99 hours shows as endless. */
const position_text = computed(() => {
  if (track_length.value === 0) return "--:--/--:--";
  if (track_length.value > 99 * HOUR) return "Inf/Inf";
  const pattern = track_length.value > 99 * MINUTE ? "h:mm" : "mm:ss";
  return `${format_duration(current_position.value, pattern)}/${format_duration(track_length.value, pattern)}`;
});

const has_track = computed(() => track.value !== "" || artist.value !== "");

// ============================================================================
// Handlers
// ============================================================================

function receive(event: MusicEvent) {
  is_playing.value = event.is_playing;
  artist.value = event.artist;
  track.value = event.track;
  position.value = event.position;
  position_at.value = now.value;
  track_length.value = event.length;
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
      padding={6}
      gap={4}
      onSwipe={(event) => {
        if (event.direction === "left") send("next");
        else if (event.direction === "right") send("previous");
        else if (event.direction === "up") shows_volume.value = true;
        else if (shows_volume.value) shows_volume.value = false;
        else back();
      }}
    >
      <Row justify="between">
        <Label font="small" color="muted">{position_text.value}</Label>
        <Icon icon={icons.music} size={20} color={is_playing.value ? "accent" : "muted"} />
      </Row>
      <Bar value={current_position.value} maximum={Math.max(1, track_length.value)} />
      <Column grow={1} justify="center" gap={2}>
        {has_track.value ? (
          <>
            <Label font="small" color="#b0b0b0" long_mode="scroll">{artist.value}</Label>
            <Label height={92}>{track.value}</Label>
          </>
        ) : (
          <Label color="muted" text_align="center">Nothing playing</Label>
        )}
      </Column>
      <Row gap={6} justify="between">
        <Button width={72} height={72} radius={20} onClick={() => send(shows_volume.value ? "volume_down" : "previous")}>
          <Icon icon={shows_volume.value ? icons.volume_down : icons.previous} />
        </Button>
        <Button width={72} height={72} radius={20} background="accent" onClick={play_or_pause}>
          <Icon icon={is_playing.value ? icons.pause : icons.play} />
        </Button>
        <Button width={72} height={72} radius={20} onClick={() => send(shows_volume.value ? "volume_up" : "next")}>
          <Icon icon={shows_volume.value ? icons.volume_up : icons.next} />
        </Button>
      </Row>
    </Screen>
  );
}

export default define_app({
  id: "music",
  name: "Music",
  icon: icons.music,
  screens: { main: Music },
  subscriptions: [music(receive)],
  permissions: ["phone", "background"],
});
