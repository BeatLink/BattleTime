// The GATT services BattleTime serves besides DFU: Current Time, Battery, its own diagnostics and the
// rest it advertises, as docs/companion-protocol.md describes them and src/protocol/battletime.ts mirrors.

use chrono::{DateTime, Datelike, Local, Offset, TimeZone, Timelike};
use uuid::Uuid;

use super::link::{GattLink, WriteMode};
use crate::ble::{NUS_RX, NUS_SERVICE};

// ============================================================================
// Service and characteristic UUIDs
// ============================================================================

pub const CURRENT_TIME_SERVICE: Uuid = Uuid::from_u128(0x00001805_0000_1000_8000_00805f9b34fb);
pub const CURRENT_TIME: Uuid = Uuid::from_u128(0x00002a2b_0000_1000_8000_00805f9b34fb);
pub const LOCAL_TIME_INFORMATION: Uuid = Uuid::from_u128(0x00002a0f_0000_1000_8000_00805f9b34fb);

pub const BATTERY_SERVICE: Uuid = Uuid::from_u128(0x0000180f_0000_1000_8000_00805f9b34fb);
pub const BATTERY_LEVEL: Uuid = Uuid::from_u128(0x00002a19_0000_1000_8000_00805f9b34fb);

pub const BATTLETIME_SERVICE: Uuid = Uuid::from_u128(0x6e540001_0000_4000_8000_4e656f54696d);
pub const BATTLETIME_DIAGNOSTICS: Uuid = Uuid::from_u128(0x6e540002_0000_4000_8000_4e656f54696d);
pub const BATTLETIME_PACKAGES: Uuid = Uuid::from_u128(0x6e540003_0000_4000_8000_4e656f54696d);

pub const FILE_TRANSFER_SERVICE: Uuid = Uuid::from_u128(0x0000febb_0000_1000_8000_00805f9b34fb);
pub const FILE_TRANSFER: Uuid = Uuid::from_u128(0xadaf0200_4669_6c65_5472_616e73666572);

pub const LEGACY_DFU_SERVICE: Uuid = Uuid::from_u128(0x00001530_1212_efde_1523_785feabcd123);
pub const LEGACY_CONTROL_POINT: Uuid = Uuid::from_u128(0x00001531_1212_efde_1523_785feabcd123);

// ============================================================================
// What the connected watch can do
// ============================================================================

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub struct Capabilities {
    pub dfu: bool,
    pub time: bool,
    pub battery: bool,
    pub diagnostics: bool,
    // The BLEFS file service, for files, backups and installing apps.
    pub files: bool,
    // The packages characteristic, for installing, listing and removing apps.
    pub packages: bool,
    // The Nordic UART message channel.
    pub uart: bool,
}

pub fn read_capabilities(link: &impl GattLink) -> Capabilities {
    Capabilities {
        dfu: link.has_characteristic(LEGACY_DFU_SERVICE, LEGACY_CONTROL_POINT),
        time: link.has_characteristic(CURRENT_TIME_SERVICE, CURRENT_TIME),
        battery: link.has_characteristic(BATTERY_SERVICE, BATTERY_LEVEL),
        diagnostics: link.has_characteristic(BATTLETIME_SERVICE, BATTLETIME_DIAGNOSTICS),
        files: link.has_characteristic(FILE_TRANSFER_SERVICE, FILE_TRANSFER),
        packages: link.has_characteristic(BATTLETIME_SERVICE, BATTLETIME_PACKAGES),
        uart: link.has_characteristic(NUS_SERVICE, NUS_RX),
    }
}

// ============================================================================
// Current Time
// ============================================================================

