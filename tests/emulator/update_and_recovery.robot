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

Holding The Button Opens The Menu And Picking Recovery Installs It
    Create A PineTime
    Execute Command           sysbus LoadBinary @${BOOTLOADER_BIN} 0x0
    Execute Command           sysbus LoadBinary @${RECOVERY_BIN} ${RECOVERY_SLOT}
    # Nothing in the firmware slot: a watch whose firmware was lost.
    Clear The Alive Marker

    Open The Menu
    Pick Menu Option          2
    Run For                   ${SWAP_SECONDS}
    Flash Should Hold File    ${FIRMWARE_START}    ${RECOVERY_BIN}
    Firmware Should Be Alive

Without A Recovery Firmware Picking It Starts The Old Firmware
    Create A PineTime
    Load Bootloader And Firmware
    Clear The Alive Marker

    Open The Menu
    Pick Menu Option          2
    Run For                   5
    Firmware Should Be Alive
    Flash Should Hold File    ${FIRMWARE_START}    ${FIRMWARE_BIN}

Rolling Back From The Menu Installs The Previous Firmware
    [Documentation]           The recovery build stands in for an older firmware a swap left in the update slot.
    Create A PineTime
    Execute Command           sysbus LoadBinary @${BOOTLOADER_BIN} 0x0
    Execute Command           sysbus LoadBinary @${RECOVERY_BIN} ${FIRMWARE_START}
    Execute Command           sysbus LoadBinary @${FIRMWARE_BIN} ${PREVIOUS_SLOT}
    Clear The Alive Marker

    Open The Menu
    Pick Menu Option          1
    Run For                   ${SWAP_SECONDS}
    Flash Should Hold File    ${FIRMWARE_START}    ${FIRMWARE_BIN}
    Firmware Should Be Alive

Rolling Back A Firmware On Trial Installs The One Before It
    [Documentation]           A swap leaves its progress in the boot state; the rollback must start a fresh swap rather than a revert.
    Create A PineTime
    Execute Command           sysbus LoadBinary @${BOOTLOADER_BIN} 0x0
    Execute Command           sysbus LoadBinary @${RECOVERY_BIN} ${FIRMWARE_START}
    Execute Command           sysbus LoadBinary @${FIRMWARE_BIN} ${UPDATE_SLOT}
    Execute Command           sysbus WriteDoubleWord ${BOOT_STATE} ${STATE_SWAP}
    Run For                   ${SWAP_SECONDS}
    Flash Should Hold File    ${FIRMWARE_START}    ${FIRMWARE_BIN}
    Clear The Alive Marker

    # The firmware stops feeding the watchdog while the button is down, so this resets into the bootloader.
    Execute Command           ${BUTTON} Press
    Run For                   6
    Open The Menu
    Pick Menu Option          1
    Run For                   ${SWAP_SECONDS}
    Flash Should Hold File    ${FIRMWARE_START}    ${RECOVERY_BIN}
    Firmware Should Be Alive

The Menu Starts The Firmware When Left Alone
    Create A PineTime
    Load Bootloader And Firmware
    Clear The Alive Marker

    Open The Menu
    Run For                   32
    Firmware Should Be Alive
    Flash Should Hold File    ${FIRMWARE_START}    ${FIRMWARE_BIN}

*** Keywords ***
Open The Menu
    [Documentation]           Holds the button through the bootloader's five seconds, then lets go.
    Execute Command           ${BUTTON} Press
    Run For                   6
    Execute Command           ${BUTTON} Release
    Run For                   0.2

Pick Menu Option
    [Documentation]           Moves down with short presses, then picks with a long one.
    ...                       The presses are taps, because in the emulator the bootloader's delay loop runs faster
    ...                       after the opening hold, so a 0.2 second press already counts as a pick there.
    [Arguments]    ${index}
    FOR    ${press}    IN RANGE    ${index}
        Execute Command       ${BUTTON} Press
        Run For               0.01
        Execute Command       ${BUTTON} Release
        Run For               0.5
    END
    Execute Command           ${BUTTON} Press
    Run For                   1.5
    Execute Command           ${BUTTON} Release
