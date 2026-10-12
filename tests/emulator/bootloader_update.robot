*** Settings ***
Documentation     The bootloader updater, sent as a firmware update, replaces the bootloader of a watch already on NeoTime and hands it back to the firmware before it.
...               It starts from the bootloader the reloader put on the first watch, kept in fixtures/ because it cannot be rebuilt bit for bit.
Suite Setup       Setup
Suite Teardown    Teardown
Test Setup        Reset Emulation
Test Teardown     Test Teardown
Resource          neotime.resource

*** Variables ***
${OLD_BOOTLOADER_BIN}     ${CURDIR}/fixtures/first-watch-bootloader.bin
${UPDATER_BIN}            ${DIST}/bootloader-updater.bin
${UPDATER_NOTE}           0x2000FFFC
# A swap in, the updater's checks and write, and the swap back each take a few seconds of emulated time.
${HAND_BACK_SECONDS}      ${60}
${HOLD_SECONDS}           ${7}

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

*** Test Cases ***
Writes The New Bootloader And Hands Back To The Firmware
    Start A NeoTime Watch With The Updater Staged    ${OLD_BOOTLOADER_BIN}
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
    Start A NeoTime Watch With The Updater Staged    ${OLD_BOOTLOADER_BIN}

    Run For                   ${20}
    Flash Should Hold File    0x0    ${OLD_BOOTLOADER_BIN}
    Firmware Should Not Have Run

    Execute Command           ${BUTTON} Press
    Run For                   ${HOLD_SECONDS}
    Execute Command           ${BUTTON} Release
    Run For                   ${HAND_BACK_SECONDS}
    Flash Should Hold File    0x0    ${OLD_BOOTLOADER_BIN}
    Flash Should Hold File    ${FIRMWARE_START}    ${FIRMWARE_BIN}
    Firmware Should Be Alive
    # A quit is a real rollback: no note, so the firmware keeps saying the update did not happen.
    Boot State Should Be      ${STATE_REVERTED}
