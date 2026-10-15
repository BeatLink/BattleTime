*** Settings ***
Documentation     While the firmware advertises, the Bluetooth controller lets the 64 MHz crystal sleep between radio events.
Suite Setup       Setup
Suite Teardown    Teardown
Test Setup        Reset Emulation
Test Teardown     Test Teardown
Resource          battletime.resource

*** Variables ***
${HFCLKSTAT}              0x4000040C
${CRYSTAL_RUNNING}        0x10000
${SAMPLES}                100
# Not a divisor of the advertising interval, so the samples fall all over it.
${SAMPLE_GAP}             0.0073

*** Keywords ***
Crystal Is Running
    ${status}=                Word At    ${HFCLKSTAT}
    RETURN    ${{ bool(${status} & ${CRYSTAL_RUNNING}) }}

*** Test Cases ***
Crystal Sleeps Between Advertisements
    Create A PineTime
    Load Bootloader And Firmware
    Clear The Alive Marker
    Run For                   3
    Firmware Should Be Alive

    ${running}=               Set Variable    ${0}
    FOR    ${sample}    IN RANGE    ${SAMPLES}
        Run For               ${SAMPLE_GAP}
        ${is_running}=        Crystal Is Running
        ${running}=           Evaluate    ${running} + int(${is_running})
    END
    Log                       Crystal running in ${running} of ${SAMPLES} samples    console=True
    # An advertising event keeps it on for about 4 ms in every 160 ms.
    Should Be True            ${running} < ${SAMPLES} / 5    The crystal ran in ${running} of ${SAMPLES} samples
    Firmware Should Be Alive
