*** Settings ***
Documentation     The storage rehearsal exercises NeoTime's flash driver and littlefs on the external chip under wasp-os, and never writes internal flash.
Suite Setup       Setup
Suite Teardown    Teardown
Test Setup        Reset Emulation
Test Teardown     Test Teardown
Resource          neotime.resource
Library           OperatingSystem

*** Variables ***
${WASP_BOOTLOADER_HEX}    ${PROJECTS}/wasp-os/build-pinetime-s112/bootloader.hex
${REHEARSAL_BIN}          ${DIST}/storage-rehearsal-under-wasp-os.bin
${WASP_APP_START}         0x19000
${REHEARSAL_END}          0x71000
${WASP_SETTINGS}          0x7F000
${BANK_VALID_APP}         0x1
${BREADCRUMB_STAGE}       0x200039CC
${STAGE_ADVERTISING}      8
${FILE_SYSTEM}            ${{ 0x60000000 + 0x100000 }}
${NVMC_WRITES}            ${TEMPDIR}/neotime_storage_nvmc_writes.txt

*** Keywords ***
Start wasp-os's Bootloader With The Storage Rehearsal As Its App
    Create A PineTime
    Execute Command           sysbus LoadHEX @${WASP_BOOTLOADER_HEX}
    Execute Command           sysbus LoadBinary @${REHEARSAL_BIN} ${WASP_APP_START}
    ${size}=                  Evaluate    __import__('os').path.getsize('${REHEARSAL_BIN}')
    Execute Command           sysbus WriteDoubleWord ${WASP_SETTINGS} ${BANK_VALID_APP}
    Execute Command           sysbus WriteDoubleWord ${{ ${WASP_SETTINGS} + 8 }} ${size}
    Remove File               ${NVMC_WRITES}
    FOR    ${register}    IN    0x4001E504    0x4001E508    0x4001E50C    0x4001E514
        Execute Command       sysbus AddWatchpointHook ${register} DoubleWord Write "open('${NVMC_WRITES}', 'a').write('%x %x %x\\n' % (${register}, self.GetCurrentCPU().PC.RawValue, value))"
    END

Test Byte
    [Arguments]    ${offset}
    ${value}=                 Evaluate    ((${offset} * 31) ^ (${offset} >> 8) ^ 0x5A) & 0xFF
    RETURN    ${value}

*** Test Cases ***
Exercises The Flash And Files Then Advertises
    Start wasp-os's Bootloader With The Storage Rehearsal As Its App

    FOR    ${second}    IN RANGE    30
        Run For               1
        ${stage}=             Word At    ${BREADCRUMB_STAGE}
        IF    ${stage} == ${STAGE_ADVERTISING}    BREAK
    END
    Word Should Be            ${BREADCRUMB_STAGE}    ${STAGE_ADVERTISING}

    # The pattern survives past the first sector, which the test erases again at its end.
    FOR    ${offset}    IN    0x1000    0x1101    0x4FFF    0x7FFC
        ${byte}=              Execute Command    sysbus ReadByte ${{ ${UPDATE_SLOT} + ${offset} }}
        ${expected}=          Test Byte    ${offset}
        Should Be Equal As Integers    ${byte.strip()}    ${expected}    Byte at update slot offset ${offset}
    END
    ${first}=                 Word At    ${UPDATE_SLOT}
    Should Be Equal As Integers    ${first}    0xFFFFFFFF

    # littlefs keeps its magic in a superblock in one of the first two blocks.
    ${block0}=                Execute Command    sysbus ReadBytes ${FILE_SYSTEM} 0x40
    ${block1}=                Execute Command    sysbus ReadBytes ${{ ${FILE_SYSTEM} + 0x1000 }} 0x40
    ${bytes}=                 Evaluate    bytes.fromhex(''.join(__import__('re').findall(r'0x([0-9A-Fa-f]{2})', $block0 + $block1)))
    ${found}=                 Evaluate    b'littlefs' in $bytes
    Should Be True            ${found}    No littlefs superblock at the file system

    ${exists}=                Run Keyword And Return Status    File Should Exist    ${NVMC_WRITES}
    IF    ${exists}
        ${text}=              Get File    ${NVMC_WRITES}
        Log                   Flash controller writes (register, PC, value): ${text}    console=True
        ${inside}=            Evaluate    [line for line in $text.splitlines() if line and ${WASP_APP_START} <= int(line.split()[1], 16) < ${REHEARSAL_END}]
        Should Be Empty       ${inside}    The storage rehearsal's code wrote to the flash controller
    END
