// QR code: shows the code the companion app made, black on white and at full brightness so a scanner reads it.
// The phone writes the code's modules to /qr/code as a one-bit picture (see BitmapFile) and an optional
// caption to /qr/label, in plain ASCII.

import { computed, define_app, file_bytes, keep_screen_on, nth_index_of, on, set_brightness, signal, text_from_codes } from "neotime";
import { icons } from "neotime/icons";
import { BitmapFile, Column, Label, Screen } from "neotime/widgets";

// ============================================================================
// The files
// ============================================================================

const CODE_PATH = "/qr/code";
const LABEL_PATH = "/qr/label";
/** The longest caption the phone writes. */
const MOST_LABEL_BYTES = 24;
/** The largest code the watch draws, in modules on a side. */
const MOST_MODULES = 120;

/** The code's width in modules, the file's first byte, or 0 when there is no code. */
const code_header = signal([0]);

/** The caption's character codes, with zeros past its end. */
const label_codes = signal([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);

const has_code = computed(() => code_header.value[0] > 0 && code_header.value[0] <= MOST_MODULES);

const label_length = computed(() => {
  const end = nth_index_of(label_codes.value, 0, 0);
  return end < 0 ? MOST_LABEL_BYTES : end;
});

function read_files() {
  code_header.value = file_bytes(CODE_PATH, 1);
  label_codes.value = file_bytes(LABEL_PATH, MOST_LABEL_BYTES);
}

/** Scanners read a bright, steady screen best. */
function show_brightly() {
  read_files();
  set_brightness(3);
  keep_screen_on(true);
}

// ============================================================================
// Screen
// ============================================================================

function QrCode() {
  return (
    <Screen background={has_code.value ? "#ffffff" : "background"} padding={10} gap={4}>
      {has_code.value && <BitmapFile path="/qr/code" color="#000000" grow={1} />}
      {has_code.value && label_length.value > 0 && (
        <Label color="#000000" text_align="center" long_mode="clip">
          {text_from_codes(label_codes.value, label_length.value)}
        </Label>
      )}
      {!has_code.value && (
        <Column grow={1} justify="center">
          <Label color="muted" text_align="center">
            No QR code yet. Make one in the NeoTime companion app.
          </Label>
        </Column>
      )}
    </Screen>
  );
}

on("open", show_brightly);

export default define_app({
  id: "qr-code",
  name: "QR code",
  group: "phone",
  icon: icons.qr_code,
  screens: { main: QrCode },
});
