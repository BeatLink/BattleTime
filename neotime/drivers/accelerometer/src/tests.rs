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
    assert_eq!(chip_model.chip_id(), BMA425_CHIP_ID);
    assert_eq!(accelerometer.release().uploaded_file, BMA425_CONFIGURATION_FILE);
}

#[test]
fn start_uploads_the_bma421_configuration_file_whole() {
    let mut accelerometer = Accelerometer::new(FakeChip::with_chip_id(BMA421_CHIP_ID));

    let chip_model =
        block_on(accelerometer.start(Settings::DEFAULT, &mut FakeDelay::default(), BOTH_FILES)).unwrap();

    assert_eq!(chip_model, ChipModel::Bma421);
    assert_eq!(chip_model.chip_id(), BMA421_CHIP_ID);
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

// ============================================================================
// Wake gestures
// ============================================================================

const WAKE_READING_PERIOD_IN_MILLISECONDS: u64 = 100;

const WATCHING_EVERYTHING: wake_gestures::Watching = wake_gestures::Watching {
    raise: true,
    lower: true,
    shake: true,
    shake_threshold: 150,
};

const WATCHING_ONLY_SHAKES: wake_gestures::Watching = wake_gestures::Watching {
    raise: false,
    lower: false,
    ..WATCHING_EVERYTHING
};

/// What the detector watches for while the screen is lit.
const WATCHING_WHILE_LIT: wake_gestures::Watching = wake_gestures::Watching {
    lower: true,
    ..wake_gestures::Watching::NOTHING
};

/// A reading given in InfiniTime's units and axes: 1024 to a g, with y reversed.
fn infinitime_reading(x: i32, y: i32, z: i32) -> Acceleration {
    let to_milli_g = |value: i32| (value * 1000 / 1024) as i16;
    Acceleration {
        x_in_milli_g: to_milli_g(x),
        y_in_milli_g: -to_milli_g(y),
        z_in_milli_g: to_milli_g(z),
    }
}

/// Feeds readings a tenth of a second apart from `start`, and gives each gesture found with its time.
fn gestures_found(
    detector: &mut wake_gestures::WakeGestureDetector,
    start: u64,
    readings: &[Acceleration],
    watching: wake_gestures::Watching,
) -> Vec<(u64, wake_gestures::WakeGesture)> {
    let mut found = Vec::new();
    for (place, reading) in readings.iter().enumerate() {
        let time = start + place as u64 * WAKE_READING_PERIOD_IN_MILLISECONDS;
        if let Some(gesture) = detector.feed(*reading, time, watching) {
            found.push((time, gesture));
        }
    }
    found
}

/// The watch lying face up, then turned towards the wearer's eyes over a few readings.
fn raise() -> Vec<Acceleration> {
    let lying = infinitime_reading(0, 0, -1024);
    let turning = infinitime_reading(0, -400, -900);
    let raised = infinitime_reading(0, -800, -640);
    vec![lying, lying, lying, turning, turning, turning, raised, raised]
}

/// The arm let fall to the side, so the watch's y axis comes to point down.
fn lowering() -> Vec<Acceleration> {
    let tilted = infinitime_reading(0, 300, -980);
    let hanging = infinitime_reading(0, 1000, -200);
    vec![tilted, tilted, tilted, tilted, tilted, tilted, hanging, hanging]
}

#[test]
fn raising_the_wrist_wakes_the_screen() {
    let mut detector = wake_gestures::WakeGestureDetector::new();

    let found = gestures_found(&mut detector, 0, &raise(), WATCHING_EVERYTHING);

    assert_eq!(found, vec![(700, wake_gestures::WakeGesture::Raise)]);
}

#[test]
fn a_raise_is_not_reported_unless_watched_for() {
    let mut detector = wake_gestures::WakeGestureDetector::new();
    let watching = wake_gestures::Watching {
        raise: false,
        ..WATCHING_EVERYTHING
    };

    assert!(gestures_found(&mut detector, 0, &raise(), watching).is_empty());
}

#[test]
fn lowering_the_arm_puts_the_screen_to_sleep() {
    let mut detector = wake_gestures::WakeGestureDetector::new();

    let found = gestures_found(&mut detector, 0, &lowering(), WATCHING_WHILE_LIT);

    assert_eq!(found, vec![(700, wake_gestures::WakeGesture::Lower)]);
}

#[test]
fn a_lowered_arm_that_was_not_held_down_first_does_not_sleep() {
    let mut detector = wake_gestures::WakeGestureDetector::new();
    let mut readings = lowering();
    readings[3] = infinitime_reading(0, 100, -1000);

    assert!(gestures_found(&mut detector, 0, &readings, WATCHING_WHILE_LIT).is_empty());
}

#[test]
fn rolling_the_watch_sideways_puts_the_screen_to_sleep() {
    let mut detector = wake_gestures::WakeGestureDetector::new();
    let flat = infinitime_reading(300, 0, -980);
    let on_its_side = infinitime_reading(1000, 0, -200);
    let readings = [flat, flat, flat, flat, flat, flat, on_its_side, on_its_side];

    let found = gestures_found(&mut detector, 0, &readings, WATCHING_WHILE_LIT);

    assert_eq!(found, vec![(700, wake_gestures::WakeGesture::Lower)]);
}

#[test]
fn a_still_watch_makes_no_gesture() {
    let mut detector = wake_gestures::WakeGestureDetector::new();
    let readings = [infinitime_reading(0, -300, -980); 20];

    assert!(gestures_found(&mut detector, 0, &readings, WATCHING_EVERYTHING).is_empty());
    assert_eq!(detector.shake_speed(), 0);
}

#[test]
fn a_shake_past_the_threshold_wakes_the_screen() {
    let mut detector = wake_gestures::WakeGestureDetector::new();
    let readings: Vec<Acceleration> = (0..4)
        .map(|place| infinitime_reading(0, 0, if place % 2 == 0 { -1000 } else { 1000 }))
        .collect();
    let watching = WATCHING_ONLY_SHAKES;

    let found = gestures_found(&mut detector, 0, &readings, watching);

    assert_eq!(found, vec![(100, wake_gestures::WakeGesture::Shake)]);
    assert!(detector.shake_speed() > 150);
}

#[test]
fn a_gentle_movement_stays_under_the_shake_threshold() {
    let mut detector = wake_gestures::WakeGestureDetector::new();
    let readings: Vec<Acceleration> = (0..20)
        .map(|place| infinitime_reading(0, 0, -1000 + (place % 2) * 100))
        .collect();
    let watching = WATCHING_ONLY_SHAKES;

    assert!(gestures_found(&mut detector, 0, &readings, watching).is_empty());
    assert!(detector.shake_speed() < 150);
}

#[test]
fn a_gesture_counts_once_then_the_detector_waits_a_second() {
    let mut detector = wake_gestures::WakeGestureDetector::new();
    let readings: Vec<Acceleration> = (0..30)
        .map(|place| infinitime_reading(0, 0, if place % 2 == 0 { -1000 } else { 1000 }))
        .collect();
    let watching = WATCHING_ONLY_SHAKES;

    let found = gestures_found(&mut detector, 0, &readings, watching);

    let times: Vec<u64> = found.iter().map(|(time, _)| *time).collect();
    assert_eq!(times, vec![100, 1100, 2100]);
}

#[test]
fn readings_after_a_long_gap_start_the_history_afresh() {
    let mut detector = wake_gestures::WakeGestureDetector::new();
    let readings = raise();
    gestures_found(&mut detector, 0, &readings[..6], wake_gestures::Watching::NOTHING);

    let found = gestures_found(&mut detector, 5_000, &readings[6..], WATCHING_EVERYTHING);

    assert!(found.is_empty());
    assert_eq!(detector.shake_speed(), 0);
}

#[test]
fn resetting_forgets_the_shake_speed() {
    let mut detector = wake_gestures::WakeGestureDetector::new();
    gestures_found(
        &mut detector,
        0,
        &[infinitime_reading(0, 0, -1000), infinitime_reading(0, 0, 1000)],
        wake_gestures::Watching::NOTHING,
    );
    assert!(detector.shake_speed() > 0);

    detector.reset();

    assert_eq!(detector.shake_speed(), 0);
}

#[test]
fn watching_nothing_is_told_apart_from_watching_a_gesture() {
    assert!(!wake_gestures::Watching::NOTHING.anything());
    assert!(WATCHING_EVERYTHING.anything());
}

#[test]
fn the_arcsine_gives_whole_degrees_either_way() {
    assert_eq!(wake_gestures::arcsine_in_degrees(0), 0);
    assert_eq!(wake_gestures::arcsine_in_degrees(32767), 90);
    assert_eq!(wake_gestures::arcsine_in_degrees(16383), 30);
    assert_eq!(wake_gestures::arcsine_in_degrees(-23170), -45);
    assert_eq!(wake_gestures::arcsine_in_degrees(16700), 31);
    assert_eq!(wake_gestures::arcsine_in_degrees(40000), 90);
}

#[test]
fn the_roll_follows_infinitimes_cases_for_each_side_of_the_watch() {
    let half_g = 512;

    assert_eq!(wake_gestures::degrees_rolled(half_g, -1, 0, -1), 30);
    assert_eq!(wake_gestures::degrees_rolled(half_g, 1, 0, 1), -30);
    assert_eq!(wake_gestures::degrees_rolled(half_g, 1, 0, -1), 150);
    assert_eq!(wake_gestures::degrees_rolled(-half_g, 1, 0, -1), -150);
    assert_eq!(wake_gestures::degrees_rolled(half_g, -1, 0, 1), -150);
    assert_eq!(wake_gestures::degrees_rolled(-half_g, -1, 0, 1), 150);
}