// The ten-byte Current Time value in local time: year, month, day, hour, minute, second, weekday, fraction, reason.
pub fn encode_current_time<Zone: TimeZone>(now: &DateTime<Zone>) -> [u8; 10] {
    let year = now.year() as u16;
    let weekday = now.weekday().number_from_monday() as u8;
    let fraction = (now.timestamp_subsec_millis().min(999) * 256 / 1000) as u8;
    [
        year as u8,
        (year >> 8) as u8,
        now.month() as u8,
        now.day() as u8,
        now.hour() as u8,
        now.minute() as u8,
        now.second() as u8,
        weekday,
        fraction,
        1,
    ]
}

// The daylight saving offsets Local Time Information allows, in quarter hours.
const DAYLIGHT_SAVING_OFFSETS: [i32; 4] = [0, 2, 4, 8];

// The two-byte Local Time Information value from the zone's offsets now, in January and in July, in quarter hours.
pub fn encode_local_time_information(now: i32, january: i32, july: i32) -> [u8; 2] {
    let standard = january.min(july);
    let daylight_saving = now - standard;
    if DAYLIGHT_SAVING_OFFSETS.contains(&daylight_saving) {
        [standard as i8 as u8, daylight_saving as u8]
    } else {
        [now as i8 as u8, 0]
    }
}

fn quarter_hours_ahead_of_utc<Zone: TimeZone>(zone: &Zone, year: i32, month: u32) -> i32 {
    let moment = zone.with_ymd_and_hms(year, month, 1, 12, 0, 0).earliest();
    moment.map_or(0, |moment| moment.offset().fix().local_minus_utc() / 900)
}

// This computer's zone for the watch: its offset now, and in January and July of this year.
pub fn local_time_information(now: &DateTime<Local>) -> [u8; 2] {
    let offset = now.offset().fix().local_minus_utc() / 900;
    encode_local_time_information(
        offset,
        quarter_hours_ahead_of_utc(&Local, now.year(), 1),
        quarter_hours_ahead_of_utc(&Local, now.year(), 7),
    )
}

// Writes the zone first and then the local time, so the watch's UTC is right; firmware without 0x2A0F gets the time alone.
// BattleTime keeps a firmware update only once something like this has been exchanged after it.
pub async fn set_watch_time(link: &impl GattLink, now: &DateTime<Local>) -> Result<(), String> {
    if link.has_characteristic(CURRENT_TIME_SERVICE, LOCAL_TIME_INFORMATION) {
        link.write(
            CURRENT_TIME_SERVICE,
            LOCAL_TIME_INFORMATION,
            &local_time_information(now),
            WriteMode::Request,
        )
        .await?;
    }
    link.write(
        CURRENT_TIME_SERVICE,
        CURRENT_TIME,
        &encode_current_time(now),
        WriteMode::Request,
    )
    .await
}

// ============================================================================
// Battery and diagnostics
// ============================================================================

pub async fn read_battery_level(link: &impl GattLink) -> Result<u8, String> {
    Ok(link
        .read(BATTERY_SERVICE, BATTERY_LEVEL)
        .await?
        .first()
        .copied()
        .unwrap_or(0))
}

// The counters in the order the firmware's `Counter` enum lists them.
pub const DIAGNOSTIC_COUNTERS: [(&str, &str); 8] = [
    ("shellLoops", "Shell loops"),
    (
        "shellLastWake",
        "Last wake (1 touch, 2 button, 3 timer, 4 system)",
    ),
    ("screenLit", "Screen lit"),
    ("touchState", "Touch panel (1 started, 2 failed)"),
    ("touchReports", "Touch reports read"),
    ("touchesSent", "Touches sent to the shell"),
    ("buttonPresses", "Button presses"),
    (
        "accelerometerState",
        "Accelerometer (1 started, 2 failed, 3 read failed, 4 waiting for the shared library)",
    ),
];

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct DiagnosticCounter {
    pub key: &'static str,
    pub label: &'static str,
    pub value: u32,
}

