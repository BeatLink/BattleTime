extern crate std;

use std::{vec, vec::Vec};

use embassy_futures::block_on;
use embedded_hal::i2c::{self, ErrorKind, Operation};

use super::*;

// ============================================================================
// Fake hardware
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq)]
struct FakeBusError;

impl i2c::Error for FakeBusError {
    fn kind(&self) -> ErrorKind {
        ErrorKind::Other
    }
}

#[derive(Debug, PartialEq)]
struct RegisterWrite {
    register: u8,
    value: u8,
}

/// A chip that keeps its registers, feature memory and uploaded file, and records every single write.
struct FakeChip {
    registers: [u8; 128],
    feature_configuration: [u8; FEATURE_CONFIGURATION_LENGTH],
    uploaded_file: Vec<u8>,
    writes: Vec<RegisterWrite>,
    rejects_configuration: bool,
    refuses_writes: bool,
}

impl FakeChip {
    fn with_chip_id(chip_id: u8) -> FakeChip {
        let mut registers = [0; 128];
        registers[CHIP_ID_REGISTER as usize] = chip_id;
        registers[POWER_CONFIG_REGISTER as usize] = POWER_SAVING_ON;

        FakeChip {
            registers,
            feature_configuration: [0; FEATURE_CONFIGURATION_LENGTH],
            uploaded_file: vec![0; 6144],
            writes: Vec::new(),
            rejects_configuration: false,
            refuses_writes: false,
        }
    }

    fn register(&self, register: u8) -> u8 {
        self.registers[register as usize]
    }

    fn receive_feature_bytes(&mut self, bytes: &[u8]) {
        let is_power_saving = self.register(POWER_CONFIG_REGISTER) & 0x01 != 0;
        assert!(
            !is_power_saving,
            "feature memory written while power saving was on"
        );

        if self.register(CONFIGURATION_LOAD_REGISTER) == CONFIGURATION_LOAD_STOPPED {
            let word_address = usize::from(self.register(UPLOAD_ADDRESS_HIGH_REGISTER)) << 4
                | usize::from(self.register(UPLOAD_ADDRESS_LOW_REGISTER));
            let start = word_address * 2;
            self.uploaded_file[start..start + bytes.len()].copy_from_slice(bytes);
        } else {
            self.feature_configuration.copy_from_slice(bytes);
        }
    }

    fn receive_register_write(&mut self, register: u8, value: u8) {
        self.writes.push(RegisterWrite { register, value });
        self.registers[register as usize] = value;

        if register == CONFIGURATION_LOAD_REGISTER && value == CONFIGURATION_LOAD_STARTED {
            let internal_status = if self.rejects_configuration { 0x02 } else { 0x01 };
            self.registers[INTERNAL_STATUS_REGISTER as usize] = internal_status;
        }
    }
}

impl I2cErrorType for FakeChip {
    type Error = FakeBusError;
}

impl I2c for FakeChip {
    async fn transaction(
        &mut self,
        address: u8,
        operations: &mut [Operation<'_>],
    ) -> Result<(), Self::Error> {
        assert_eq!(address, I2C_ADDRESS);

        match operations {
            [Operation::Write([register, values @ ..])] => {
                if self.refuses_writes {
                    return Err(FakeBusError);
                }
                if *register == FEATURE_CONFIGURATION_REGISTER {
                    self.receive_feature_bytes(values);
                } else {
                    let [value] = values else {
                        panic!("multi-byte write to register {register:#04x}");
                    };
                    self.receive_register_write(*register, *value);
                }
            }
            [Operation::Write(&[FEATURE_CONFIGURATION_REGISTER]), Operation::Read(buffer)] => {
                buffer.copy_from_slice(&self.feature_configuration);
            }
            [Operation::Write(&[register]), Operation::Read(buffer)] => {
                let start = register as usize;
                buffer.copy_from_slice(&self.registers[start..start + buffer.len()]);
            }
            _ => panic!("unexpected I2C transaction"),
        }

        Ok(())
    }
}

#[derive(Default)]
struct FakeDelay {
    total_nanoseconds: u64,
}

impl DelayNs for FakeDelay {
    async fn delay_ns(&mut self, nanoseconds: u32) {
        self.total_nanoseconds += nanoseconds as u64;
    }
}

fn started_accelerometer(chip: FakeChip, settings: Settings) -> Accelerometer<FakeChip> {
    let mut accelerometer = Accelerometer::new(chip);
    block_on(accelerometer.start(settings, &mut FakeDelay::default(), BOTH_FILES)).unwrap();
    accelerometer
}

/// A BMA425 holding one reading, given in 12-bit steps along the chip's own axes.
fn chip_reading(chip_x: i16, chip_y: i16, chip_z: i16) -> FakeChip {
    let [x_low, x_high] = (chip_x * 16).to_le_bytes();
    let [y_low, y_high] = (chip_y * 16).to_le_bytes();
    let [z_low, z_high] = (chip_z * 16).to_le_bytes();

    let mut chip = FakeChip::with_chip_id(BMA425_CHIP_ID);
    chip.registers[ACCELERATION_REGISTER as usize..][..ACCELERATION_LENGTH]
        .copy_from_slice(&[x_low, x_high, y_low, y_high, z_low, z_high]);
    chip
}

/// Both chips' files, as the shared library gives them.
const BOTH_FILES: Configurations<'static> = Configurations {
    bma421: Some(BMA421_CONFIGURATION_FILE),
    bma425: Some(BMA425_CONFIGURATION_FILE),
};

