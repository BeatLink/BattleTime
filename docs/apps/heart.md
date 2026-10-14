# Heart

A heart rate monitor, merged from InfiniTime's HeartRate and wasp-os's heart app. It starts
measuring as soon as it opens.

## Using it

- Wear the watch snugly and keep still. The large number is the latest reading in beats per
  minute, or **---** until there is one.
- The line below says what the sensor is doing: starting, no touch, waiting for enough data, or
  measuring.
- Once readings come in, a chart of the latest ones appears above the button.
- Tap **Stop** to stop measuring, and **Start** to begin again. The screen stays on while it
  measures, and measuring stops when the app closes.

## Screens

![The heart rate app starting](../../neotime/harness/snapshots/screens/app-heart.png)

Just opened, while the sensor starts.

![The heart rate app with no touch](../../neotime/harness/snapshots/screens/app-heart-no-touch.png)

The sensor does not feel skin.

![The heart rate app settling](../../neotime/harness/snapshots/screens/app-heart-settling.png)

The sensor needs more data before its first reading.

![The heart rate app measuring](../../neotime/harness/snapshots/screens/app-heart-measuring.png)

Measuring: the latest reading, with a chart of the readings so far.

![The heart rate app stopped](../../neotime/harness/snapshots/screens/app-heart-stopped.png)

Stopped, after tapping Stop.