// Little-endian 32-bit counters; the words on the last restart that follow them are the debug report's.
pub fn decode_diagnostics(bytes: &[u8]) -> Vec<DiagnosticCounter> {
    DIAGNOSTIC_COUNTERS
        .iter()
        .zip(bytes.as_chunks::<4>().0)
        .map(|((key, label), word)| DiagnosticCounter {
            key,
            label,
            value: u32::from_le_bytes(*word),
        })
        .collect()
}

pub async fn read_diagnostics(link: &impl GattLink) -> Result<Vec<DiagnosticCounter>, String> {
    Ok(decode_diagnostics(
        &link.read(BATTLETIME_SERVICE, BATTLETIME_DIAGNOSTICS).await?,
    ))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::link::tests::FakeWatch;
    use chrono::FixedOffset;

    #[test]
    fn the_current_time_is_local_with_a_monday_one_weekday_and_a_256th_fraction() {
        let zone = FixedOffset::east_opt(-5 * 3600).unwrap();
        let moment =
            zone.with_ymd_and_hms(2026, 10, 11, 23, 59, 58).unwrap() + chrono::Duration::milliseconds(500);
        assert_eq!(
            encode_current_time(&moment),
            [0xea, 0x07, 10, 11, 23, 59, 58, 7, 128, 1]
        );
    }

    #[test]
    fn local_time_information_splits_off_daylight_saving_where_it_can() {
        // New York in summer: standard -5 hours, one hour of daylight saving.
        assert_eq!(encode_local_time_information(-16, -20, -16), [(-20i8) as u8, 4]);
        // Jamaica keeps one offset all year.
        assert_eq!(encode_local_time_information(-20, -20, -20), [(-20i8) as u8, 0]);
        // An offset the field cannot split goes in whole.
        assert_eq!(encode_local_time_information(23, 20, 20), [23, 0]);
    }

    #[tokio::test]
    async fn the_zone_is_written_before_the_time_when_the_watch_has_it() {
        let watch = FakeWatch::serving(&[(CURRENT_TIME, vec![]), (LOCAL_TIME_INFORMATION, vec![])]);
        let now = Local::now();
        set_watch_time(&watch, &now).await.unwrap();
        let writes = watch.writes.lock().unwrap();
        assert_eq!(
            writes.iter().map(|(uuid, _, _)| *uuid).collect::<Vec<_>>(),
            [LOCAL_TIME_INFORMATION, CURRENT_TIME]
        );
        assert!(writes.iter().all(|(_, _, mode)| *mode == WriteMode::Request));
        assert_eq!(writes[1].1.len(), 10);
    }

    #[tokio::test]
    async fn older_firmware_gets_the_time_alone() {
        let watch = FakeWatch::serving(&[(CURRENT_TIME, vec![])]);
        set_watch_time(&watch, &Local::now()).await.unwrap();
        assert_eq!(watch.writes.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn capabilities_battery_and_diagnostics_come_from_what_the_watch_serves() {
        let mut counters = Vec::new();
        for value in [7u32, 2, 1, 1, 300, 12, 3, 4, 99] {
            counters.extend_from_slice(&value.to_le_bytes());
        }
        let watch = FakeWatch::serving(&[
            (BATTERY_LEVEL, vec![68]),
            (BATTLETIME_DIAGNOSTICS, counters),
            (BATTLETIME_PACKAGES, vec![]),
        ]);
        let capabilities = read_capabilities(&watch);
        assert!(capabilities.battery && capabilities.diagnostics && capabilities.packages);
        assert!(!capabilities.time && !capabilities.uart);
        assert_eq!(read_battery_level(&watch).await, Ok(68));
        let read = read_diagnostics(&watch).await.unwrap();
        assert_eq!(read.len(), 8);
        assert_eq!((read[0].key, read[0].value), ("shellLoops", 7));
        assert_eq!((read[7].key, read[7].value), ("accelerometerState", 4));
    }

    #[test]
    fn a_short_diagnostics_value_gives_only_the_counters_it_holds() {
        assert_eq!(decode_diagnostics(&[1, 0, 0, 0, 2, 0]).len(), 1);
    }
}
