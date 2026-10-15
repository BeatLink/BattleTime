# Firmware updates

The companion sends the watch new firmware over Bluetooth. BattleTime takes the update while it runs,
restarts into it, and keeps it once the companion reconnects. [Firmware updates](../updates.md)
describes what happens inside the watch.

## Sending an update

1. Connect the watch and open the **Firmware** tab.
2. Choose the firmware:
    - **Choose a package** picks a firmware zip from the phone or computer, such as `firmware.zip`
      from a BattleTime release; or
    - **Look for builds** lists the releases of the repository set in **Settings**, and recent
      Actions builds when a GitHub token is set there. Choose one to download it.
3. Choose **Send to the watch** and keep the watch close. The screen shows the progress, and the
   watch shows its own status page while the image arrives.
4. The watch restarts, its bootloader swaps the new firmware in, and the companion reconnects and
   sets the time. That exchange tells the watch the new firmware works, so it keeps it.

If the link drops partway through, the companion reconnects and carries on from where the watch got
to, up to four more times. A package the watch refuses is not sent again.

Only a package holding one application image is accepted; one carrying a SoftDevice or bootloader
is refused.

### Settings for downloads

Under **Firmware downloads** in **Settings**:

- **Repository**: where builds are listed from, `BeatLink/BattleTime` unless changed;
- **Board**: the watch the builds are for, such as `pinetime`;
- **GitHub token**: needed only for Actions builds. A token with no scopes is enough, and it is kept
  on the device.

## Keeping or rolling back an update

A new firmware runs on trial until a phone or computer exchanges data with it, which the companion
does as soon as it reconnects. Until then the watch's status page says **On trial**, and the next
restart would put the previous firmware back. The System app's first page also offers to confirm
or roll back a firmware still on trial.

If an update leaves the watch unable to start, the bootloader rolls back by itself. Holding the
side button for a long time brings up the bootloader's recovery firmware; see
[Bootloader](../bootloader.md).

## A watch waiting in its bootloader

A watch that shows its bootloader, or the recovery firmware, appears in the scan list marked
**(bootloader)**, or connects as waiting in its bootloader. The Watch screen then points to the
Firmware tab, which is all such a watch can take. Send it a BattleTime firmware package the same way.

## Moving a watch onto BattleTime

A watch still running wasp-os is moved over by sending it BattleTime's `reloader-from-wasp-os.zip`
from the Firmware tab. The companion restarts wasp-os into its bootloader, finds it, and sends the
reloader, which then installs BattleTime. [The reloader](../updates.md#the-reloader-getting-onto-battletime)
explains the steps and their risks.

## From the command line

```bash
battletime flash firmware.zip       # send a firmware update
battletime recovery recovery.zip    # replace the recovery firmware in its slot
battletime slots                    # where the update and recovery slots sit
battletime slots fit                # shrink the recovery slot to its image
battletime debug                    # save a debug report after a crash
```

The recovery firmware sits in a slot of its own; `battletime recovery` replaces it while the running
firmware stays, and the bootloader menu's **Install recovery** puts it in when it is needed.
