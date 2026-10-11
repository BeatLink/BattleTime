// The launcher: a scrolling list of the installed apps, each with its icon and name; a tap opens it.
// The system keeps the list in `installed_apps`, so this is an ordinary app that any other launcher
// can replace.
import { define_app, installed_apps, open_app, range } from "neotime";
import { icons } from "neotime/icons";
import { Button, Icon, Label, List, Row } from "neotime/widgets";

// ============================================================================
// Layout
// ============================================================================

/** The most apps the list has rows for; rows past the installed apps stay hidden. Each row has three bound
 * values, and an app may have 64. */
const MOST_APPS = 16;

const ROW_HEIGHT = 52;

/** Space around the list and between its rows, so nothing touches the edges or each other. */
const SPACING = 8;

// ============================================================================
// Screen
// ============================================================================

function AppRow(props: { place: number }) {
  return (
    <>
      {props.place < installed_apps.value.length && (
        <Button height={ROW_HEIGHT} onClick={() => open_app(installed_apps.value[props.place].id)}>
          <Row gap={12} align_items="center" padding={8}>
            <Icon icon={installed_apps.value[props.place].icon} size={32} />
            <Label grow={1} long_mode="scroll">{installed_apps.value[props.place].name}</Label>
          </Row>
        </Button>
      )}
    </>
  );
}

function Launcher() {
  return (
    <List padding={SPACING} gap={SPACING}>
      {range(MOST_APPS).map((place) => (
        <AppRow place={place} />
      ))}
    </List>
  );
}

export default define_app({
  id: "launcher",
  name: "Apps",
  icon: icons.pine_logo,
  screens: { main: Launcher },
});
