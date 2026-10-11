*** Settings ***
Documentation     The bootloader swaps a staged update in, rolls a bad one back, and installs the recovery firmware on a long button hold.
Suite Setup       Setup
Suite Teardown    Teardown
Test Setup        Reset Emulation
Test Teardown     Test Teardown
Resource          neotime.resource

*** Variables ***
# A swap of the whole 400 KB firmware slot through the external flash takes a few seconds of emulated time.
${SWAP_SECONDS}           25

*** Test Cases ***
A Staged Firmware Is Swapped In And Started
    [Documentation]           With nothing valid to roll back to, the new firmware confirms itself at once.
    Create A PineTime
    Execute Command           sysbus LoadBinary @${BOOTLOADER_BIN} 0x0
    Execute Command           sysbus LoadBinary @${FIRMWARE_BIN} ${UPDATE_SLOT}
    Execute Command           sysbus WriteDoubleWord ${BOOT_STATE} ${STATE_SWAP}
    Clear The Alive Marker

    Run For                   ${SWAP_SECONDS}
    Flash Should Hold File    ${FIRMWARE_START}    ${FIRMWARE_BIN}
    Firmware Should Be Alive
    Boot State Should Be      ${STATE_BOOTED}

A Firmware That Never Confirms Itself Is Rolled Back
    Create A PineTime
    Load Bootloader And Firmware
    Write A Firmware That Never Confirms Itself    ${UPDATE_SLOT}
    Execute Command           sysbus WriteDoubleWord ${BOOT_STATE} ${STATE_SWAP}
    Clear The Alive Marker

    # The swap, the stuck firmware, the watchdog, and the swap back.
    Run For                   ${{ 2 * ${SWAP_SECONDS} + 10 }}
    Flash Should Hold File    ${FIRMWARE_START}    ${FIRMWARE_BIN}
    Firmware Should Be Alive
    Boot State Should Be      ${STATE_REVERTED}

Holding The Button Through Two Resets Installs The Recovery Firmware
    Create A PineTime
    Execute Command           sysbus LoadBinary @${BOOTLOADER_BIN} 0x0
    Execute Command           sysbus LoadBinary @${FIRMWARE_BIN} ${RECOVERY_SLOT}
    # Nothing in the firmware slot: a watch whose firmware was lost.
    Clear The Alive Marker

    # Five seconds of hold in the bootloader, then let go so the recovery firmware's watchdog is fed.
    Execute Command           ${BUTTON} Press
    Run For                   6
    Execute Command           ${BUTTON} Release
    Run For                   ${SWAP_SECONDS}
    Flash Should Hold File    ${FIRMWARE_START}    ${FIRMWARE_BIN}
    Firmware Should Be Alive

Without A Recovery Firmware The Hold Starts The Old Firmware
    Create A PineTime
    Load Bootloader And Firmware
    Clear The Alive Marker

    Execute Command           ${BUTTON} Press
    Run For                   6
    Execute Command           ${BUTTON} Release
    Run For                   5
    Firmware Should Be Alive