// ============================================================================
// Tests
// ============================================================================

#[test]
fn new_does_not_talk_to_the_chip() {
    let accelerometer = Accelerometer::new(FakeChip::with_chip_id(BMA425_CHIP_ID));

    let chip = accelerometer.release();

    assert!(chip.writes.is_empty());
}

#[test]
fn start_resets_the_chip_first() {
    let accelerometer = started_accelerometer(FakeChip::with_chip_id(BMA425_CHIP_ID), Settings::DEFAULT);

    let chip = accelerometer.release();

    assert_eq!(
        chip.writes[0],
        RegisterWrite {
            register: COMMAND_REGISTER,
            value: SOFT_RESET
        }
    );
}

#[test]
fn start_uploads_the_bma425_configuration_file_whole() {
    let mut accelerometer = Accelerometer::new(FakeChip::with_chip_id(BMA425_CHIP_ID));

    let chip_model =
        block_on(accelerometer.start(Settings::DEFAULT, &mut FakeDelay::default(), BOTH_FILES)).unwrap();

    assert_eq!(chip_model, ChipModel::Bma425);
    assert_eq!(accelerometer.release().uploaded_file, BMA425_CONFIGURATION_FILE);
}

#[test]
fn start_uploads_the_bma421_configuration_file_whole() {
    let mut accelerometer = Accelerometer::new(FakeChip::with_chip_id(BMA421_CHIP_ID));

    let chip_model =
        block_on(accelerometer.start(Settings::DEFAULT, &mut FakeDelay::default(), BOTH_FILES)).unwrap();

    assert_eq!(chip_model, ChipModel::Bma421);
    assert_eq!(accelerometer.release().uploaded_file, BMA421_CONFIGURATION_FILE);
}

#[test]
fn start_without_the_chips_file_refuses_and_uploads_nothing() {
    let mut accelerometer = Accelerometer::new(FakeChip::with_chip_id(BMA425_CHIP_ID));
    let only_bma421 = Configurations {
        bma421: Some(BMA421_CONFIGURATION_FILE),
        bma425: None,
    };

    let result = block_on(accelerometer.start(Settings::DEFAULT, &mut FakeDelay::default(), only_bma421));

    assert_eq!(
        result,
        Err(Error::NoConfiguration {
            chip_model: ChipModel::Bma425
        })
    );
    assert!(accelerometer
        .release()
        .uploaded_file
        .iter()
        .all(|&byte| byte == 0));
}

#[test]
fn each_chip_has_its_own_configuration_file() {
    assert_eq!(BMA421_CONFIGURATION_FILE.len(), 6144);
    assert_eq!(BMA425_CONFIGURATION_FILE.len(), 6144);
    assert_ne!(BMA421_CONFIGURATION_FILE, BMA425_CONFIGURATION_FILE);
}

