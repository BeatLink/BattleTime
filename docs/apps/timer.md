# Timer

A countdown timer of up to 99 minutes, merged from wasp-os's timer and InfiniTime's Timer. It stores
when it ends, so it keeps running while the app is closed or the watch sleeps.

## Using it

- Roll the **minutes** and **seconds** to the length, then tap **play** to start.
- While it runs, tap **pause** to hold the time. While paused, tap **play** to carry on, or the red
  **stop** button to go back to the rollers.
- When the time runs out the watch wakes and buzzes, and shows how long ago it ended. Tap
  **Dismiss**, or anywhere on the screen, to silence it. The buzzing stops on its own after a minute.
- The rollers keep the last length chosen.

## Screens

![The timer's rollers](../../neotime/harness/snapshots/screens/app-timer.png)

Choosing a length, here ten minutes.

![The timer running](../../neotime/harness/snapshots/screens/app-timer-running.png)

Running, with the time left inside a ring that empties as it counts down, and the pause button.

![The timer paused](../../neotime/harness/snapshots/screens/app-timer-paused.png)

Paused: the time and the ring grey out, with stop and play buttons.

![The timer ringing](../../neotime/harness/snapshots/screens/app-timer-ringing.png)

Time's up, with the time since the end and Dismiss.
