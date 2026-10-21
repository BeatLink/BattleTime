// Survival guides: what to do in an emergency, such as CPR, severe bleeding or being lost in the wild, read on the
// watch without a phone or a signal. The companion app writes the guides the wearer picked, each as text at
// /survival/<slot>, and lists them with their topics in /survival/list; only the guide on show is read from storage.
// The app opens on the topics, a topic lists its guides, and a guide shows its text.

import { computed, define_app, file_bytes, go, on, range, signal, slice_text, text_from_codes } from "battletime";
import { Header } from "battletime/components";
import { icons } from "battletime/icons";
import { Button, Column, Label, List, Screen, TextFile } from "battletime/widgets";

// ============================================================================
// The list file
// ============================================================================

const LIST_PATH = "/survival/list";
const LIST_FORMAT_VERSION = 2;
/** The most guides the phone keeps on the watch, one screen each. */
const MOST_GUIDES = 24;
/** The version and the count come before the entries. */
const HEADER_BYTES = 2;
/** The longest title the phone writes. */
const MOST_TITLE_BYTES = 23;
/** Each entry is its guide's slot, its topic, its title's length, then the title padded to full length. */
const ENTRY_BYTES = 3 + MOST_TITLE_BYTES;
/** Text is cut from at most 256 bytes at once, so the entries are read in three parts of eight. */
const PART_GUIDES = 8;
const PART_BYTES = PART_GUIDES * ENTRY_BYTES;

/** The topics, numbered as the phone writes them. */
const TOPIC_NAMES = ["First aid", "Wilderness", "Disasters", "Personal safety"];
const TOPIC_COUNT = 4;

const EMPTY_HEADER = range(HEADER_BYTES).map(() => 0);
const EMPTY_PART = range(PART_BYTES).map(() => 0);

/** The list file as the phone wrote it, with zeros past its end or when it is missing. */
const header = signal(EMPTY_HEADER);
const first_part = signal(EMPTY_PART);
const second_part = signal(EMPTY_PART);
const third_part = signal(EMPTY_PART);

/** How many guides to list; none for a missing file or one in another format. */
const guide_count = computed(() =>
  header.value[0] === LIST_FORMAT_VERSION ? Math.min(header.value[1], MOST_GUIDES) : 0,
);

/** Each part of the entries as text, so each title can be cut from it. */
const first_text = computed(() => text_from_codes(first_part.value, PART_BYTES));
const second_text = computed(() => text_from_codes(second_part.value, PART_BYTES));
const third_text = computed(() => text_from_codes(third_part.value, PART_BYTES));

/** Where the entry at a place in the list starts within its part. */
function start_in_part(place: number): number {
  return (place % PART_GUIDES) * ENTRY_BYTES;
}

/** A byte of the entry at a place in the list. */
function entry_byte(place: number, offset: number): number {
  const index = start_in_part(place) + offset;
  if (place < PART_GUIDES) {
    return first_part.value[index];
  }
  return place < 2 * PART_GUIDES ? second_part.value[index] : third_part.value[index];
}

/** The slot of the guide at a place in the list, which names its file and its screen. */
function slot_at(place: number): number {
  return entry_byte(place, 0);
}

function topic_at(place: number): number {
  return entry_byte(place, 1);
}

/** The title of the guide at a place in the list. */
function title_at(place: number): string {
  const start = start_in_part(place) + 3;
  const length = Math.min(entry_byte(place, 2), MOST_TITLE_BYTES);
  if (place < PART_GUIDES) {
    return slice_text(first_text.value, start, start + length);
  }
  return slice_text(place < 2 * PART_GUIDES ? second_text.value : third_text.value, start, start + length);
}

/** How many of the listed guides are on a topic. */
function guides_on(topic: number): number {
  let count = 0;
  for (const place of range(MOST_GUIDES)) {
    if (place < guide_count.value && topic_at(place) === topic) {
      count += 1;
    }
  }
  return count;
}

/** The topic whose guides are listed, and the place in the list of the guide on show. */
const shown_topic = signal(0);
const shown_place = signal(0);

/** A handler may change at most 1 KB of state, so the list is read in two handlers. */
function read_list() {
  header.value = file_bytes(LIST_PATH, HEADER_BYTES);
  first_part.value = file_bytes(LIST_PATH, PART_BYTES, HEADER_BYTES);
  second_part.value = file_bytes(LIST_PATH, PART_BYTES, HEADER_BYTES + PART_BYTES);
}

function read_list_end() {
  third_part.value = file_bytes(LIST_PATH, PART_BYTES, HEADER_BYTES + 2 * PART_BYTES);
}

// ============================================================================
// Moving between screens
// ============================================================================

/** A TextFile reads one fixed path, so each slot has a screen of its own. */
const GUIDE_SCREENS = [
  "guide_0",
  "guide_1",
  "guide_2",
  "guide_3",
  "guide_4",
  "guide_5",
  "guide_6",
  "guide_7",
  "guide_8",
  "guide_9",
  "guide_10",
  "guide_11",
  "guide_12",
  "guide_13",
  "guide_14",
  "guide_15",
  "guide_16",
  "guide_17",
  "guide_18",
  "guide_19",
  "guide_20",
  "guide_21",
  "guide_22",
  "guide_23",
];

