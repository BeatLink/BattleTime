*** Settings ***
Documentation     A HardFault leaves a crash record in RAM, the bootloader leaves it alone, and the next start reports it with the reset reasons
...               and shows the crash screen, which keeps the record until the wearer picks Restart.
Suite Setup       Setup
Suite Teardown    Teardown
Test Setup        Reset Emulation
Test Teardown     Test Teardown
Resource          neotime.resource
Library           screen_pixels.py
Library           elf_symbols.py

*** Variables ***
# Erased flash just past the firmware image, worked out from its size because the image grows.
${FREE_FLASH}                       ${{ (${FIRMWARE_START} + __import__('os').path.getsize($FIRMWARE_BIN) + 0xFF) // 0x100 * 0x100 }}
${UNDEFINED_INSTRUCTION_ADDRESS}    ${{ ${FREE_FLASH} + 0x20 }}
# Two Thumb UDF #254 instructions, which fault on the first.
${UNDEFINED_INSTRUCTIONS}           0xDEFEDEFE
# CFSR's UNDEFINSTR bit, which a UDF sets.
${UNDEFINED_INSTRUCTION_FAULT}      0x00010000
${WATCHDOG_REASON}                  0x2
${HARD_FAULT_KIND}                  2
${OUT_OF_MEMORY_KIND}               3
${FIRMWARE_ELF}                     ${DIST}/firmware.elf
${REFUSED_BYTES}                    4321
# Zeroed words the poked panic reads as its information, high in the stack and clear of the alive marker.
${PANIC_INFORMATION}                0x2000FF00
# Erased flash below the undefined instructions, for a few instructions that call the panic handler.
${PANIC_CALL_ADDRESS}               ${FREE_FLASH}
# ldr r0, [pc, #8]; ldr r1, [pc, #12]; bx r1; nops; then the two addresses those loads read.
${PANIC_CALL_CODE}                  0x49034802
${PANIC_CALL_BRANCH}                0xBF004708
${PANIC_CALL_PADDING}               0xBF00BF00
# The memory protection unit's control register, which the firmware turns on, and its region base
# register, which holds the stack guard's address.
${MPU_CONTROL}                      0xE000ED94
${MPU_REGION_BASE}                  0xE000ED9C
${STACK_GUARD_BYTES}                256
# CFSR's MemManage bits: a data access or a push into a protected region.
${MEMORY_FAULT_BITS}                0x00000012
${RAM_START}                        0x20000000
${RAM_SIZE}                         0x10000
${LCD}                              sysbus.serial0.spiBus.lcd
${TOUCH}                            sysbus.serial1.touch
${SCREENSHOT}                       ${TEMPDIR}/neotime_crash_screen.png
# The middles of the crash screen's two buttons, Restart above Send debug info, from neotime_crash_record::screen.
${RESTART_X}                        120
${RESTART_Y}                        160
${SEND_DEBUG_INFO_Y}                212

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

Make The Firmware Crash
    [Documentation]           Runs an undefined instruction, waits out the watchdog reset and the start-up page; gives the record's address.
    Create A PineTime
    Load Bootloader And Firmware
    Clear The Alive Marker
    Run For                   3
    Firmware Should Be Alive
    Execute Command           sysbus WriteDoubleWord ${UNDEFINED_INSTRUCTION_ADDRESS} ${UNDEFINED_INSTRUCTIONS}
    Execute Command           cpu PC ${UNDEFINED_INSTRUCTION_ADDRESS}
    Run For                   0.2
    ${record}=                Address Of The Crash Record
    Run For                   12
    Word Should Be            ${BOOT_COUNT}    2
    RETURN    ${record}

Screen Should Be Red
    [Documentation]           The crash pages' dark red, whose red is well above its green and blue.
    Execute Command           ${LCD} SaveScreenshot @${SCREENSHOT}
    ${red}    ${green}    ${blue}=    Pixel At    ${SCREENSHOT}    2    2
    Should Be True            ${red} > 40 and ${red} > 4 * ${green} and ${red} > 4 * ${blue}    The corner is ${red},${green},${blue}, not the crash screen

Screen Should Not Be Red
    Execute Command           ${LCD} SaveScreenshot @${SCREENSHOT}
    ${red}    ${green}    ${blue}=    Pixel At    ${SCREENSHOT}    2    2
    Should Not Be True        ${red} > 40 and ${red} > 4 * ${green} and ${red} > 4 * ${blue}    The crash screen is still up

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
    # The record stays until the wearer picks Restart on the crash screen.
    Word Should Be            ${record}    0x48535243
    Word Should Be            ${RESETREAS}    0
    # The diagnostics words after the counters: reasons, kind, program counter.
    Words Should Be In RAM    ${WATCHDOG_REASON}    ${HARD_FAULT_KIND}    ${UNDEFINED_INSTRUCTION_ADDRESS}

A Stack Overflow Faults At The Guard And Is Reported
    Create A PineTime
    Load Bootloader And Firmware
    Clear The Alive Marker
    Run For                   3
    Firmware Should Be Alive

    # The Rust engine has no memory protection unit yet, so there the guard cannot fault; Renode at 97ebb02 runs it.
    ${control}=               Word At    ${MPU_CONTROL}
    Skip If                   ${control} == 0    This emulator has no memory protection unit

    # A stack pointer just above the guard makes the next push land in it, as a stack grown too deep would.
    ${base}=                  Word At    ${MPU_REGION_BASE}
    ${guard}=                 Evaluate    ${base} & ~0x1F
    Execute Command           cpu SP ${{ ${guard} + ${STACK_GUARD_BYTES} + 8 }}
    Run For                   0.2
    ${record}=                Address Of The Crash Record
    Word Should Be            ${{ ${record} + 4 }}    ${HARD_FAULT_KIND}
    ${status}=                Word At    ${{ ${record} + 16 }}
    Should Be True            ${status} & ${MEMORY_FAULT_BITS}    CFSR ${status} shows no MemManage fault
    ${address}=               Word At    ${{ ${record} + 20 }}
    Should Be True            ${guard} <= ${address} < ${guard} + ${STACK_GUARD_BYTES}    The fault address ${address} is outside the guard

    Run For                   8
    Word Should Be            ${BOOT_COUNT}    2
    Words Should Be In RAM    ${WATCHDOG_REASON}    ${HARD_FAULT_KIND}

A Start Without A Crash Reports None
    Create A PineTime
    Load Bootloader And Firmware
    Clear The Alive Marker
    Run For                   3
    Firmware Should Be Alive

    # The hold opens the bootloader's menu, which starts the firmware again once left alone.
    Execute Command           ${BUTTON} Press
    Run For                   6
    Execute Command           ${BUTTON} Release
    Run For                   32
    Word Should Be            ${BOOT_COUNT}    2
    ${ram}=                   RAM Bytes
    Should Be True            $ram.find(b'CRSH') < 0    A crash record is in RAM after a reset that was not a crash
    Word Should Be            ${RESETREAS}    0

A Crash Shows The Crash Screen And Restart Clears It
    ${record}=                Make The Firmware Crash
    Screen Should Be Red
    Words Should Be In RAM    ${WATCHDOG_REASON}    ${HARD_FAULT_KIND}    ${UNDEFINED_INSTRUCTION_ADDRESS}

    Execute Command           ${TOUCH} Tap ${RESTART_X} ${RESTART_Y}
    Run For                   1
    Screen Should Not Be Red
    Word Should Be            ${record}    0
    # The diagnostics words keep the crash for the rest of the run.
    Words Should Be In RAM    ${WATCHDOG_REASON}    ${HARD_FAULT_KIND}    ${UNDEFINED_INSTRUCTION_ADDRESS}

Send Debug Info Waits And Skipping Keeps The Crash
    ${record}=                Make The Firmware Crash
    Execute Command           ${TOUCH} Tap ${RESTART_X} ${SEND_DEBUG_INFO_Y}
    Run For                   1
    Screen Should Be Red
    Word Should Be            ${record}    0x48535243

    # A click of the side button skips the wait; the crash stays for the companion app and the next start.
    Execute Command           ${BUTTON} Press
    Run For                   0.1
    Execute Command           ${BUTTON} Release
    Run For                   1
    Screen Should Not Be Red
    Word Should Be            ${record}    0x48535243

An Allocation Failure Is Reported As Running Out Of Memory
    Create A PineTime
    Load Bootloader And Firmware
    Clear The Alive Marker
    Run For                   3
    Firmware Should Be Alive

    # As the heap does when it refuses an allocation, then the panic the refusal raises.
    ${refused}=               Symbol Address    ${FIRMWARE_ELF}    REFUSED_ALLOCATION_BYTES
    ${panic_handler}=         Symbol Address    ${FIRMWARE_ELF}    rust_begin_unwind
    Execute Command           sysbus WriteDoubleWord ${refused} ${REFUSED_BYTES}
    FOR    ${offset}    IN RANGE    0    16    4
        Execute Command       sysbus WriteDoubleWord ${{ ${PANIC_INFORMATION} + ${offset} }} 0
    END
    Execute Command           sysbus WriteDoubleWord ${PANIC_CALL_ADDRESS} ${PANIC_CALL_CODE}
    Execute Command           sysbus WriteDoubleWord ${{ ${PANIC_CALL_ADDRESS} + 4 }} ${PANIC_CALL_BRANCH}
    Execute Command           sysbus WriteDoubleWord ${{ ${PANIC_CALL_ADDRESS} + 8 }} ${PANIC_CALL_PADDING}
    Execute Command           sysbus WriteDoubleWord ${{ ${PANIC_CALL_ADDRESS} + 12 }} ${PANIC_INFORMATION}
    Execute Command           sysbus WriteDoubleWord ${{ ${PANIC_CALL_ADDRESS} + 16 }} ${{ ${panic_handler} | 1 }}
    Execute Command           cpu PC ${PANIC_CALL_ADDRESS}
    Run For                   0.2
    ${record}=                Address Of The Crash Record
    Word Should Be            ${{ ${record} + 4 }}    ${OUT_OF_MEMORY_KIND}
    Word Should Be            ${{ ${record} + 8 }}    ${REFUSED_BYTES}

    Run For                   12
    Word Should Be            ${BOOT_COUNT}    2
    Screen Should Be Red
    Words Should Be In RAM    ${WATCHDOG_REASON}    ${OUT_OF_MEMORY_KIND}    ${REFUSED_BYTES}
