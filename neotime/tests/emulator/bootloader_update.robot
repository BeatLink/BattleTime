*** Settings ***
Documentation     The bootloader updater, sent as a firmware update, replaces the bootloader of a watch already on NeoTime and hands it back to the firmware before it.
...               Its first-layout build moves a watch from the 16 KB bootloader, kept in fixtures/ as built from 89d3a63, to the current layout.
...               Its 388 KB-layout build moves a watch whose shared library sat at 0x69000, with the bootloader kept in fixtures/ as built from 0b6fd33.
Suite Setup       Setup
Suite Teardown    Teardown
Test Setup        Reset Emulation
Test Teardown     Test Teardown
Resource          neotime.resource

*** Variables ***
${FIRST_LAYOUT_BOOTLOADER_BIN}    ${CURDIR}/fixtures/first-layout-bootloader.bin
${UPDATER_BIN}            ${DIST}/bootloader-updater.bin
${MIGRATING_UPDATER_BIN}  ${DIST}/bootloader-updater-from-first-layout.bin
${OLD_LAYOUT_BOOTLOADER_BIN}    ${CURDIR}/fixtures/388-kb-layout-bootloader.bin
${OLD_LAYOUT_UPDATER_BIN}    ${DIST}/bootloader-updater-from-388-kb-layout.bin
# The 16 KB layout kept its boot state at 0x4000 and started the firmware at 0x5000.
${FIRST_LAYOUT_BOOT_STATE}    0x4000
${BOOT_NOTE}              0x2000FFF8
${RECOVERY_INSTALLED_NOTE}    0xB0070002
${UPDATER_NOTE}           0x2000FFFC
# A swap in, the updater's checks and write, and the swap back each take a few seconds of emulated time.
${HAND_BACK_SECONDS}      ${60}
${HOLD_SECONDS}           ${7}
# The old swap, the recovery and bootloader writes, then the new bootloader's install of the recovery firmware.
${MIGRATION_SECONDS}      ${120}

*** Keywords ***
Start A NeoTime Watch With The Updater Staged
    [Arguments]    ${bootloader}
    Create A PineTime
    Execute Command           sysbus LoadBinary @${bootloader} 0x0
    Execute Command           sysbus LoadBinary @${FIRMWARE_BIN} ${FIRMWARE_START}
    Execute Command           sysbus LoadBinary @${UPDATER_BIN} ${UPDATE_SLOT}
    Execute Command           sysbus WriteDoubleWord ${BOOT_STATE} ${STATE_SWAP}
    Clear The Alive Marker
    Execute Command           sysbus WriteDoubleWord ${UPDATER_NOTE} 0

The Firmware Should Be Back And Confirmed
    Flash Should Hold File    ${FIRMWARE_START}    ${FIRMWARE_BIN}
    Firmware Should Be Alive
    # The firmware clears the rollback's mark when the updater's note says the rollback was meant.
    Boot State Should Be      ${STATE_BOOTED}
    # And the note is used up, so a later rollback reads as a real one.
    Word Should Be            ${UPDATER_NOTE}    0

An Older Bootloader
    ${path}=                  Write Older Bootloader    ${BOOTLOADER_BIN}    ${OUTPUT DIR}/older-bootloader.bin
    RETURN    ${path}

Start A First Layout Watch With The Migrating Updater Staged
    Create A PineTime
    Execute Command           sysbus LoadBinary @${FIRST_LAYOUT_BOOTLOADER_BIN} 0x0
    Execute Command           sysbus LoadBinary @${MIGRATING_UPDATER_BIN} ${UPDATE_SLOT}
    Execute Command           sysbus WriteDoubleWord ${FIRST_LAYOUT_BOOT_STATE} ${STATE_SWAP}
    Clear The Alive Marker

Start A 388 KB Layout Watch With The Migrating Updater Staged
    [Documentation]           As the watch was: its slot table moved recovery to 0x65000 and shrank it, and a firmware of that layout runs.
    Create A PineTime
    Execute Command           sysbus LoadBinary @${OLD_LAYOUT_BOOTLOADER_BIN} 0x0
    ${record}=                Write Slot Table Record    ${OUTPUT DIR}/old-slots.bin    0x0    ${{ 388 * 1024 + 4096 }}    0x65000    ${{ 168 * 1024 }}
    Execute Command           sysbus LoadBinary @${record} ${SLOT_TABLE}
    ${old_firmware}=          Write Never Confirming Image    ${OUTPUT DIR}/old-layout-firmware.bin    1
    Execute Command           sysbus LoadBinary @${old_firmware} ${FIRMWARE_START}
    Execute Command           sysbus LoadBinary @${OLD_LAYOUT_UPDATER_BIN} ${UPDATE_SLOT}
    Execute Command           sysbus WriteDoubleWord ${BOOT_STATE} ${STATE_SWAP}
    Clear The Alive Marker

*** Test Cases ***
Writes The New Bootloader And Hands Back To The Firmware
    ${older}=                 An Older Bootloader
    Start A NeoTime Watch With The Updater Staged    ${older}
    Execute Command           ${BATTERY} ConnectCharger

    Run For                   ${HAND_BACK_SECONDS}
    Flash Should Hold File    0x0    ${BOOTLOADER_BIN}
    The Firmware Should Be Back And Confirmed

