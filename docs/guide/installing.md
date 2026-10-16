# Installing BattleTime

This page takes a PineTime from InfiniTime or wasp-os onto BattleTime, then sets it up with a face
and apps. It needs no cable or SWD probe: everything goes over Bluetooth, through the old system's
own update path.

!!! warning "Read this first"
    BattleTime is early work. Moving a watch from **wasp-os** has been done on a real PineTime;
    moving one from **InfiniTime** has so far run only in the emulator. A PineTime that cannot reach
    its bootloader cannot be brought back without an SWD probe, so read
    [the risk, honestly](../updates.md#the-risk-honestly) before you start.

## What you need

- A PineTime running **wasp-os** or **InfiniTime**, and its charger.
- The BattleTime Companion on a Linux computer or an Android phone; see
  [ways to run it](../companion/index.md#ways-to-run-it). On a computer with Nix:

    ```bash
    nix run github:BeatLink/BattleTime#companion-tauri
    ```

- From InfiniTime only: the app you already update InfiniTime with, such as Gadgetbridge, ITD or
  Siglo.
- The files from the newest [BattleTime release](https://github.com/BeatLink/BattleTime/releases):

| File | What it is for |
|---|---|
| `reloader-from-wasp-os.zip` | Moves a watch from wasp-os onto BattleTime |
| `reloader-from-infinitime.zip` | Moves a watch from InfiniTime onto BattleTime |
| `firmware.zip` | The full BattleTime firmware, sent once the watch is on BattleTime |
| `SHA256SUMS` | Checksums for every file, to check the downloads |

Check the downloads before sending them:

```bash
sha256sum --check --ignore-missing SHA256SUMS
```

## How it works

Moving over takes three steps, all over Bluetooth:

1. **The reloader**, sent as an ordinary update for the old system, replaces its bootloader with
   BattleTime's and leaves the watch running BattleTime's small recovery firmware. The full firmware
   does not fit beside the bootloader in the old system's update slot, so it comes next.
2. **The full firmware**, sent from the companion as an ordinary BattleTime update.
3. **Setting up**, where the companion installs a face, the launcher and the apps you pick. The
   firmware carries none of its own.

## 1. Send the reloader

Put the watch **on its charger** and leave it there until step 1 is finished. The reloader waits
for the charger and a battery of at least 3.7 V before it writes anything, and shows on the screen
what it is waiting for, such as "Battery 87% charging".

### From wasp-os

1. Open the companion, choose **Find a watch** on the Watch screen, and connect to the watch.
2. Open the **Firmware** tab, choose **Choose a package**, and pick `reloader-from-wasp-os.zip`.
3. Choose **Send to the watch** and keep the watch close. The companion restarts wasp-os into its
   bootloader and sends the reloader there; it takes about 4 minutes.

### From InfiniTime

1. In Gadgetbridge, ITD or Siglo, start a firmware update as you would for a new InfiniTime, and
   send `reloader-from-infinitime.zip`.
2. Keep the watch close until the upload finishes and the watch restarts into the reloader.

### While the reloader works

Leave the watch alone on the charger. The reloader checks its images, copies the recovery firmware
into the watch's storage, writes BattleTime's bootloader (about half a second, the only moment the
watch could be lost), and restarts. The watch then starts BattleTime's recovery firmware, whose status
page is **purple**.

If the reloader stops with a message, such as a damaged download, the old system is still in
place unless the message comes after the bootloader write. Holding the side button for a few
seconds restarts the watch: into the old system's update mode if the bootloader was not yet
written, or into BattleTime's bootloader if it was.

## 2. Send the full firmware

1. In the companion, choose **Find a watch** again. A watch running the recovery firmware shows
   as **(bootloader)**, since it can take only a firmware update.
2. Open the **Firmware** tab, choose **Choose a package**, pick `firmware.zip`, and choose
   **Send to the watch**. **Look for builds** finds the same file in the newest release for you.
3. The upload takes under 2 minutes. The watch restarts, its bootloader swaps the new firmware in,
   and the companion reconnects and sets the time, which confirms the firmware so the watch keeps
   it.

The watch now starts on a help screen that shows the time and asks for a face.
[Firmware updates](../companion/firmware-updates.md) has more on this step.

## 3. Set up the watch

When the companion connects to a watch with no face, the Watch screen says
**This watch has no face yet**.

1. Choose **Set up the watch**.
2. Tick the apps to install. The list starts with the digital face, the launcher and the apps most
   people keep; everything else waits in the store.
3. Choose **Install** and keep the watch close for a minute or two.

The face appears as soon as it lands. Swipe up on it for the launcher, and click the side button to
go back. [Getting around](getting-around.md) explains the rest, and
[Apps, faces and the store](../companion/apps-and-store.md) covers installing more.

## Using Gadgetbridge instead

Once BattleTime is installed, Gadgetbridge and the other InfiniTime companions can stand in for the
BattleTime Companion for notifications, music, weather and firmware updates. Turn on
**Show as InfiniTime** in the watch's settings so they recognise it; see
[Phone and Bluetooth](phone-and-bluetooth.md). Setting the watch up with apps still needs the
BattleTime Companion.

## If something goes wrong

- **A firmware update never confirms.** It runs on trial and the next restart puts the previous
  firmware back. Reconnect the companion, or choose **Confirm** in the About app.
- **The watch is stuck or will not start.** Hold the side button for about 10 seconds until it
  buzzes and shows the bootloader's menu, then pick **Roll back** or **Install recovery** and send
  `firmware.zip` again. [Updates and recovery](updates-and-recovery.md#the-side-button-menu)
  describes the menu.
- **The companion cannot find the watch.** Wake it with the button; it advertises quickly for 30
  seconds after it starts. On a computer, switching Bluetooth off and on clears a stuck link.
  [Connecting a watch](../companion/connecting.md) has more.

## Trying it without a watch

BattleTime also runs in PineTimeEmulator on a computer, as
[testing.md](../testing.md#trying-battletime-in-the-emulator) describes.
