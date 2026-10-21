// Survival guides: what to do in an emergency, such as CPR, severe bleeding or being lost in the wild, read on the
// watch without a phone or a signal. The companion app writes the guides the wearer picked, each as text at
// /survival/<slot>, and lists them in /survival/list; only the guide on show is read from storage.

import { computed, define_app, file_bytes, go, on, range, signal, slice_text, text_from_codes } from "battletime";
import { Header } from "battletime/components";
import { icons } from "battletime/icons";
import { Button, Column, Label, List, Screen, TextFile } from "battletime/widgets";

// ============================================================================
// The list file
// ============================================================================

const LIST_PATH = "/survival/list";
const LIST_FORMAT_VERSION = 1;
/** The most guides the phone keeps on the watch, one screen each. */
const MOST_GUIDES = 16;
/** The version and the count come before the entries. */
const HEADER_BYTES = 2;
/** The longest title the phone writes. */
const MOST_TITLE_BYTES = 24;
/** Each entry is its guide's slot, its title's length, then the title padded to full length. */
const ENTRY_BYTES = 2 + MOST_TITLE_BYTES;
/** Text is cut from at most 256 bytes at once, so the entries are read in two halves of eight. */
const HALF_GUIDES = MOST_GUIDES / 2;
const HALF_BYTES = HALF_GUIDES * ENTRY_BYTES;

const EMPTY_HEADER = range(HEADER_BYTES).map(() => 0);
const EMPTY_HALF = range(HALF_BYTES).map(() => 0);

/** The list file as the phone wrote it, with zeros past its end or when it is missing. */
const header = signal(EMPTY_HEADER);
const first_half = signal(EMPTY_HALF);
const second_half = signal(EMPTY_HALF);

/** How many guides to list; none for a missing file or one in another format. */
const guide_count = computed(() =>
  header.value[0] === LIST_FORMAT_VERSION ? Math.min(header.value[1], MOST_GUIDES) : 0,
);

/** Each half of the entries as text, so each title can be cut from it. */
const first_text = computed(() => text_from_codes(first_half.value, HALF_BYTES));
const second_text = computed(() => text_from_codes(second_half.value, HALF_BYTES));

/** The slot of the guide at a place in the list, which names its file and its screen. */
function slot_at(place: number): number {
  return place < HALF_GUIDES
    ? first_half.value[place * ENTRY_BYTES]
    : second_half.value[(place - HALF_GUIDES) * ENTRY_BYTES];
}

/** The title of the guide at a place in the list. */
function title_at(place: number): string {
  const in_first = place < HALF_GUIDES;
  const start = (in_first ? place : place - HALF_GUIDES) * ENTRY_BYTES + 2;
  const length = Math.min(in_first ? first_half.value[start - 1] : second_half.value[start - 1], MOST_TITLE_BYTES);
  return slice_text(in_first ? first_text.value : second_text.value, start, start + length);
}

/** The place in the list of the guide on show. */
const shown_place = signal(0);

function read_list() {
  header.value = file_bytes(LIST_PATH, HEADER_BYTES);
  first_half.value = file_bytes(LIST_PATH, HALF_BYTES, HEADER_BYTES);
  second_half.value = file_bytes(LIST_PATH, HALF_BYTES, HEADER_BYTES + HALF_BYTES);
}

// ============================================================================
// Opening a guide
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
];

function show_guide(place: number) {
  shown_place.value = place;
  go(GUIDE_SCREENS[slot_at(place)]);
}

// ============================================================================
// Screens
// ============================================================================

const ROW_HEIGHT = 48;
const SPACING = 6;

function GuideRow(props: { place: number }) {
  return (
    <>
      {props.place < guide_count.value && (
        <Button height={ROW_HEIGHT} padding={12} background="surface" onClick={() => show_guide(props.place)}>
          <Label long_mode="clip">{title_at(props.place)}</Label>
        </Button>
      )}
    </>
  );
}

function GuideList() {
  return (
    <Screen padding={SPACING} gap={2}>
      <Header title="Survival" icon={icons.flag} />
      {guide_count.value > 0 && (
        <List grow={1} gap={SPACING}>
          {range(MOST_GUIDES).map((place) => (
            <GuideRow place={place} />
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

on("open", read_list);
on("screen_on", read_list);

export default define_app({
  id: "survival",
  name: "Survival",
  group: "tools",
  icon: icons.flag,
  screens: {
    main: GuideList,
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
  },
});
