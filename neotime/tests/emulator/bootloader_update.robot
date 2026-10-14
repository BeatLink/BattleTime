*** Settings ***
Documentation     The bootloader updater, sent as a firmware update, replaces the bootloader of a watch already on NeoTime and hands it back to the firmware before it.
Suite Setup       Setup
Suite Teardown    Teardown
Test Setup        Reset Emulation
Test Teardown     Test Teardown
Resource          neotime.resource

*** Variables ***
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

An Older Bootloader
    ${path}=                  Write Older Bootloader    ${BOOTLOADER_BIN}    ${OUTPUT DIR}/older-bootloader.bin
    RETURN    ${path}

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

Waits On The Charger Until The Battery Has Enough Charge
    [Documentation]           40 % puts 3.48 V on the battery, below the 3.7 V the bootloader write needs; 80 % puts 3.96 V.
    ${older}=                 An Older Bootloader
    Start A NeoTime Watch With The Updater Staged    ${older}
    Execute Command           ${BATTERY} Percent 40
    Execute Command           ${BATTERY} ConnectCharger

    Run For                   ${20}
    Flash Should Hold File    0x0    ${older}
    Firmware Should Not Have Run

    Execute Command           ${BATTERY} Percent 80
    Run For                   ${HAND_BACK_SECONDS}
    Flash Should Hold File    0x0    ${BOOTLOADER_BIN}
    The Firmware Should Be Back And Confirmed
