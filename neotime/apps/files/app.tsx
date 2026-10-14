// Files: the watch's storage on its external flash, a folder at a time. Each folder lists its
// folders first and then its files, each by name with its size; a folder's size is every file
// inside it added up. Tapping a folder opens it, and the side button goes back up a folder.

import { current_folder, define_app, format_fixed, go, on, open_folder, range, signal } from "neotime";
import { icons } from "neotime/icons";
import { Button, Column, Icon, Label, List, Row, Screen } from "neotime/widgets";

// ============================================================================
// Layout
// ============================================================================

/** As many entries as the watch lists in one folder. */
const MOST_ENTRIES = 64;

const ROW_HEIGHT = 52;

/** Space around the list and between its rows. */
const SPACING = 6;

const BYTES_IN_A_KILOBYTE = 1024;
const BYTES_IN_A_MEGABYTE = 1024 * 1024;

// ============================================================================
// State
// ============================================================================

/** How many folders below the top the open one is, which says which of the two screens shows it. */
const depth = signal(0);

/** Set while a tap opens a folder, so the screen it leaves does not go back up. */
const entering = signal(false);

// ============================================================================
// Text
// ============================================================================

/** A size in bytes, kilobytes or megabytes, the larger two with one decimal. */
export function size_text(bytes: number): string {
  if (bytes < BYTES_IN_A_KILOBYTE) return `${bytes} B`;
  if (bytes < BYTES_IN_A_MEGABYTE) return `${format_fixed(Math.trunc((bytes * 10) / BYTES_IN_A_KILOBYTE), 1)} KB`;
  return `${format_fixed(Math.trunc((bytes * 10) / BYTES_IN_A_MEGABYTE), 1)} MB`;
}

// ============================================================================
// Handlers
// ============================================================================

function open_top() {
  depth.value = 0;
  entering.value = false;
  open_folder("/");
}

/** Opens a folder of the list on the other screen, so the side button comes back to this one. */
export function enter(place: number) {
  if (!current_folder.value.entries[place].is_folder) return;
  entering.value = true;
  depth.value = depth.value + 1;
  open_folder(`${current_folder.value.path}/${current_folder.value.entries[place].name}`);
  go(depth.value % 2 === 1 ? "inside" : "main");
}

/** Goes back up a folder when the side button leaves a screen, but not when a tap opened the next folder. */
export function leave_folder() {
  if (entering.value) {
    entering.value = false;
    return;
  }
  depth.value = Math.max(depth.value - 1, 0);
  open_folder(`${current_folder.value.path}/..`);
}

on("open", open_top);

// ============================================================================
// Screens
// ============================================================================

function EntryRow(props: { place: number }) {
  return (
    <>
      {props.place < current_folder.value.entries.length && (
        <Button height={ROW_HEIGHT} background="background" onClick={() => enter(props.place)}>
          <Column gap={0}>
            <Label long_mode="clip">{current_folder.value.entries[props.place].name}</Label>
            <Label font="small" color="muted">
              {`${current_folder.value.entries[props.place].is_folder ? "Folder, " : ""}${size_text(current_folder.value.entries[props.place].size)}`}
            </Label>
          </Column>
        </Button>
      )}
    </>
  );
}

/** One folder's entries under its path; both screens show it, one for each step down. */
function FolderScreen() {
  return (
    <Screen padding={SPACING} gap={2} onHide={leave_folder}>
      <Row gap={8} height={36} padding={4}>
        <Icon icon={icons.note} size={28} color="accent" />
        <Label color="accent" long_mode="clip" grow={1}>
          {current_folder.value.path}
        </Label>
      </Row>
      <List grow={1} gap={SPACING}>
        {current_folder.value.entries.length === 0 && <Label color="muted">Empty</Label>}
        {range(MOST_ENTRIES).map((place) => (
          <EntryRow place={place} />
        ))}
      </List>
    </Screen>
  );
}

function EvenDepthFolder() {
  return <FolderScreen />;
}

function OddDepthFolder() {
  return <FolderScreen />;
}

export default define_app({
  id: "files",
  name: "Files",
  group: "system",
  icon: icons.note,
  screens: { main: EvenDepthFolder, inside: OddDepthFolder },
});
