# Level

A spirit level: a bubble moves against the pull of gravity and turns green when the watch lies
level. Ported from wasp-os's level app.

## Using it

- Lay the watch on a surface. The bubble moves towards the higher side, and turns green within
  a few pixels of the centre.
- Tap anywhere to open the menu, which reminds you to calibrate on a flat surface:
  - **Calibrate** takes the current reading as level. Do this with the watch on a flat surface.
  - **Reset** forgets the calibration.
  - **Cancel** goes back without changing anything.
- The calibration is remembered. It needs the motion permission, and reads the accelerometer only
  while the app is open.

## Screens

![The level, lying level](../../neotime/harness/snapshots/screens/app-level.png)

Level: the green bubble in the middle of the cross.

![The level, tilted](../../neotime/harness/snapshots/screens/app-level-tilted.png)

Tilted, with the bubble off towards the lower left in white.

![The level's menu](../../neotime/harness/snapshots/screens/app-level-menu.png)

The menu, opened with a tap: **Calibrate**, **Reset** and **Cancel**.
