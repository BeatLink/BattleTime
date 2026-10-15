// Call: answers the phone's calls from the watch, after InfiniTime's and Bangle.js's call screens.
// The watch opens it when a call comes in and rings until the call is answered, rejected, muted or
// left. While it rings the screen says who is calling, with Accept, Reject and Mute; once answered
// it offers End call, and when the call ends it goes back to what was open before.

import { back, call_control, current_call, define_app, on, signal, type CallEvent } from "battletime";
import { icons } from "battletime/icons";
import { Button, Column, Icon, Label, Row, Screen } from "battletime/widgets";

// ============================================================================
// State
// ============================================================================

/** Whether the wearer muted the ringing call, which silences the phone and the watch but leaves the call. */
const is_muted = signal(false);

// ============================================================================
// Handlers
// ============================================================================

function follow_call(event: CallEvent) {
  if (event.kind === "incoming") is_muted.value = false;
  if (event.kind === "ended") back();
}

function accept() {
  call_control("answer");
}

/** Rejecting a ringing call and ending an answered one are the same command to the phone. */
function reject() {
  call_control("reject");
  back();
}

function mute() {
  is_muted.value = true;
  call_control("mute");
}

// ============================================================================
// Screens
// ============================================================================

function Ringing() {
  return (
    <>
      <Column grow={1} align_items="center" justify="center" gap={4}>
        <Row gap={6} align_items="center">
          <Icon icon={icons.phone} size={20} color="accent" />
          <Label font="small" color="muted">
            Incoming call from
          </Label>
        </Row>
        <Label font="medium" align="center" height={60}>
          {current_call.value.caller}
        </Label>
      </Column>
      <Button height={44} disabled={is_muted.value} onClick={mute}>
        {is_muted.value ? "Muted" : "Mute"}
      </Button>
      <Row gap={6} height={60}>
        <Button grow={1} background="danger" onClick={reject}>
          Reject
        </Button>
        <Button grow={1} background="success" onClick={accept}>
          Accept
        </Button>
      </Row>
    </>
  );
}

function Answered() {
  return (
    <>
      <Column grow={1} align_items="center" justify="center" gap={4}>
        <Label font="small" color="muted">
          On call with
        </Label>
        <Label font="medium" align="center" height={60}>
          {current_call.value.caller}
        </Label>
      </Column>
      <Button height={60} background="danger" onClick={reject}>
        End call
      </Button>
    </>
  );
}

function Call() {
  return (
    <Screen padding={8} gap={6}>
      {current_call.value.state === "incoming" && <Ringing />}
      {current_call.value.state === "answered" && <Answered />}
      {current_call.value.state === "ended" && (
        <Column grow={1} align_items="center" justify="center" gap={6}>
          <Icon icon={icons.phone} size={40} color="muted" />
          <Label color="muted">No call</Label>
        </Column>
      )}
    </Screen>
  );
}

on("call", follow_call, { while_open: true });

export default define_app({
  id: "call",
  name: "Call",
  group: "phone",
  icon: icons.phone,
  screens: { main: Call },
});
