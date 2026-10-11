//! Links the reloader where the old system starts apps, and bakes in the images it carries.
//!
//! The payloads come from three environment variables: NEOTIME_BOOTLOADER_BIN and
//! NEOTIME_FIRMWARE_BIN are required, NEOTIME_RECOVERY_BIN is optional. Each is checked here, so a
//! wrong or oversized image fails the build rather than the watch.

use std::{env, fs, path::PathBuf};

use neotime_boot::{
    is_firmware_vector_table,
    migration::{crc32, PayloadSizes, CRC32_INITIAL},
    BOOTLOADER, FIRMWARE_ALIVE_MARKER, VECTOR_TABLE_CHECK_SIZE,
};

/// Where wasp-os's bootloader on S112 starts apps, up to where that bootloader itself begins.
const WASP_OS_APP_START: u32 = 0x19000;
const WASP_OS_APP_END: u32 = 0x78000;
/// Where InfiniTime's MCUBoot starts apps: after its 32-byte image header, up to the scratch area.
const INFINITIME_APP_START: u32 = 0x8020;
const INFINITIME_APP_END: u32 = 0x7C000 - 0x200;
/// The reloader keeps clear of the first words of RAM, which Nordic's MBR uses while it is still there.
const RELOADER_RAM_START: u32 = 0x2000_1000;
const VECTOR_TABLE_ALIGNMENT: u32 = 256;

fn main() {
    let output_directory = PathBuf::from(env::var("OUT_DIR").unwrap());

    let (flash_start, flash_end) = match (
        env::var_os("CARGO_FEATURE_FROM_WASP_OS"),
        env::var_os("CARGO_FEATURE_FROM_INFINITIME"),
    ) {
        (Some(_), None) => (WASP_OS_APP_START, WASP_OS_APP_END),
        (None, Some(_)) => (INFINITIME_APP_START, INFINITIME_APP_END),
        _ => panic!("build with exactly one of --features from-wasp-os or --features from-infinitime"),
    };
    // The old system reads the stack pointer and reset vector from the slot start, but the CPU wants the
    // vector table 256-byte aligned, so a two-word stub sits at the start and the table at the next
    // aligned address.
    let table_start = (flash_start / VECTOR_TABLE_ALIGNMENT + 1) * VECTOR_TABLE_ALIGNMENT;
    let memory_map = format!(
        "MEMORY\n{{\n  STUB : ORIGIN = {:#010x}, LENGTH = {:#x}\n  FLASH : ORIGIN = {:#010x}, LENGTH = {:#x}\n  RAM : ORIGIN = {:#010x}, LENGTH = {:#x}\n}}\n\
         SECTIONS\n{{\n  .entry_stub ORIGIN(STUB) : {{ KEEP(*(.entry_stub)) }} > STUB\n}} INSERT BEFORE .vector_table;\n",
        flash_start,
        table_start - flash_start,
        table_start,
        flash_end - table_start,
        RELOADER_RAM_START,
        FIRMWARE_ALIVE_MARKER.start - RELOADER_RAM_START,
    );
    fs::write(output_directory.join("memory.x"), memory_map).unwrap();
    println!("cargo:rustc-link-search={}", output_directory.display());

    let bootloader =
        payload("NEOTIME_BOOTLOADER_BIN").expect("NEOTIME_BOOTLOADER_BIN must name the bootloader .bin");
    let firmware = payload("NEOTIME_FIRMWARE_BIN").expect("NEOTIME_FIRMWARE_BIN must name the firmware .bin");
    let recovery = payload("NEOTIME_RECOVERY_BIN");

    check_bootloader(&bootloader.bytes);
    check_firmware(&firmware.bytes, "firmware");
    if let Some(recovery) = &recovery {
        check_firmware(&recovery.bytes, "recovery firmware");
    }
    let sizes = PayloadSizes {
        bootloader_in_bytes: bootloader.bytes.len() as u32,
        firmware_in_bytes: firmware.bytes.len() as u32,
        recovery_in_bytes: recovery.as_ref().map(|recovery| recovery.bytes.len() as u32),
    };
    if let Err(problem) = sizes.check() {
        panic!("a payload does not fit its slot: {problem:?}");
    }

    let mut source = String::new();
    source.push_str(&bootloader.as_rust("BOOTLOADER"));
    source.push_str(&firmware.as_rust("FIRMWARE"));
    match &recovery {
        Some(recovery) => source.push_str(&recovery.as_rust("RECOVERY")),
        None => source.push_str("pub const RECOVERY: Option<Payload> = None;\n"),
    }
    fs::write(output_directory.join("payloads.rs"), source).unwrap();

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../boot/src/lib.rs");
    println!("cargo:rerun-if-changed=../boot/src/migration.rs");
}

struct PayloadFile {
    path: PathBuf,
    bytes: Vec<u8>,
}

impl PayloadFile {
    /// Padded to whole words, which is what the chip's flash controller writes.
    fn as_rust(&self, name: &str) -> String {
        let padded_length = self.bytes.len().div_ceil(4) * 4;
        let option = if name == "RECOVERY" {
            "Option<Payload>"
        } else {
            "Payload"
        };
        let open = if name == "RECOVERY" { "Some(" } else { "" };
        let close = if name == "RECOVERY" { ")" } else { "" };
        format!(
            "pub const {name}: {option} = {open}Payload {{ bytes: {name}_BYTES, length_in_bytes: {}, crc32: {:#010x} }}{close};\n\
             #[repr(align(4))]\nstruct {name}Aligned([u8; {padded_length}]);\n\
             static {name}_ALIGNED: {name}Aligned = {name}Aligned(*include_bytes!({:?}));\n\
             static {name}_BYTES: &[u8] = &{name}_ALIGNED.0;\n",
            self.bytes.len(),
            crc32(CRC32_INITIAL, &self.bytes),
            self.padded_path(padded_length),
        )
    }

    fn padded_path(&self, padded_length: usize) -> PathBuf {
        let output_directory = PathBuf::from(env::var("OUT_DIR").unwrap());
        let padded_path = output_directory.join(format!(
            "{}.padded",
            self.path.file_name().unwrap().to_string_lossy()
        ));
        let mut padded = self.bytes.clone();
        padded.resize(padded_length, 0xFF);
        fs::write(&padded_path, padded).unwrap();
        padded_path
    }
}

fn payload(variable: &str) -> Option<PayloadFile> {
    println!("cargo:rerun-if-env-changed={variable}");
    let path = PathBuf::from(env::var_os(variable)?);
    println!("cargo:rerun-if-changed={}", path.display());
    let bytes = fs::read(&path).unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    Some(PayloadFile { path, bytes })
}

fn check_bootloader(bytes: &[u8]) {
    let reset_handler = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
    assert!(
        BOOTLOADER.contains(reset_handler & !1),
        "the bootloader image is not linked at {:#x}",
        BOOTLOADER.start
    );
}

fn check_firmware(bytes: &[u8], what: &str) {
    let start: [u8; VECTOR_TABLE_CHECK_SIZE] = bytes[..VECTOR_TABLE_CHECK_SIZE].try_into().unwrap();
    assert!(
        is_firmware_vector_table(start),
        "the {what} image does not start with a firmware vector table"
    );
}
