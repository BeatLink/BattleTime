// The launcher: the installed apps, grouped as the companion app set up, each with its icon and name; a tap opens one.
// The companion chooses the style: one plain list, folders that each open a group, one list with a heading per
// group, or a page per group swiped left and right; and the view: a row per app or a grid of icons.
// The watch draws the apps itself from `installed_apps`, so this is an ordinary app that any other launcher can replace.
import { computed, signal } from "@preact/signals";
import {
  define_app,
  go,
  installed_apps,
  launcher_groups,
  launcher_style,
  launcher_view,
  open_app,
  range,
  type ChangeEvent,
  type SwipeEvent,
} from "neotime";
import { icons } from "neotime/icons";
import { AppList, Button, Column, Icon, Label, List, Row, Screen } from "neotime/widgets";

// ============================================================================
// Groups
// ============================================================================

/** Space around the list and between its rows, so nothing touches the edges or each other. */
const SPACING = 8;

/** How many apps a row of the grid holds. */
const GRID_COLUMNS = 3;

/** The most groups the watch keeps, as the companion's launcher file allows. */
const MOST_GROUPS = 24;

const GROUP_ROW_HEIGHT = 52;

/** The folder open, or the page shown. */
const shown_group = signal(0);

const columns = computed(() => (launcher_view.value === "grid" ? GRID_COLUMNS : 1));

/** The shown group's place, kept inside the groups there are, which change as apps come and go. */
const group_place = computed(() => Math.min(shown_group.value, Math.max(0, launcher_groups.value.length - 1)));

const has_groups = computed(() => launcher_groups.value.length > 0);

function open_tapped(event: ChangeEvent<number>) {
  open_app(installed_apps.value[event.value].id);
}

function open_folder(place: number) {
  shown_group.value = place;
  go("group");
}

function turn_page(event: SwipeEvent) {
  if (launcher_style.value !== "pages" && launcher_style.value !== "folders") return;
  const last = launcher_groups.value.length - 1;
  if (event.direction === "left") shown_group.value = Math.min(group_place.value + 1, last);
  if (event.direction === "right") shown_group.value = Math.max(group_place.value - 1, 0);
}

// ============================================================================
// Screens
// ============================================================================

/** One folder's row: the group's icon, its name and how many apps it holds. */
function FolderRow(props: { place: number }) {
  return (
    <>
      {props.place < launcher_groups.value.length && (
        <Button height={GROUP_ROW_HEIGHT} onClick={() => open_folder(props.place)}>
          <Row gap={12} align_items="center" padding={8}>
            <Icon icon={launcher_groups.value[props.place].icon} size={32} />
            <Label grow={1} long_mode="clip">
              {launcher_groups.value[props.place].name}
            </Label>
            <Label color="muted">{`${launcher_groups.value[props.place].count}`}</Label>
          </Row>
        </Button>
      )}
    </>
  );
}

/** One group's apps under its name, with the page's number when the groups are pages. */
function GroupApps() {
  return (
    <>
      <Row height={28} justify="center" align_items="center" gap={6}>
        <Label color="accent">{has_groups.value ? launcher_groups.value[group_place.value].name : ""}</Label>
        {launcher_style.value === "pages" && (
          <Label color="muted" font="small">{`${group_place.value + 1}/${launcher_groups.value.length}`}</Label>
        )}
      </Row>
      <AppList
        grow={1}
        padding={SPACING}
        gap={SPACING}
        columns={columns.value}
        first={has_groups.value ? launcher_groups.value[group_place.value].first : 0}
        count={has_groups.value ? launcher_groups.value[group_place.value].count : 0}
        onClick={open_tapped}
      />
    </>
  );
}

/** An open folder; a swipe left or right moves to the next or the last group. */
function GroupPage() {
  return (
    <Screen onSwipe={turn_page}>
      <GroupApps />
    </Screen>
  );
}

function Launcher() {
  return (
    <Screen onSwipe={turn_page}>
      {launcher_style.value === "folders" && (
        <List grow={1} padding={SPACING} gap={SPACING}>
          {range(MOST_GROUPS).map((place) => (
            <FolderRow place={place} />
          ))}
        </List>
      )}
      {launcher_style.value === "pages" && <GroupApps />}
      {(launcher_style.value === "list" || launcher_style.value === "sections") && (
        <AppList
          grow={1}
          padding={SPACING}
          gap={SPACING}
          columns={columns.value}
          sections={launcher_style.value === "sections"}
          onClick={open_tapped}
        />
      )}
      {installed_apps.value.length === 0 && (
        <Column grow={1} align_items="center" justify="center">
          <Label color="muted">No apps</Label>
        </Column>
      )}
    </Screen>
  );
}

export default define_app({
  id: "launcher",
  name: "Apps",
  icon: icons.pine_logo,
  group: "system",
  screens: { main: Launcher, group: GroupPage },
});