#[test]
fn start_turns_on_measuring_step_counting_and_power_saving() {
    let accelerometer = started_accelerometer(FakeChip::with_chip_id(BMA425_CHIP_ID), Settings::DEFAULT);

    let chip = accelerometer.release();

    assert_eq!(chip.register(POWER_CONTROL_REGISTER), ACCELEROMETER_ON);
    assert_eq!(chip.register(ACCELERATION_CONFIG_REGISTER), 0x28);
    assert_eq!(chip.register(ACCELERATION_RANGE_REGISTER), 0x00);
    assert_eq!(chip.register(INTERFACE_WATCHDOG_REGISTER), 0x06);
    assert_eq!(
        chip.feature_configuration[STEP_COUNTER_CONTROL_OFFSET],
        STEP_COUNTER_ON
    );
    assert_eq!(chip.register(POWER_CONFIG_REGISTER), POWER_SAVING_ON);
}

#[test]
fn start_waits_for_the_reset_and_the_configuration_load() {
    let mut accelerometer = Accelerometer::new(FakeChip::with_chip_id(BMA425_CHIP_ID));
    let mut delay = FakeDelay::default();

    block_on(accelerometer.start(Settings::DEFAULT, &mut delay, BOTH_FILES)).unwrap();

    assert_eq!(
        delay.total_nanoseconds,
        50_000_000 + 450_000 + 150_000_000 + 2 * 450_000
    );
}

#[test]
fn start_rejects_an_unknown_chip() {
    let mut accelerometer = Accelerometer::new(FakeChip::with_chip_id(0x42));

    let result = block_on(accelerometer.start(Settings::DEFAULT, &mut FakeDelay::default(), BOTH_FILES));

    assert_eq!(result, Err(Error::UnknownChip { chip_id: 0x42 }));
}

#[test]
fn start_reports_a_configuration_the_chip_did_not_accept() {
    let mut chip = FakeChip::with_chip_id(BMA421_CHIP_ID);
    chip.rejects_configuration = true;
    let mut accelerometer = Accelerometer::new(chip);

    let result = block_on(accelerometer.start(Settings::DEFAULT, &mut FakeDelay::default(), BOTH_FILES));

    assert_eq!(
        result,
        Err(Error::ConfigurationRejected {
            internal_status: 0x02
        })
    );
    assert_eq!(accelerometer.release().register(POWER_CONTROL_REGISTER), 0);
}

#[test]
fn a_bus_error_is_passed_on() {
    let mut chip = FakeChip::with_chip_id(BMA425_CHIP_ID);
    chip.refuses_writes = true;
    let mut accelerometer = Accelerometer::new(chip);

    let result = block_on(accelerometer.start(Settings::DEFAULT, &mut FakeDelay::default(), BOTH_FILES));

    assert_eq!(result, Err(Error::Bus(FakeBusError)));
}

#[test]
fn the_step_count_is_read_as_four_little_endian_bytes() {
    let mut chip = FakeChip::with_chip_id(BMA425_CHIP_ID);
    chip.registers[STEP_COUNT_REGISTER as usize..][..STEP_COUNT_LENGTH]
        .copy_from_slice(&[0x39, 0x30, 0x01, 0x00]);
    let mut accelerometer = started_accelerometer(chip, Settings::DEFAULT);

    let step_count = block_on(accelerometer.read_step_count()).unwrap();

    assert_eq!(step_count, 77_881);
}

#[test]
fn resetting_the_step_count_keeps_the_counter_on() {
    let mut accelerometer = started_accelerometer(FakeChip::with_chip_id(BMA425_CHIP_ID), Settings::DEFAULT);

    block_on(accelerometer.reset_step_count(&mut FakeDelay::default())).unwrap();

    let chip = accelerometer.release();
    assert_eq!(
        chip.feature_configuration[STEP_COUNTER_CONTROL_OFFSET],
        STEP_COUNTER_ON | STEP_COUNT_RESET
    );
    assert_eq!(chip.register(POWER_CONFIG_REGISTER), POWER_SAVING_ON);
}

#[test]
fn acceleration_comes_in_the_chip_axes_when_asked() {
    let settings = Settings {
        orientation: Orientation::CHIP,
        ..Settings::DEFAULT
    };
    let chip = chip_reading(1024, -512, 2047);
    let mut accelerometer = started_accelerometer(chip, settings);

    let acceleration = block_on(accelerometer.read_acceleration()).unwrap();

    assert_eq!(
        acceleration,
        Acceleration {
            x_in_milli_g: 1000,
            y_in_milli_g: -500,
            z_in_milli_g: 1999,
        }
    );
}

