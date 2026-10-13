*** Settings ***
Documentation     The reloader moves a watch from wasp-os or InfiniTime onto NeoTime.
Suite Setup       Setup
Suite Teardown    Teardown
Test Setup        Reset Emulation
Test Teardown     Test Teardown
Resource          neotime.resource
Library           OperatingSystem

*** Variables ***
${WASP_OS}                ${PROJECTS}/wasp-os
# MBR, S112 and wasp-os's bootloader on S112, merged, as `make BOARD=pinetime SD=s112 bootloader` leaves it.
${WASP_BOOTLOADER_HEX}    ${WASP_OS}/build-pinetime-s112/bootloader.hex
${WASP_RELOADER_BIN}      ${DIST}/reloader-from-wasp-os.bin
${WASP_APP_START}         0x19000
${WASP_APP_END}           0x78000
${WASP_SETTINGS}          0x7F000
${BANK_VALID_APP}         0x1

${INFINITIME}             ${PROJECTS}/InfiniTime
# Copied to a plain name before loading, since Renode refuses the dots in the release file name.
${MCUBOOT_RELEASE}        ${INFINITIME}/bootloader/bootloader-5.0.4.bin
${MCUBOOT_BIN}            ${TEMPDIR}/neotime_mcuboot.bin
${INFINITIME_RELOADER}    ${DIST}/reloader-from-infinitime.mcuboot.bin
${MCUBOOT_SLOT}           0x8000

# How long the reloader needs, with the flash's real busy times, plus the first NeoTime boot.
${RELOADER_SECONDS}       40
${FIRST_BOOT_SECONDS}     30
# The bootloader's 20 KB, the slot table and the boot state page after it.
${OLD_START_LENGTH}       0x8000

*** Keywords ***
Start A Watch Running wasp-os With The Reloader As Its App
    Start A Watch Running wasp-os With The Reloader As Its App, Unplugged
    Execute Command           ${BATTERY} ConnectCharger

Start A Watch Running wasp-os With The Reloader As Its App, Unplugged
    Create A PineTime
    Execute Command           sysbus LoadHEX @${WASP_BOOTLOADER_HEX}
    Execute Command           sysbus LoadBinary @${WASP_RELOADER_BIN} ${WASP_APP_START}
    # Bootloader settings: bank 0 holds a valid app whose CRC is not checked.
    ${size}=                  Evaluate    __import__('os').path.getsize('${WASP_RELOADER_BIN}')
    Execute Command           sysbus WriteDoubleWord ${WASP_SETTINGS} ${BANK_VALID_APP}
    Execute Command           sysbus WriteDoubleWord ${{ ${WASP_SETTINGS} + 8 }} ${size}
    Remember The Old Start Of Flash
    Clear The Alive Marker

Start A Watch Running InfiniTime's Bootloader With The Reloader As Its App
    Create A PineTime
    Copy File                 ${MCUBOOT_RELEASE}    ${MCUBOOT_BIN}
    Execute Command           sysbus LoadBinary @${MCUBOOT_BIN} 0x0
    Execute Command           sysbus LoadBinary @${INFINITIME_RELOADER} ${MCUBOOT_SLOT}
    Execute Command           ${BATTERY} ConnectCharger
    Remember The Old Start Of Flash
    Clear The Alive Marker

NeoTime Should Be Installed And Running
    [Documentation]           The reloader installs the recovery firmware as the firmware too; the update slot is not checked, since the swap leaves the old internal flash there.
    Flash Should Hold File    0x0    ${BOOTLOADER_BIN}
    Flash Should Hold File    ${RECOVERY_SLOT}    ${RECOVERY_BIN}
    Flash Should Hold File    ${FIRMWARE_START}    ${RECOVERY_BIN}
    Firmware Should Be Alive

Remember The Old Start Of Flash
    [Documentation]           Everything the reloader may write in internal flash: the bootloader's pages, the slot table and the boot state page.
    ${start}=                 Execute Command    sysbus ReadBytes 0x0 ${OLD_START_LENGTH}
    Set Test Variable         ${OLD_START}    ${start}

Old System Still Starts
    [Documentation]           True when internal flash is as the old system left it: the reloader has not begun the window.
    ${start}=                 Execute Command    sysbus ReadBytes 0x0 ${OLD_START_LENGTH}
    RETURN    ${{ $start == $OLD_START }}

*** Test Cases ***
Moves A wasp-os Watch Onto NeoTime
    [Tags]                    wasp-os
    Start A Watch Running wasp-os With The Reloader As Its App

    Run For                   ${RELOADER_SECONDS}
    Flash Should Hold File    0x0    ${BOOTLOADER_BIN}
    Run For                   ${FIRST_BOOT_SECONDS}
    NeoTime Should Be Installed And Running
    # Nothing valid to roll back to after a migration, so the firmware confirms itself at once.
    Boot State Should Be      ${STATE_BOOTED}

Moves An InfiniTime Watch Onto NeoTime
    Start A Watch Running InfiniTime's Bootloader With The Reloader As Its App

    Run For                   ${RELOADER_SECONDS}
    Flash Should Hold File    0x0    ${BOOTLOADER_BIN}
    Run For                   ${FIRST_BOOT_SECONDS}
    NeoTime Should Be Installed And Running
    # Nothing valid to roll back to after a migration, so the firmware confirms itself at once.
    Boot State Should Be      ${STATE_BOOTED}

The Reloader Waits For The Charger
    Create A PineTime
    Copy File                 ${MCUBOOT_RELEASE}    ${MCUBOOT_BIN}
    Execute Command           sysbus LoadBinary @${MCUBOOT_BIN} 0x0
    Execute Command           sysbus LoadBinary @${INFINITIME_RELOADER} ${MCUBOOT_SLOT}

    Run For                   ${RELOADER_SECONDS}
    Flash Should Hold File    0x0    ${MCUBOOT_BIN}
    Execute Command           ${BATTERY} ConnectCharger
    Run For                   ${RELOADER_SECONDS}
    Flash Should Hold File    0x0    ${BOOTLOADER_BIN}

Holding The Button Leaves The Reloader For The Old Update Mode
    [Tags]                    wasp-os
    [Documentation]           Waiting for the charger, the reloader stops feeding the watchdog while the button is held, so the watchdog resets into wasp-os's bootloader, and nothing is changed.
    Start A Watch Running wasp-os With The Reloader As Its App, Unplugged
    Run For                   ${5}
    ${pc}=                    Execute Command    cpu PC
    Should Be True            ${WASP_APP_START} <= ${pc.strip()} < ${WASP_APP_END}    The reloader is not running

    # The watchdog's 5 seconds run out a second before this look. wasp-os's update mode does not stay
    # up in the emulator and starts the reloader again a few seconds later, so the look is made while held.
    Execute Command           ${BUTTON} Press
    Run For                   ${6}
    ${pc}=                    Execute Command    cpu PC
    Should Be True            ${WASP_APP_END} <= ${pc.strip()} < ${WASP_SETTINGS}    The watchdog did not reset into wasp-os's bootloader
    Execute Command           ${BUTTON} Release
    ${old}=                   Old System Still Starts
    Should Be True            ${old}    The reloader changed internal flash
