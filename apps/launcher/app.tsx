// The launcher: a scrolling list of the installed apps, each with its icon and name; a tap opens it.
// The watch draws the list itself from `installed_apps`, so this is an ordinary app that any other
// launcher can replace.
import { define_app, installed_apps, open_app, type ChangeEvent } from "neotime";
import { icons } from "neotime/icons";
import { AppList } from "neotime/widgets";

// ============================================================================
// Screen
// ============================================================================

/** Space around the list and between its rows, so nothing touches the edges or each other. */
const SPACING = 8;

function open_tapped(event: ChangeEvent<number>) {
  open_app(installed_apps.value[event.value].id);
}

function Launcher() {
  return <AppList padding={SPACING} gap={SPACING} onClick={open_tapped} />;
}

export default define_app({
  id: "launcher",
  name: "Apps",
  icon: icons.pine_logo,
  screens: { main: Launcher },
});
