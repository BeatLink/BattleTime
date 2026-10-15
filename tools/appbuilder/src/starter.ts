// The source a new app starts from: one screen with a label, ready for the palette to add to.

/** An app id is lower case words joined by hyphens, as the folders in `apps/` are named. */
export function is_app_id(text: string): boolean {
  return /^[a-z][a-z0-9]*(-[a-z0-9]+)*$/.test(text);
}

/** `four-in-a-row` becomes `Four in a row`. */
export function name_from_id(id: string): string {
  const words = id.split("-").join(" ");
  return words.charAt(0).toUpperCase() + words.slice(1);
}

/** `four-in-a-row` becomes `FourInARow`, for the screen's component. */
function component_from_id(id: string): string {
  return id
    .split("-")
    .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
    .join("");
}

export function starter_app(id: string): string {
  const name = name_from_id(id);
  const component = component_from_id(id);
  return `// ${name}: made with the BattleTime App Builder.

import { define_app } from "battletime";
import { icons } from "battletime/icons";
import { Label, Screen } from "battletime/widgets";

// ============================================================================
// Screen
// ============================================================================

function ${component}() {
  return (
    <Screen padding={8} gap={6}>
      <Label text_align="center">${name}</Label>
    </Screen>
  );
}

export default define_app({
  id: "${id}",
  name: "${name}",
  group: "tools",
  icon: icons.check,
  screens: { main: ${component} },
});
`;
}