Leaves A Current Bootloader Alone
    Start A NeoTime Watch With The Updater Staged    ${BOOTLOADER_BIN}
    Execute Command           ${BATTERY} ConnectCharger

    Run For                   ${HAND_BACK_SECONDS}
    Flash Should Hold File    0x0    ${BOOTLOADER_BIN}
    The Firmware Should Be Back And Confirmed

Waits For The Charger, And Holding The Button Quits Without Writing
    ${older}=                 An Older Bootloader
    Start A NeoTime Watch With The Updater Staged    ${older}

    Run For                   ${20}
    Flash Should Hold File    0x0    ${older}
    Firmware Should Not Have Run

    Execute Command           ${BUTTON} Press
    Run For                   ${HOLD_SECONDS}
    Execute Command           ${BUTTON} Release
    Run For                   ${HAND_BACK_SECONDS}
    Flash Should Hold File    0x0    ${older}
    Flash Should Hold File    ${FIRMWARE_START}    ${FIRMWARE_BIN}
    Firmware Should Be Alive
    # A quit is a real rollback: no note, so the firmware keeps saying the update did not happen.
    Boot State Should Be      ${STATE_REVERTED}

Moves A First Layout Watch Onto The Current Layout
    [Documentation]           The 16 KB bootloader swaps the updater in at 0x5000; it writes the recovery firmware and the new bootloader,
    ...                       and the new bootloader, finding no sound header at 0x8000, installs the recovery firmware.
    Start A First Layout Watch With The Migrating Updater Staged
    Execute Command           ${BATTERY} ConnectCharger

    Run For                   ${MIGRATION_SECONDS}
    Flash Should Hold File    0x0    ${BOOTLOADER_BIN}
    Flash Should Hold File    ${RECOVERY_SLOT}    ${RECOVERY_BIN}
    Flash Should Hold File    ${FIRMWARE_START}    ${RECOVERY_BIN}
    Firmware Should Be Alive
    Word Should Be            ${BOOT_NOTE}    ${RECOVERY_INSTALLED_NOTE}

    # The recovery firmware confirmed itself, so a reset keeps it.
    Clear The Alive Marker
    Execute Command           sysbus WriteDoubleWord 0xE000ED0C 0x05FA0004
    Run For                   10
    Flash Should Hold File    ${FIRMWARE_START}    ${RECOVERY_BIN}
    Firmware Should Be Alive

The Migrating Updater Waits For The Charger, And Holding The Button Quits Without Writing
    Start A First Layout Watch With The Migrating Updater Staged

    Run For                   ${20}
    Flash Should Hold File    0x0    ${FIRST_LAYOUT_BOOTLOADER_BIN}
    Execute Command           ${BUTTON} Press
    Run For                   ${HOLD_SECONDS}
    Execute Command           ${BUTTON} Release
    Run For                   ${HAND_BACK_SECONDS}
    Flash Should Hold File    0x0    ${FIRST_LAYOUT_BOOTLOADER_BIN}
    # The recovery slot is only written once the charger is in.
    ${first_word}=            Word At    ${RECOVERY_SLOT}
    Should Be Equal As Integers    ${first_word}    0xFFFFFFFF

Moves A 388 KB Layout Watch Onto The Current Layout
    [Documentation]           The old bootloader swaps the updater in; it writes the recovery firmware and the new bootloader. The new bootloader
    ...                       ignores the old slot table record, finds only images of the old header version at 0x8000 and one page up in the
    ...                       update slot, and installs the recovery firmware, from which the firmware with its library goes on as an update.
    Start A 388 KB Layout Watch With The Migrating Updater Staged
    Execute Command           ${BATTERY} ConnectCharger

    Run For                   ${MIGRATION_SECONDS}
    Flash Should Hold File    0x0    ${BOOTLOADER_BIN}
    Flash Should Hold File    ${RECOVERY_SLOT}    ${RECOVERY_BIN}
    Flash Should Hold File    ${FIRMWARE_START}    ${RECOVERY_BIN}
    Firmware Should Be Alive
    Word Should Be            ${BOOT_NOTE}    ${RECOVERY_INSTALLED_NOTE}

    Execute Command           sysbus LoadBinary @${FIRMWARE_BIN} ${UPDATE_SLOT}
    Execute Command           sysbus WriteDoubleWord ${BOOT_STATE} ${STATE_SWAP}
    Clear The Alive Marker
    Execute Command           sysbus WriteDoubleWord 0xE000ED0C 0x05FA0004
    Run For                   ${HAND_BACK_SECONDS}
    Flash Should Hold File    ${FIRMWARE_START}    ${FIRMWARE_BIN}
    Firmware Should Be Alive

The 388 KB Layout Updater Leaves The Old Watch Alone Without The Charger
    Start A 388 KB Layout Watch With The Migrating Updater Staged

    Run For                   ${20}
    Flash Should Hold File    0x0    ${OLD_LAYOUT_BOOTLOADER_BIN}
    ${first_word}=            Word At    ${RECOVERY_SLOT}
    Should Be Equal As Integers    ${first_word}    0xFFFFFFFF
