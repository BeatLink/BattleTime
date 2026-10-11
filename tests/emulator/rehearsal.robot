*** Settings ***
Documentation     The rehearsal runs as a wasp-os app, writes nothing to either flash, and the button hold returns to wasp-os's update mode.
Suite Setup       Setup
Suite Teardown    Teardown
Test Setup        Reset Emulation
Test Teardown     Test Teardown
Resource          neotime.resource
Library           OperatingSystem

*** Variables ***
${WASP_BOOTLOADER_HEX}    ${PROJECTS}/wasp-os/build-pinetime-s112/bootloader.hex
${REHEARSAL_BIN}          ${DIST}/rehearsal-under-wasp-os.bin
${WASP_APP_START}         0x19000
${REHEARSAL_END}          0x71000
${WASP_SETTINGS}          0x7F000
${BANK_VALID_APP}         0x1
${FAULT_STATUS}           0xE000ED28
${NVMC_WRITES}            ${OUTPUT DIR}/nvmc_writes.txt

*** Keywords ***
Start wasp-os's Bootloader With The Rehearsal As Its App
    Create A PineTime
    Execute Command           sysbus LoadHEX @${WASP_BOOTLOADER_HEX}
    Execute Command           sysbus LoadBinary @${REHEARSAL_BIN} ${WASP_APP_START}
    ${size}=                  Evaluate    __import__('os').path.getsize('${REHEARSAL_BIN}')
    Execute Command           sysbus WriteDoubleWord ${WASP_SETTINGS} ${BANK_VALID_APP}
    Execute Command           sysbus WriteDoubleWord ${{ ${WASP_SETTINGS} + 8 }} ${size}
    Clear The Alive Marker

Snapshot Of Both Flashes
    [Documentation]           A digest per region, so a change says where it happened.
    ${internal}=              Execute Command    sysbus ReadBytes 0x0 0x80000
    ${uicr}=                  Execute Command    sysbus ReadBytes 0x10001000 0x1000
    ${external}=              Execute Command    sysbus ReadBytes ${EXTERNAL_FLASH} 0x400000
    ${digests}=               Evaluate    {name: __import__('hashlib').sha256(text.encode()).hexdigest()[:16] for name, text in (('internal', $internal), ('uicr', $uicr), ('external', $external))}
    RETURN    ${digests}

Watch The Flash Controller
    [Documentation]           Records the CPU's position on every write to the flash controller's enable and erase registers.
    Remove File               ${NVMC_WRITES}
    FOR    ${register}    IN    0x4001E504    0x4001E508    0x4001E50C    0x4001E514
        Execute Command       sysbus AddWatchpointHook ${register} DoubleWord Write "open('${NVMC_WRITES}', 'a').write('%x %x %x\\\\n' % (${register}, self.GetCurrentCPU().PC.RawValue, value))"
    END

Flash Controller Writes From The Rehearsal
    ${exists}=                Run Keyword And Return Status    File Should Exist    ${NVMC_WRITES}
    IF    not ${exists}    RETURN    ${{ [] }}
    ${text}=                  Get File    ${NVMC_WRITES}
    Log                       Flash controller writes (register, PC, value): ${text}    console=True
    ${inside}=                Evaluate    [line for line in $text.splitlines() if line and ${WASP_APP_START} <= int(line.split()[1], 16) < ${REHEARSAL_END}]
    RETURN    ${inside}

Run Until The Rehearsal Is Up
    FOR    ${tenth}    IN RANGE    100
        Run For               0.1
        ${marker}=            Word At    ${ALIVE_MARKER}
        IF    ${marker} == ${ALIVE_MAGIC}    BREAK
    END
    Firmware Should Be Alive
    # Its own start-up, Bluetooth included, is over well within a second.
    Run For                   1

*** Test Cases ***
Starts Under wasp-os's Bootloader
    Start wasp-os's Bootloader With The Rehearsal As Its App

    Run For                   5
    Firmware Should Be Alive
    Word Should Be            ${FAULT_STATUS}    0

Writes Nothing To Either Flash
    [Documentation]           No flash controller write comes from the rehearsal's code at any time, and the flash is unchanged once it runs.
    Start wasp-os's Bootloader With The Rehearsal As Its App
    Watch The Flash Controller
    Run Until The Rehearsal Is Up
    ${before}=                Snapshot Of Both Flashes

    Run For                   10
    Firmware Should Be Alive
    ${after}=                 Snapshot Of Both Flashes
    Should Be Equal           ${before}    ${after}    The rehearsal changed the flash
    ${from_rehearsal}=        Flash Controller Writes From The Rehearsal
    Should Be Empty           ${from_rehearsal}    The rehearsal's code wrote to the flash controller

Leaves The Settings wasp-os's Bootloader Wrote
    [Documentation]           The reset pin and NFC pin words, which embassy's start-up would write if they differed.
    Start wasp-os's Bootloader With The Rehearsal As Its App
    Run Until The Rehearsal Is Up
    Word Should Be            0x10001200    21
    Word Should Be            0x10001204    21
    ${nfc}=                   Word At    0x1000120C
    Should Be Equal As Integers    ${{ ${nfc} & 1 }}    0

Holding The Button Returns To wasp-os's Update Mode
    Start wasp-os's Bootloader With The Rehearsal As Its App
    Run For                   5
    Firmware Should Be Alive

    Execute Command           ${BUTTON} Press
    Run For                   7
    Execute Command           ${BUTTON} Release
    Run For                   3
    ${count}=                 Word At    ${BOOT_COUNT}
    Should Be Equal As Integers    ${count}    1    The rehearsal started again instead of wasp-os's update mode
