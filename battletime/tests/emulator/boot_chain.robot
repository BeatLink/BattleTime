*** Settings ***
Documentation     The bootloader starts the firmware, the firmware keeps running, the button's clicks reach the shell, and the button hold gets back to the bootloader.
Suite Setup       Setup
Suite Teardown    Teardown
Test Setup        Reset Emulation
Test Teardown     Test Teardown
Resource          battletime.resource

*** Variables ***
${LCD}                    sysbus.serial0.spiBus.lcd

*** Keywords ***
Click The Button
    [Documentation]           A short press, then long enough for the double click window to close and the shell to act.
    Execute Command           ${BUTTON} Press
    Run For                   0.1
    Execute Command           ${BUTTON} Release
    Run For                   0.5

Screen Should Be Asleep
    [Arguments]    ${expected}
    ${sleeping}=              Execute Command    ${LCD} Sleeping
    Should Be Equal           ${sleeping.strip()}    ${expected}

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
    # The bootloader starts the firmware 4 to 5 seconds after the button comes up.
    Run For                   6
    Word Should Be            ${BOOT_COUNT}    2

A Click On The Face Puts The Watch To Sleep And The Next Wakes It
    Create A PineTime
    Load Bootloader And Firmware
    Clear The Alive Marker
    Run For                   4
    Screen Should Be Asleep   False

    Click The Button
    Screen Should Be Asleep   True
    Click The Button
    Screen Should Be Asleep   False
    Word Should Be            ${BOOT_COUNT}    1

Holding The Button For Three And A Half Seconds Does Not Reset The Watch
    [Documentation]           The firmware reads the button every half second, so only a hold of 4 seconds or more can reach the watchdog's 5.
    Create A PineTime
    Load Bootloader And Firmware
    Clear The Alive Marker
    Run For                   3
    Firmware Should Be Alive

    Execute Command           ${BUTTON} Press
    Run For                   3.5
    Execute Command           ${BUTTON} Release
    Run For                   4
    Word Should Be            ${BOOT_COUNT}    1
