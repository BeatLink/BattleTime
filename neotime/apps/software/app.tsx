// Software, NeoTime's own app manager, after the app list of InfiniTime's settings and wasp-os's
// Software app: every installed app and watch face with a switch to turn it on or off, and the
// memory it needs while open. The watch checks each change first, so an app that would not fit,
// the launcher and the last face cannot be switched, and a tap on one says why.

import { app_memory_budget, back, define_app, go, packages, range, set_app_enabled, signal } from "neotime";
import { Header } from "neotime/components";
import { icons } from "neotime/icons";
import { Button, Column, Label, List, Row, Screen, Switch } from "neotime/widgets";

// ============================================================================
// Layout
// ============================================================================

/** More than a watch has room to install. */
const MOST_PACKAGES = 48;

const ROW_HEIGHT = 52;

/** Space around the list and between its rows. */
const SPACING = 6;

const BYTES_IN_A_KILOBYTE = 1024;

// ============================================================================
// Refusals, as `packages` numbers them
// ============================================================================

const NOT_REFUSED = 0;
const TOO_LARGE = 1;
const LAUNCHER_WOULD_NOT_FIT = 2;
const LAUNCHER_STAYS_ON = 3;
const LAST_FACE_STAYS_ON = 4;
const IS_OPEN = 5;

// ============================================================================
// State
// ============================================================================

/** The place in the list of the package whose switch was refused. */
const refused = signal(0);

// ============================================================================
// Text
// ============================================================================

/** Bytes as whole kilobytes, rounded up, so a need is never shown smaller than it is. */
export function kilobytes(bytes: number): string {
  return `${Math.trunc((bytes + BYTES_IN_A_KILOBYTE - 1) / BYTES_IN_A_KILOBYTE)} KB`;
}

/** Why a switch was refused, in a few words. */
export function refusal_text(refusal: number): string {
  if (refusal === TOO_LARGE) return "Needs more memory than an app can have";
  if (refusal === LAUNCHER_WOULD_NOT_FIT) return "The launcher would run out of memory";
  if (refusal === LAUNCHER_STAYS_ON) return "The launcher stays on";
  if (refusal === LAST_FACE_STAYS_ON) return "The last watch face stays on";
  if (refusal === IS_OPEN) return "It is open now";
  return "";
}

// ============================================================================
// Handlers
// ============================================================================

export function toggle(place: number) {
  if (packages.value[place].refusal !== NOT_REFUSED) {
    refused.value = place;
    go("refused");
    return;
  }
  set_app_enabled(packages.value[place].id, !packages.value[place].enabled);
}

// ============================================================================
// Screens
// ============================================================================

function PackageRow(props: { place: number }) {
  return (
    <>
      {props.place < packages.value.length && (
        <Button height={ROW_HEIGHT} background="background" onClick={() => toggle(props.place)}>
          <Row gap={8} align_items="center">
            <Column grow={1} gap={0}>
              <Label long_mode="clip">{packages.value[props.place].name}</Label>
              <Label font="small" color="muted">
                {`${packages.value[props.place].is_face ? "Face, " : ""}${kilobytes(packages.value[props.place].memory)}`}
              </Label>
            </Column>
            <Switch value={packages.value[props.place].enabled} />
          </Row>
        </Button>
      )}
    </>
  );
}

function PackageList() {
  return (
    <Screen padding={SPACING} gap={2}>
      <Header title="Installed" icon={icons.chart} />
      <List grow={1} gap={SPACING}>
        <Label font="small" color="muted">{`Each app may use ${kilobytes(app_memory_budget.value)}`}</Label>
        {range(MOST_PACKAGES).map((place) => (
          <PackageRow place={place} />
        ))}
      </List>
    </Screen>
  );
}

/** Why the last switch tapped did not change, with the memory when that is the reason. */
function Refused() {
  return (
    <Screen padding={12} gap={10} justify="center">
      {refused.value < packages.value.length && (
        <Column gap={10} align_items="center">
          <Label font="large" long_mode="clip">{packages.value[refused.value].name}</Label>
          <Label text_align="center">{refusal_text(packages.value[refused.value].refusal)}</Label>
          {packages.value[refused.value].refusal === TOO_LARGE && (
            <Label font="small" color="muted">
              {`Needs ${kilobytes(packages.value[refused.value].memory)} of ${kilobytes(app_memory_budget.value)}`}
            </Label>
          )}
        </Column>
      )}
      <Button height={44} onClick={back}>
        <Label text_align="center">OK</Label>
      </Button>
    </Screen>
  );
}

export default define_app({
  id: "software",
  name: "Software",
  group: "system",
  icon: icons.chart,
  screens: { main: PackageList, refused: Refused },
});
