# Finder

Makes the connected phone ring so you can find it, and stops it again. Ported from wasp-os's
phone_finder.

## Using it

- Tap **Ring** to make the phone ring. A message says **Ringing your phone** and the button turns
  red and reads **Stop**.
- Tap **Stop** to stop the ringing. Leaving the app stops it too.
- Without a phone connected, the card says **Phone not connected** and the button is dimmed and
  cannot be tapped.

## Screens

![The phone finder ready to ring](../../neotime/harness/snapshots/screens/app-phone-finder.png)

Ready, with the phone connected.

![The phone finder while the phone rings](../../neotime/harness/snapshots/screens/app-phone-finder-ringing.png)

Ringing, after tapping **Ring**; the button now reads **Stop**.

![The phone finder with no phone connected](../../neotime/harness/snapshots/screens/app-phone-finder-not-connected.png)

No phone connected, with the button dimmed.

## With the companion app

The watch asks the phone to start or stop ringing with Gadgetbridge's find-phone message, which
Gadgetbridge answers by ringing. The NeoTime companion app shows **The watch is looking for this
phone** on its main screen while the watch asks.
