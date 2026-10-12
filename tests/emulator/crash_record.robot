*** Settings ***
Documentation     A HardFault leaves a crash record in RAM, the bootloader leaves it alone, and the next start reads it, clears it and reports it with the reset reasons.
Suite Setup       Setup
Suite Teardown    Teardown
Test Setup        Reset Emulation
Test Teardown     Test Teardown
Resource          neotime.resource

*** Variables ***
# Erased flash near the end of the firmware slot, past the end of any firmware image.
${UNDEFINED_INSTRUCTION_ADDRESS}    0x68FF0
# Two Thumb UDF #254 instructions, which fault on the first.
${UNDEFINED_INSTRUCTIONS}           0xDEFEDEFE
# CFSR's UNDEFINSTR bit, which a UDF sets.
${UNDEFINED_INSTRUCTION_FAULT}      0x00010000
${WATCHDOG_REASON}                  0x2
${HARD_FAULT_KIND}                  2
${RAM_START}                        0x20000000
${RAM_SIZE}                         0x10000

*** Keywords ***
RAM Bytes
    ${text}=                  Execute Command    sysbus ReadBytes ${RAM_START} ${RAM_SIZE}
    ${ram}=                   Evaluate    bytes(int(byte, 16) for byte in __import__('re').findall(r'0x([0-9A-Fa-f]{2})\\b', $text))
    Should Be Equal As Integers    ${{ len($ram) }}    ${RAM_SIZE}
    RETURN    ${ram}

Address Of The Crash Record
    [Documentation]           The lowest "CRSH" in RAM; the fault handler's own copy of the record sits higher, on the stack.
    ${ram}=                   RAM Bytes
    ${offset}=                Evaluate    $ram.find(b'CRSH')
    Should Be True            ${offset} >= 0    No crash record in RAM
    RETURN    ${{ ${RAM_START} + ${offset} }}

Words Should Be In RAM
    [Documentation]           Finds the little-endian words, one after another and word-aligned, anywhere in RAM.
    [Arguments]    @{words}
    ${ram}=                   RAM Bytes
    ${pattern}=               Evaluate    b''.join(int(str(word), 0).to_bytes(4, 'little') for word in $words)
    ${offsets}=               Evaluate    [match.start() for match in __import__('re').finditer(__import__('re').escape($pattern), $ram) if match.start() % 4 == 0]
    Should Not Be Empty       ${offsets}    The words ${words} are nowhere in RAM

*** Test Cases ***
A HardFault Is Reported On The Next Start
    Create A PineTime
    Load Bootloader And Firmware
    Clear The Alive Marker
    Run For                   3
    Firmware Should Be Alive

    Execute Command           sysbus WriteDoubleWord ${UNDEFINED_INSTRUCTION_ADDRESS} ${UNDEFINED_INSTRUCTIONS}
    Execute Command           cpu PC ${UNDEFINED_INSTRUCTION_ADDRESS}
    Run For                   0.2
    ${record}=                Address Of The Crash Record
    Word Should Be            ${{ ${record} + 4 }}    ${HARD_FAULT_KIND}
    Word Should Be            ${{ ${record} + 8 }}    ${UNDEFINED_INSTRUCTION_ADDRESS}
    Word Should Be            ${{ ${record} + 16 }}    ${UNDEFINED_INSTRUCTION_FAULT}

    # The handler halts, the watchdog resets the watch, and the bootloader starts the firmware again.
    Run For                   8
    Word Should Be            ${BOOT_COUNT}    2
    Word Should Be            ${record}    0
    Word Should Be            ${RESETREAS}    0
    # The diagnostics words after the counters: reasons, kind, program counter.
    Words Should Be In RAM    ${WATCHDOG_REASON}    ${HARD_FAULT_KIND}    ${UNDEFINED_INSTRUCTION_ADDRESS}

A Start Without A Crash Reports None
    Create A PineTime
    Load Bootloader And Firmware
    Clear The Alive Marker
    Run For                   3
    Firmware Should Be Alive

    Execute Command           ${BUTTON} Press
    Run For                   6
    Execute Command           ${BUTTON} Release
    Run For                   4
    Word Should Be            ${BOOT_COUNT}    2
    ${ram}=                   RAM Bytes
    Should Be True            $ram.find(b'CRSH') < 0    A crash record is in RAM after a reset that was not a crash
    Word Should Be            ${RESETREAS}    0
