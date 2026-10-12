*** Settings ***
Documentation     The bootloader starts the firmware, the firmware keeps running, and the button hold gets back to the bootloader.
Suite Setup       Setup
Suite Teardown    Teardown
Test Setup        Reset Emulation
Test Teardown     Test Teardown
Resource          neotime.resource

*** Test Cases ***
Bootloader Starts The Firmware
    Create A PineTime
    Load Bootloader And Firmware
    Clear The Alive Marker

    Run For                   3
    Firmware Should Be Alive

Firmware Keeps Running While The Button Is Up
    Create A PineTime
    Load Bootloader And Firmware
    Clear The Alive Marker

    Run For                   20
    Firmware Should Be Alive
    Word Should Be            ${BOOT_COUNT}    1

Holding The Button Resets Into The Bootloader
    Create A PineTime
    Load Bootloader And Firmware
    Clear The Alive Marker
    Run For                   3
    Firmware Should Be Alive

    Execute Command           ${BUTTON} Press
    # The watchdog resets within 5 seconds and the firmware clears RESETREAS when it starts, so it is read while the bootloader runs.
    Run For                   5.5
    ${reason}=                Word At    ${RESETREAS}
    Should Be True            ${reason} & 0x2    The watchdog did not reset the watch
    Run For                   1.5
    Execute Command           ${BUTTON} Release
    Run For                   4
    Word Should Be            ${BOOT_COUNT}    2
