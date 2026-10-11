*** Settings ***
Documentation     The reloader moves a watch from wasp-os or InfiniTime onto NeoTime, and a power cut at any point leaves a watch that starts.
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
${WASP_SETTINGS}          0x7F000
${BANK_VALID_APP}         0x1

${INFINITIME}             ${PROJECTS}/InfiniTime
# Copied to a plain name before loading, since Renode refuses the dots in the release file name.
${MCUBOOT_RELEASE}        ${INFINITIME}/bootloader/bootloader-5.0.4.bin
${MCUBOOT_BIN}            ${OUTPUT DIR}/mcuboot.bin
${INFINITIME_RELOADER}    ${DIST}/reloader-from-infinitime.mcuboot.bin
${MCUBOOT_SLOT}           0x8000

# How long the reloader needs, with the flash's real busy times, plus the first NeoTime boot.
${RELOADER_SECONDS}       40
${FIRST_BOOT_SECONDS}     30
# Power cuts are spread over the reloader's run in steps of this many seconds.
${CUT_STEP}               2

*** Keywords ***
Start A Watch Running wasp-os With The Reloader As Its App
    Create A PineTime
    Execute Command           sysbus LoadHEX @${WASP_BOOTLOADER_HEX}
    Execute Command           sysbus LoadBinary @${WASP_RELOADER_BIN} ${WASP_APP_START}
    # Bootloader settings: bank 0 holds a valid app whose CRC is not checked.
    ${size}=                  Evaluate    __import__('os').path.getsize('${WASP_RELOADER_BIN}')
    Execute Command           sysbus WriteDoubleWord ${WASP_SETTINGS} ${BANK_VALID_APP}
    Execute Command           sysbus WriteDoubleWord ${{ ${WASP_SETTINGS} + 8 }} ${size}
    Execute Command           ${BATTERY} ConnectCharger
    Clear The Alive Marker

Start A Watch Running InfiniTime's Bootloader With The Reloader As Its App
    Create A PineTime
    Copy File                 ${MCUBOOT_RELEASE}    ${MCUBOOT_BIN}
    Execute Command           sysbus LoadBinary @${MCUBOOT_BIN} 0x0
    Execute Command           sysbus LoadBinary @${INFINITIME_RELOADER} ${MCUBOOT_SLOT}
    Execute Command           ${BATTERY} ConnectCharger
    Clear The Alive Marker

NeoTime Should Be Installed And Running
    Flash Should Hold File    0x0    ${BOOTLOADER_BIN}
    Flash Should Hold File    ${UPDATE_SLOT}    ${FIRMWARE_BIN}
    Flash Should Hold File    ${RECOVERY_SLOT}    ${FIRMWARE_BIN}
    Flash Should Hold File    ${FIRMWARE_START}    ${FIRMWARE_BIN}
    Firmware Should Be Alive

Old System Still Starts
    [Documentation]           True when the old bootloader's first page is untouched: the reloader has not begun the window.
    [Arguments]    ${old_image}
    ${status}=                Flash Holds File    0x0    ${old_image}
    RETURN    ${status}

Classify After A Cut
    [Arguments]    ${old_image}
    ${old}=                   Old System Still Starts    ${old_image}
    ${new}=                   Flash Holds File    0x0    ${BOOTLOADER_BIN}
    IF    ${old}
        RETURN    old system
    ELSE IF    ${new}
        RETURN    neotime bootloader
    ELSE
        RETURN    nothing starts
    END

Cut The Power At Each Step And Classify
    [Arguments]    ${start keyword}    ${old_image}
    ${outcomes}=              Create List
    FOR    ${cut}    IN RANGE    ${CUT_STEP}    ${RELOADER_SECONDS}    ${CUT_STEP}
        Reset Emulation
        Run Keyword           ${start keyword}
        Run For               ${cut}
        Execute Command       powerClock PowerCycle
        ${outcome}=           Classify After A Cut    ${old_image}
        Append To List        ${outcomes}    ${cut}s: ${outcome}
        Log                   Cut at ${cut}s: ${outcome}    console=True
    END
    RETURN    ${outcomes}

No Cut May Leave A Watch That Does Not Start
    [Arguments]    ${outcomes}
    FOR    ${outcome}    IN    @{outcomes}
        Should Not Contain    ${outcome}    nothing starts
    END
    Log Many                  @{outcomes}

*** Test Cases ***
Moves A wasp-os Watch Onto NeoTime
    Start A Watch Running wasp-os With The Reloader As Its App

    Run For                   ${RELOADER_SECONDS}
    Flash Should Hold File    0x0    ${BOOTLOADER_BIN}
    Boot State Should Be      ${STATE_SWAP}
    Run For                   ${FIRST_BOOT_SECONDS}
    NeoTime Should Be Installed And Running

Moves An InfiniTime Watch Onto NeoTime
    Start A Watch Running InfiniTime's Bootloader With The Reloader As Its App

    Run For                   ${RELOADER_SECONDS}
    Flash Should Hold File    0x0    ${BOOTLOADER_BIN}
    Boot State Should Be      ${STATE_SWAP}
    Run For                   ${FIRST_BOOT_SECONDS}
    NeoTime Should Be Installed And Running

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

Power Cuts During The wasp-os Migration
    [Documentation]           Every cut must leave either the old system or NeoTime's bootloader able to start.
    [Tags]                    long
    ${outcomes}=              Cut The Power At Each Step And Classify    Start A Watch Running wasp-os With The Reloader As Its App    ${WASP_BOOTLOADER_HEX}
    No Cut May Leave A Watch That Does Not Start    ${outcomes}

Power Cuts During The InfiniTime Migration
    [Tags]                    long
    ${outcomes}=              Cut The Power At Each Step And Classify    Start A Watch Running InfiniTime's Bootloader With The Reloader As Its App    ${MCUBOOT_BIN}
    No Cut May Leave A Watch That Does Not Start    ${outcomes}
