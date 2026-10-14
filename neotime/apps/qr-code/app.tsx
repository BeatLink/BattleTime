// QR code: a list of codes the companion app made; tap one to show it black on white and at full brightness so a
// scanner reads it. The phone writes the list to /qr/list and each code to /qr/<slot> as a one-bit picture (see
// BitmapFile), and only the chosen code is read from storage.

import {
  computed,
  define_app,
  file_bytes,
  go,
  keep_screen_on,
  on,
  range,
  set_brightness,
  signal,
  slice_text,
  text_from_codes,
} from "neotime";
import { Header } from "neotime/components";
import { icons } from "neotime/icons";
import { BitmapFile, Button, Column, Label, List, Screen } from "neotime/widgets";

// ============================================================================
// The list file
// ============================================================================

const LIST_PATH = "/qr/list";
const LIST_FORMAT_VERSION = 1;
/** The most codes the phone keeps on the watch. */
const MOST_CODES = 8;
/** The version and the count come before the entries. */
const HEADER_BYTES = 2;
/** The longest caption the phone writes. */
const MOST_CAPTION_BYTES = 24;
/** Each entry is its code's slot, its caption's length, then the caption padded to full length. */
const ENTRY_BYTES = 2 + MOST_CAPTION_BYTES;
const LIST_BYTES = HEADER_BYTES + MOST_CODES * ENTRY_BYTES;

const EMPTY_LIST = range(LIST_BYTES).map(() => 0);

/** The list file as the phone wrote it, with zeros past its end or when it is missing. */
const list_file = signal(EMPTY_LIST);

/** How many codes to list; none for a missing file or one in another format. */
const code_count = computed(() =>
  list_file.value[0] === LIST_FORMAT_VERSION ? Math.min(list_file.value[1], MOST_CODES) : 0,
);

/** The whole list file as text, so each caption can be cut from it. */
const list_text = computed(() => text_from_codes(list_file.value, LIST_BYTES));

/** The slot of the code at a place in the list, which names its file. */
function slot_at(place: number): number {
  return list_file.value[HEADER_BYTES + place * ENTRY_BYTES];
}

/** The caption of the code at a place in the list. */
function caption_at(place: number): string {
  const start = HEADER_BYTES + place * ENTRY_BYTES + 2;
  const length = Math.min(list_file.value[start - 1], MOST_CAPTION_BYTES);
  return slice_text(list_text.value, start, start + length);
}

/** The place in the list of the code on show. */
const shown_place = signal(0);

function read_list() {
  list_file.value = file_bytes(LIST_PATH, LIST_BYTES);
}

function show_code(place: number) {
  shown_place.value = place;
  go("code");
}

/** Scanners read a bright, steady screen best. */
function open_brightly() {
  read_list();
  set_brightness(3);
  keep_screen_on(true);
}

// ============================================================================
// Screens
// ============================================================================

const ROW_HEIGHT = 48;
const SPACING = 6;

function CodeRow(props: { place: number }) {
  return (
    <>
      {props.place < code_count.value && (
        <Button height={ROW_HEIGHT} padding={12} background="surface" onClick={() => show_code(props.place)}>
          <Label long_mode="clip">{caption_at(props.place)}</Label>
        </Button>
      )}
    </>
  );
}

function CodeList() {
  return (
    <Screen padding={SPACING} gap={2}>
      <Header title="QR codes" icon={icons.qr_code} />
      {code_count.value > 0 && (
        <List grow={1} gap={SPACING}>
          {range(MOST_CODES).map((place) => (
            <CodeRow place={place} />
          ))}
        </List>
      )}
      {code_count.value === 0 && (
        <Column grow={1} justify="center">
          <Label color="muted" text_align="center">
            No QR codes yet. Make them in the NeoTime companion app.
          </Label>
        </Column>
      )}
    </Screen>
  );
}

function Code() {
  return (
    <Screen background="#ffffff" padding={10} gap={4}>
      <BitmapFile path="/qr/" value={slot_at(shown_place.value)} color="#000000" grow={1} />
      <Label color="#000000" text_align="center" long_mode="clip">
        {caption_at(shown_place.value)}
      </Label>
    </Screen>
  );
}

on("open", open_brightly);
on("screen_on", read_list);

export default define_app({
  id: "qr-code",
  name: "QR code",
  group: "phone",
  icon: icons.qr_code,
  screens: { main: CodeList, code: Code },
});
