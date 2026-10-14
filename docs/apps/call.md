# Call

Shows who is calling when the phone rings, and answers, rejects or mutes the call from the watch.
After InfiniTime's and Bangle.js's call screens.

## Using it

- When a call comes in, the watch lights up on this app over whatever was open and rings until you
  act, the call ends or two minutes pass. Do not disturb keeps it dark and still. Notifications that
  arrive while it rings are kept without a buzz or a preview.
- The screen says **Incoming call from** and the caller's name, or their number when the phone has
  no name for them.
- Tap **Accept** to answer, or **Reject** to turn the call down and go back to what was open.
- Tap **Mute** to silence the phone and the watch and let the call ring out; you can still accept
  or reject it.
- Once the call is answered the screen shows **On call with** the caller and an **End call** button.
- When the call ends, on the phone or the watch, the app goes back to what was open before.
- Pressing the button leaves the app and stops the watch ringing; the phone still rings.
- Opened from the launcher with no call, the app says **No call**.
- A Bluetooth tool can ring it too, by writing a New Alert in the incoming call category, as for
  InfiniTime. No answer goes back to that tool, and the screen stays until you reject or leave it.

## Screens

![The call app while a call rings](../../neotime/harness/snapshots/screens/app-call-ringing.png)

A call ringing, with the caller's name, **Mute**, **Reject** and **Accept**.

![The call app with the call muted](../../neotime/harness/snapshots/screens/app-call-muted.png)

Muted: the button reads **Muted** and the call rings on silently until it is answered, rejected or
ends.

![The call app during a call](../../neotime/harness/snapshots/screens/app-call-answered.png)

The call answered, with **End call**.

![The call app with no call](../../neotime/harness/snapshots/screens/app-call.png)

Opened from the launcher with no call.

## With the companion app

- **Accept**, **Reject**, **Mute** and **End call** go to the phone through the companion app.
  Answering needs the NeoTime companion app on Android 8 or later, or Gadgetbridge; rejecting
  needs Android 9.
- When an incoming call ends, the companion app leaves a notification on the watch saying whether
  it was missed, ignored on the watch or accepted, in place of the phone's own missed call
  notification.