#[test]
fn acceleration_is_turned_to_the_pinetime_axes_by_default() {
    let chip = chip_reading(100, 200, -1024);
    let mut accelerometer = started_accelerometer(chip, Settings::DEFAULT);

    let acceleration = block_on(accelerometer.read_acceleration()).unwrap();

    assert_eq!(
        acceleration,
        Acceleration {
            x_in_milli_g: 195,
            y_in_milli_g: -97,
            z_in_milli_g: -1000,
        }
    );
}

#[test]
fn a_wider_range_scales_each_reading_up() {
    let settings = Settings {
        range: Range::EightG,
        orientation: Orientation::CHIP,
    };
    let chip = chip_reading(256, -2048, 0);
    let mut accelerometer = started_accelerometer(chip, settings);

    let acceleration = block_on(accelerometer.read_acceleration()).unwrap();

    assert_eq!(acceleration.x_in_milli_g, 1000);
    assert_eq!(acceleration.y_in_milli_g, -8000);
    assert_eq!(
        accelerometer.release().register(ACCELERATION_RANGE_REGISTER),
        0x02
    );
}

// ============================================================================
// Shakes
// ============================================================================

const SHAKE_READING_PERIOD_IN_MILLISECONDS: u64 = 100;

fn reading(x_in_milli_g: i16, z_in_milli_g: i16) -> Acceleration {
    Acceleration {
        x_in_milli_g,
        y_in_milli_g: 0,
        z_in_milli_g,
    }
}

/// Feeds readings a tenth of a second apart from `start`, and gives the times a shake was found at.
fn shakes_found(detector: &mut shake::ShakeDetector, start: u64, readings: &[Acceleration]) -> Vec<u64> {
    let mut found = Vec::new();
    for (place, reading) in readings.iter().enumerate() {
        let time = start + place as u64 * SHAKE_READING_PERIOD_IN_MILLISECONDS;
        if detector.feed(*reading, time) {
            found.push(time);
        }
    }
    found
}

/// The wrist swung back and forth: the x axis jumps between two g either way.
fn swings(count: usize) -> Vec<Acceleration> {
    (0..count)
        .map(|place| reading(if place % 2 == 0 { -1000 } else { 1000 }, 0))
        .collect()
}

#[test]
fn three_jolts_within_a_second_make_one_shake() {
    let mut detector = shake::ShakeDetector::new();

    let found = shakes_found(&mut detector, 0, &swings(4));

    assert_eq!(found, vec![300]);
}

#[test]
fn a_still_or_gently_moving_watch_never_shakes() {
    let mut detector = shake::ShakeDetector::new();
    let gentle: Vec<Acceleration> = (0..50).map(|place| reading((place % 5) * 200, 1000)).collect();

    assert!(shakes_found(&mut detector, 0, &gentle).is_empty());
}

#[test]
fn a_long_shake_counts_once_a_second_at_most() {
    let mut detector = shake::ShakeDetector::new();

    let found = shakes_found(&mut detector, 0, &swings(30));

    assert_eq!(found.len(), 3, "{found:?}");
    assert!(found.windows(2).all(|pair| pair[1] - pair[0] > 1000), "{found:?}");
}

#[test]
fn jolts_further_apart_than_the_window_do_not_add_up() {
    let mut detector = shake::ShakeDetector::new();
    let mut readings = Vec::new();
    for level in [1000, -1000, 1000, -1000, 1000] {
        readings.extend([reading(level, 0); 12]);
    }

    assert!(shakes_found(&mut detector, 0, &readings).is_empty());
}

#[test]
fn readings_after_a_gap_are_not_compared_with_the_one_before() {
    let mut detector = shake::ShakeDetector::new();
    assert!(!detector.feed(reading(-1000, 0), 0));
    assert!(!detector.feed(reading(1000, 0), 100));
    assert!(!detector.feed(reading(-1000, 0), 200));

    assert!(!detector.feed(reading(1000, 0), 2000));

    detector.reset();
    assert!(!detector.feed(reading(-1000, 0), 2100));
    assert!(!detector.feed(reading(1000, 0), 2200));
}