function show_topic(topic: number) {
  shown_topic.value = topic;
  go("topic");
}

function show_guide(place: number) {
  shown_place.value = place;
  go(GUIDE_SCREENS[slot_at(place)]);
}

// ============================================================================
// Screens
// ============================================================================

const ROW_HEIGHT = 48;
const SPACING = 6;

function TopicRow(props: { topic: number }) {
  return (
    <>
      {guides_on(props.topic) > 0 && (
        <Button height={ROW_HEIGHT} padding={12} background="surface" onClick={() => show_topic(props.topic)}>
          <Label long_mode="clip">{TOPIC_NAMES[props.topic]}</Label>
        </Button>
      )}
    </>
  );
}

function TopicList() {
  return (
    <Screen padding={SPACING} gap={2}>
      <Header title="Survival" icon={icons.flag} />
      {guide_count.value > 0 && (
        <List grow={1} gap={SPACING}>
          {range(TOPIC_COUNT).map((topic) => (
            <TopicRow topic={topic} />
          ))}
        </List>
      )}
      {guide_count.value === 0 && (
        <Column grow={1} justify="center">
          <Label color="muted" text_align="center">
            No guides yet. Choose them in the BattleTime companion app.
          </Label>
        </Column>
      )}
    </Screen>
  );
}

function GuideRow(props: { place: number }) {
  return (
    <>
      {props.place < guide_count.value && topic_at(props.place) === shown_topic.value && (
        <Button height={ROW_HEIGHT} padding={12} background="surface" onClick={() => show_guide(props.place)}>
          <Label long_mode="clip">{title_at(props.place)}</Label>
        </Button>
      )}
    </>
  );
}

/** The guides on the topic chosen. */
function GuideList() {
  return (
    <Screen padding={SPACING} gap={2}>
      <Label color="accent" long_mode="clip" height={30}>
        {TOPIC_NAMES[shown_topic.value]}
      </Label>
      <List grow={1} gap={SPACING}>
        {range(MOST_GUIDES).map((place) => (
          <GuideRow place={place} />
        ))}
      </List>
    </Screen>
  );
}

/** One guide: its title, then its text scrolled by dragging. */
function Guide(props: { path: string }) {
  return (
    <Screen padding={SPACING} gap={2}>
      <Label color="accent" long_mode="clip" height={30}>
        {title_at(shown_place.value)}
      </Label>
      <TextFile path={props.path} placeholder="This guide is missing. Send it again from the companion app." grow={1} />
    </Screen>
  );
}

const Guide0 = () => <Guide path="/survival/0" />;
const Guide1 = () => <Guide path="/survival/1" />;
const Guide2 = () => <Guide path="/survival/2" />;
const Guide3 = () => <Guide path="/survival/3" />;
const Guide4 = () => <Guide path="/survival/4" />;
const Guide5 = () => <Guide path="/survival/5" />;
const Guide6 = () => <Guide path="/survival/6" />;
const Guide7 = () => <Guide path="/survival/7" />;
const Guide8 = () => <Guide path="/survival/8" />;
const Guide9 = () => <Guide path="/survival/9" />;
const Guide10 = () => <Guide path="/survival/10" />;
const Guide11 = () => <Guide path="/survival/11" />;
const Guide12 = () => <Guide path="/survival/12" />;
const Guide13 = () => <Guide path="/survival/13" />;
const Guide14 = () => <Guide path="/survival/14" />;
const Guide15 = () => <Guide path="/survival/15" />;
const Guide16 = () => <Guide path="/survival/16" />;
const Guide17 = () => <Guide path="/survival/17" />;
const Guide18 = () => <Guide path="/survival/18" />;
const Guide19 = () => <Guide path="/survival/19" />;
const Guide20 = () => <Guide path="/survival/20" />;
const Guide21 = () => <Guide path="/survival/21" />;
const Guide22 = () => <Guide path="/survival/22" />;
const Guide23 = () => <Guide path="/survival/23" />;

on("open", read_list);
on("open", read_list_end);
on("screen_on", read_list);
on("screen_on", read_list_end);

export default define_app({
  id: "survival",
  name: "Survival",
  group: "tools",
  icon: icons.flag,
  screens: {
    main: TopicList,
    topic: GuideList,
    guide_0: Guide0,
    guide_1: Guide1,
    guide_2: Guide2,
    guide_3: Guide3,
    guide_4: Guide4,
    guide_5: Guide5,
    guide_6: Guide6,
    guide_7: Guide7,
    guide_8: Guide8,
    guide_9: Guide9,
    guide_10: Guide10,
    guide_11: Guide11,
    guide_12: Guide12,
    guide_13: Guide13,
    guide_14: Guide14,
    guide_15: Guide15,
    guide_16: Guide16,
    guide_17: Guide17,
    guide_18: Guide18,
    guide_19: Guide19,
    guide_20: Guide20,
    guide_21: Guide21,
    guide_22: Guide22,
    guide_23: Guide23,
  },
});
