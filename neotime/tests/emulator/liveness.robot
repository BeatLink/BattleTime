*** Settings ***
Documentation     The watchdog is fed only while the shell and the radio make progress: a watch left alone keeps running, and a radio that stops lets the watchdog reset it.
Suite Setup       Setup
Suite Teardown    Teardown
Test Setup        Reset Emulation
Test Teardown     Test Teardown
Resource          neotime.resource

*** Variables ***
# NVIC's clear-enable register for interrupts 0 to 31; the radio is interrupt 1.
${INTERRUPT_CLEAR_ENABLE}    0xE000E180
${RADIO_INTERRUPT}           0x2
${WATCHDOG_REASON}           0x2

*** Test Cases ***
A Watch Left Alone Is Not Reset
    [Documentation]           The shell sleeps with nothing to do and the radio advertises; pings keep the shell counted as running.
    Create A PineTime
    Load Bootloader And Firmware
    Clear The Alive Marker
    Run For                   30
    Firmware Should Be Alive
    Word Should Be            ${BOOT_COUNT}    1

A Radio That Stops Lets The Watchdog Reset The Watch
    Create A PineTime
    Load Bootloader And Firmware
    Clear The Alive Marker
    Run For                   3
    Firmware Should Be Alive

    # With its interrupt off the radio stops while it still has advertising to do.
    Execute Command           sysbus WriteDoubleWord ${INTERRUPT_CLEAR_ENABLE} ${RADIO_INTERRUPT}
    Run For                   4
    Word Should Be            ${BOOT_COUNT}    1
    # Six seconds without an interrupt, the watchdog's five, and the bootloader's start.
    Run For                   20
    Word Should Be            ${BOOT_COUNT}    2
