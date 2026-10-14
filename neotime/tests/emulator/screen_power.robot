*** Settings ***
Documentation     The light lowers two seconds before the screen timeout and input brings it back, the external flash sleeps in
...               deep power-down while the screen is off and wakes for the next store call, and the clock counts on across a reset.
Suite Setup       Setup
Suite Teardown    Teardown
Test Setup        Reset Emulation
Test Teardown     Test Teardown
Resource          neotime.resource
Library           elf_symbols.py

*** Variables ***
${FIRMWARE_ELF}                     ${DIST}/firmware.elf
${TOUCH}                            sysbus.serial1.touch
${EXTERNAL_FLASH_PART}              sysbus.serial0.spiBus.externalFlash
${EXTERNAL_FLASH_MEMORY}            sysbus.externalFlashMemory
${ACCELEROMETER}                    sysbus.twi1.accelerometer
${FLASH_BEFORE}                     ${TEMPDIR}/neotime_screen_power_flash_before.bin
${FLASH_AFTER}                      ${TEMPDIR}/neotime_screen_power_flash_after.bin
# A six-minute step slot, and the 30-second step read that notices it ended.
${SLOT_AND_A_READ_SECONDS}          400
# Port 0's OUT register; the middle and high backlight pins light when low.
${PORT_OUT}                         0x50000504
${MIDDLE_LIGHT_PIN}                 22
${HIGH_LIGHT_PIN}                   23
# The diagnostics counter that says whether the screen is lit, the third word.
${SCREEN_LIT_OFFSET}                8
# The default screen timeout, and how long before it the light lowers.
${TIMEOUT_SECONDS}                  15
${LOWERED_SECONDS}                  2
# AIRCR with its key and SYSRESETREQ: a soft reset, as the firmware's own restart does.
${AIRCR}                            0xE000ED0C
${SYSTEM_RESET_REQUEST}             0x05FA0004
# Erased flash just past the firmware image, worked out from its size, for the few instructions that write AIRCR.
${RESET_CODE_ADDRESS}               ${{ (${FIRMWARE_START} + __import__('os').path.getsize($FIRMWARE_BIN) + 0xFF) // 0x100 * 0x100 }}
# ldr r0, [pc, #4]; ldr r1, [pc, #8]; str r1, [r0]; b . ; then the two words those loads read.
${RESET_CODE}                       0x49024801
${RESET_CODE_STORE_AND_LOOP}        0xE7FE6001
# 2030-01-01 00:00 UTC in milliseconds, far from any build time.
${SET_TIME}                         1893456000000
${KEPT_CLOCK_MARKER}                0x454D4954

*** Keywords ***
Screen Lit
    ${counters}=              Symbol Address    ${FIRMWARE_ELF}    11diagnostics8COUNTERS
    ${lit}=                   Word At    ${{ ${counters} + ${SCREEN_LIT_OFFSET} }}
    RETURN    ${lit}

Light Pin Is Lit
    [Arguments]    ${pin}
    ${out}=                   Word At    ${PORT_OUT}
    RETURN    ${{ (${out} >> ${pin}) & 1 == 0 }}

Light Pin Should Be Lit
    [Arguments]    ${pin}
    ${lit}=                   Light Pin Is Lit    ${pin}
    Should Be True            ${lit}    Backlight pin ${pin} is off

Light Pin Should Be Off
    [Arguments]    ${pin}
    ${lit}=                   Light Pin Is Lit    ${pin}
    Should Not Be True        ${lit}    Backlight pin ${pin} is lit

Flash Should Be Asleep
    ${asleep}=                Execute Command    ${EXTERNAL_FLASH_PART} DeepPoweredDown
    Should Be Equal           ${asleep.strip()}    True    The external flash is awake

Flash Should Be Awake
    ${asleep}=                Execute Command    ${EXTERNAL_FLASH_PART} DeepPoweredDown
    Should Be Equal           ${asleep.strip()}    False    The external flash is in deep power-down

Run Until The Middle Light Goes Off
    [Documentation]           Runs in quarter seconds until the middle pin goes off; gives the seconds that took.
    [Arguments]    ${most_seconds}
    FOR    ${quarters}    IN RANGE    ${{ int((${most_seconds}) * 4) }}
        ${lit}=               Light Pin Is Lit    ${MIDDLE_LIGHT_PIN}
        IF    not ${lit}    RETURN    ${{ ${quarters} / 4 }}
        Run For               0.25
    END
    Fail                      The middle light stayed on for ${most_seconds} s

Start The Watch
    Create A PineTime
    Load Bootloader And Firmware
    Clear The Alive Marker
    Run For                   3
    Firmware Should Be Alive

Kept Clock
    [Documentation]           The kept clock's UTC in milliseconds, after checking its marker.
    ${kept}=                  Symbol Address    ${FIRMWARE_ELF}    KEPT_CLOCK
    Word Should Be            ${kept}    ${KEPT_CLOCK_MARKER}
    ${low}=                   Word At    ${{ ${kept} + 4 }}
    ${high}=                  Word At    ${{ ${kept} + 8 }}
    RETURN    ${{ (${high} << 32) | ${low} }}

Set The Clock
    [Documentation]           Sets the clock's UTC at uptime zero, the first field of the static.
    [Arguments]    ${utc}
    ${clock}=                 Symbol Address    ${FIRMWARE_ELF}    5clock5CLOCK
    Execute Command           sysbus WriteDoubleWord ${clock} ${{ ${utc} & 0xFFFFFFFF }}
    Execute Command           sysbus WriteDoubleWord ${{ ${clock} + 4 }} ${{ ${utc} >> 32 }}

Changed Bytes
    [Documentation]           How many bytes differ between two dumps of the external flash.
    [Arguments]    ${before}    ${after}
    ${count}=                 Evaluate    sum(a != b for a, b in zip(open($before, 'rb').read(), open($after, 'rb').read()))
    RETURN    ${count}

Soft Reset
    [Documentation]           Runs a few poked instructions that write AIRCR, as the firmware's own restart does.
    Execute Command           sysbus WriteDoubleWord ${RESET_CODE_ADDRESS} ${RESET_CODE}
    Execute Command           sysbus WriteDoubleWord ${{ ${RESET_CODE_ADDRESS} + 4 }} ${RESET_CODE_STORE_AND_LOOP}
    Execute Command           sysbus WriteDoubleWord ${{ ${RESET_CODE_ADDRESS} + 8 }} ${AIRCR}
    Execute Command           sysbus WriteDoubleWord ${{ ${RESET_CODE_ADDRESS} + 12 }} ${SYSTEM_RESET_REQUEST}
    Execute Command           cpu PC ${RESET_CODE_ADDRESS}

*** Test Cases ***
The Light Lowers Before The Timeout And Input Brings It Back
    Start The Watch
    Light Pin Should Be Lit    ${MIDDLE_LIGHT_PIN}

    ${waited}=                Run Until The Middle Light Goes Off    ${{ ${TIMEOUT_SECONDS} + 10 }}
    ${lit}=                   Screen Lit
    Should Be Equal As Integers    ${lit}    1    The screen went off without lowering its light first

    # A touch while lowered brings the wearer's level back and starts the timeout again.
    Execute Command           ${TOUCH} Tap 120 120
    Run For                   0.5
    Light Pin Should Be Lit    ${MIDDLE_LIGHT_PIN}
    ${waited}=                Run Until The Middle Light Goes Off    ${TIMEOUT_SECONDS}
    Should Be True            ${TIMEOUT_SECONDS} - ${LOWERED_SECONDS} - 1.5 <= ${waited} <= ${TIMEOUT_SECONDS} - ${LOWERED_SECONDS}
    ...                       The light lowered ${waited} s after the touch
    ${lit}=                   Screen Lit
    Should Be Equal As Integers    ${lit}    1

    # With no input the screen goes off at the timeout, two seconds after lowering.
    Run For                   ${{ ${LOWERED_SECONDS} + 0.5 }}
    ${lit}=                   Screen Lit
    Should Be Equal As Integers    ${lit}    0

The External Flash Sleeps While The Screen Is Off And Wakes For The Store
    Start The Watch
    Flash Should Be Awake
    Run Until The Middle Light Goes Off    ${{ ${TIMEOUT_SECONDS} + 10 }}
    Run For                   ${{ ${LOWERED_SECONDS} + 0.5 }}
    ${lit}=                   Screen Lit
    Should Be Equal As Integers    ${lit}    0
    Flash Should Be Asleep
    Execute Command           ${EXTERNAL_FLASH_MEMORY} SaveBinary @${FLASH_BEFORE}

    # Steps in a slot that then ends make the storage task write the step log while the screen stays off.
    Set The Clock             ${SET_TIME}
    Execute Command           ${ACCELEROMETER} AddSteps 100
    Run For                   35
    Execute Command           ${ACCELEROMETER} AddSteps 50
    Run For                   ${SLOT_AND_A_READ_SECONDS}
    ${lit}=                   Screen Lit
    Should Be Equal As Integers    ${lit}    0
    Execute Command           ${EXTERNAL_FLASH_MEMORY} SaveBinary @${FLASH_AFTER}
    ${changed}=               Changed Bytes    ${FLASH_BEFORE}    ${FLASH_AFTER}
    Should Be True            ${changed} > 0    The step log never reached the flash
    Flash Should Be Asleep

    # A touch lights the screen; the chip wakes only when the store next needs it.
    Execute Command           ${TOUCH} Tap 120 120
    Run For                   0.5
    ${lit}=                   Screen Lit
    Should Be Equal As Integers    ${lit}    1

    # The bootloader and the next start wake the chip, and the store the steps went into mounts again.
    Soft Reset
    Run For                   8
    Word Should Be            ${BOOT_COUNT}    2
    Flash Should Be Awake
    Firmware Should Be Alive

The Clock Counts On Across A Soft Reset
    Start The Watch
    Set The Clock             ${SET_TIME}
    Run For                   1
    ${kept}=                  Kept Clock
    Should Be True            ${SET_TIME} < ${kept} < ${SET_TIME} + 10000    The kept clock reads ${kept}

    Soft Reset
    Run For                   8
    Word Should Be            ${BOOT_COUNT}    2
    Word Should Be            ${RESETREAS}    0
    ${after}=                 Kept Clock
    # It counts on from the kept time; only the bootloader's start and the half second between keeps are lost.
    Should Be True            ${kept} < ${after} < ${kept} + 12000    The clock reads ${after} after the reset, kept ${kept}

The Clock Counts On Across A Watchdog Reset
    Start The Watch
    Set The Clock             ${SET_TIME}
    Run For                   1
    ${kept}=                  Kept Clock

    # Holding the button stops the watchdog's feed, and the bootloader's menu starts the firmware again once left alone.
    Execute Command           ${BUTTON} Press
    Run For                   6
    Execute Command           ${BUTTON} Release
    Run For                   32
    Word Should Be            ${BOOT_COUNT}    2
    ${after}=                 Kept Clock
    Should Be True            ${kept} < ${after} < ${kept} + 60000    The clock reads ${after} after the reset, kept ${kept}

A Power Cut Starts The Clock From The Build Time
    Start The Watch
    Set The Clock             ${SET_TIME}
    Run For                   1
    ${kept}=                  Kept Clock
    Should Be True            ${kept} > ${SET_TIME}

    # RAM after the power comes back holds noise, which fails the kept clock's check.
    Execute Command           sysbus.powerClock PowerCycle
    Run For                   8
    ${after}=                 Kept Clock
    Should Be True            ${after} < ${SET_TIME}    The clock kept ${after} through a power cut
