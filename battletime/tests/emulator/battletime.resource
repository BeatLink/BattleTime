*** Settings ***
Documentation     Shared keywords for running BattleTime's images on the PineTime emulator.
...               Set EMULATOR to the PineTimeEmulator checkout and DIST to the folder tools/build_images.sh fills.
Resource          ${RENODEKEYWORDS}
Library           test_images.py

*** Variables ***
# The projects folder is the nearest ancestor holding the emulator, which works from a worktree too.
${PROJECTS}               ${{ str(next(p for p in __import__('pathlib').Path('${CURDIR}').resolve().parents if (p / 'PineTimeEmulator').exists())) }}
${EMULATOR}               ${PROJECTS}/PineTimeEmulator
${DIST}                   ${{ __import__('os').path.realpath('${CURDIR}/../../dist') }}
# The button and the battery hang off the GPIO port in the platform description.
${BUTTON}                 gpio0.sideButton
${BATTERY}                gpio0.battery
${PLATFORM}               @${EMULATOR}/scripts/pinetime.resc
${BOOTLOADER_BIN}         ${DIST}/bootloader.bin
${FIRMWARE_BIN}           ${DIST}/firmware.bin
${RECOVERY_BIN}           ${DIST}/recovery.bin

# From boot/src/lib.rs.
${SLOT_TABLE}             0x5000
${BOOT_STATE}             0x7000
${FIRMWARE_START}         0x8000
${EXTERNAL_FLASH}         0x60000000
${UPDATE_SLOT}            ${{ 0x60000000 + 0x000000 }}
${RECOVERY_SLOT}          ${{ 0x60000000 + 0x0C6000 }}
# Where a swap leaves the firmware it replaced: one page up in the update slot.
${PREVIOUS_SLOT}          ${{ 0x60000000 + 0x001000 }}
${ALIVE_MARKER}           0x2000FFF0
${ALIVE_MAGIC}            0x4E454F54
${BOOT_COUNT}             0x2000FFF4
${RESETREAS}              0x40000400

# embassy-boot writes one of these over the first word of the boot state page.
${STATE_SWAP}             0xF0F0F0F0
${STATE_BOOTED}           0xD0D0D0D0
${STATE_REVERTED}         0xC0C0C0C0

*** Keywords ***
Create A PineTime
    Execute Command           include ${PLATFORM}

Load Bootloader And Firmware
    Execute Command           sysbus LoadBinary @${BOOTLOADER_BIN} 0x0
    Execute Command           sysbus LoadBinary @${FIRMWARE_BIN} ${FIRMWARE_START}

Clear The Alive Marker
    Execute Command           sysbus WriteDoubleWord ${ALIVE_MARKER} 0
    Execute Command           sysbus WriteDoubleWord ${BOOT_COUNT} 0

Word At
    [Arguments]    ${address}
    ${text}=                  Execute Command    sysbus ReadDoubleWord ${address}
    ${value}=                 Convert To Integer    ${text.strip()}
    RETURN    ${value}

Word Should Be
    [Arguments]    ${address}    ${expected}
    ${value}=                 Word At    ${address}
    Should Be Equal As Integers    ${value}    ${expected}

Firmware Should Be Alive
    Word Should Be            ${ALIVE_MARKER}    ${ALIVE_MAGIC}

Firmware Should Not Have Run
    ${value}=                 Word At    ${ALIVE_MARKER}
    Should Not Be Equal As Integers    ${value}    ${ALIVE_MAGIC}

Boot State Should Be
    [Arguments]    ${expected}
    Word Should Be            ${BOOT_STATE}    ${expected}

Flash Should Hold File
    [Documentation]           Compares every 64th word of the region with the file, which catches a missing or shifted page.
    [Arguments]    ${address}    ${path}
    ${length}=                Evaluate    __import__('os').path.getsize('${path}')
    FOR    ${offset}    IN RANGE    0    ${length} - 3    256
        ${expected}=          Evaluate    int.from_bytes(open('${path}', 'rb').read()[${offset}:${offset} + 4], 'little')
        ${actual}=            Word At    ${{ ${address} + ${offset} }}
        Should Be Equal As Integers    ${actual}    ${expected}    Word at offset ${offset} differs
    END

Flash Holds File
    [Documentation]           True when the region matches the file at the sampled words.
    [Arguments]    ${address}    ${path}
    ${status}=                Run Keyword And Return Status    Flash Should Hold File    ${address}    ${path}
    RETURN    ${status}

Run For
    [Arguments]    ${seconds}
    Execute Command           emulation RunFor "${seconds}"

Write A Firmware That Never Confirms Itself
    [Documentation]           A sound header, two words of vector table and a branch to self at the given address: the bootloader starts it, it never confirms, the watchdog resets.
    [Arguments]    ${address}
    ${path}=                  Write Never Confirming Image    ${OUTPUT DIR}/never-confirms.bin
    Execute Command           sysbus LoadBinary @${path} ${address}
