use super::*;

// ============================================================================
// A stand-in converter
// ============================================================================

/// Raises each task's event at once and writes the next of its readings on every sample.
struct FakeSaadc {
    registers: [u32; 0x200],
    result: *mut i16,
    readings: [i16; 4],
    samples_taken: usize,
    /// A task whose event never comes.
    silent_task: Option<u32>,
    writes: [(u32, u32); 64],
    write_count: usize,
}

impl FakeSaadc {
    fn new(readings: [i16; 4]) -> FakeSaadc {
        FakeSaadc {
            registers: [0; 0x200],
            result: core::ptr::null_mut(),
            readings,
            samples_taken: 0,
            silent_task: None,
            writes: [(0, 0); 64],
            write_count: 0,
        }
    }

    fn written(&self) -> &[(u32, u32)] {
        &self.writes[..self.write_count]
    }
}

fn event_of(task: u32) -> u32 {
    match task {
        TASKS_START => EVENTS_STARTED,
        TASKS_SAMPLE => EVENTS_END,
        TASKS_STOP => EVENTS_STOPPED,
        _ => EVENTS_CALIBRATE_DONE,
    }
}

impl Registers for FakeSaadc {
    fn read(&self, offset: u32) -> u32 {
        self.registers[offset as usize / 4]
    }

    fn write(&mut self, offset: u32, value: u32) {
        self.writes[self.write_count] = (offset, value);
        self.write_count += 1;
        self.registers[offset as usize / 4] = value;
        if offset < 0x100 && value == 1 && self.silent_task != Some(offset) {
            if offset == TASKS_SAMPLE {
                unsafe { self.result.write(self.readings[self.samples_taken]) };
                self.samples_taken += 1;
            }
            self.registers[event_of(offset) as usize / 4] = 1;
        }
    }

    fn point_result_at(&mut self, result: *mut i16) {
        self.result = result;
    }
}

// ============================================================================
// Tests
// ============================================================================

#[test]
fn averages_four_samples_into_millivolts() {
    let mut saadc = FakeSaadc::new([2200, 2202, 2198, 2200]);

    assert_eq!(read_millivolts(&mut saadc), Some(millivolts_from_reading(2200)));
    assert_eq!(saadc.samples_taken, 4);
}

#[test]
fn sets_up_the_battery_pin_like_the_firmware_and_switches_off_after() {
    let mut saadc = FakeSaadc::new([2200; 4]);

    read_millivolts(&mut saadc);

    let written = saadc.written();
    assert!(written.contains(&(CHANNEL_0_POSITIVE_INPUT, BATTERY_INPUT)));
    assert!(written.contains(&(CHANNEL_0_CONFIG, BATTERY_CHANNEL_CONFIG)));
    assert!(written.contains(&(RESOLUTION, TWELVE_BITS)));
    assert!(written.contains(&(TASKS_CALIBRATE_OFFSET, 1)));
    assert_eq!(written[0], (ENABLE, 1));
    assert_eq!(written[written.len() - 1], (ENABLE, 0));
}

#[test]
fn a_converter_that_never_answers_gives_no_reading_and_is_still_stopped() {
    let mut saadc = FakeSaadc::new([2200; 4]);
    saadc.silent_task = Some(TASKS_SAMPLE);

    assert_eq!(read_millivolts(&mut saadc), None);
    let written = saadc.written();
    assert!(written.contains(&(TASKS_STOP, 1)));
    assert_eq!(written[written.len() - 1], (ENABLE, 0));
}

#[test]
fn a_failed_calibration_takes_no_samples() {
    let mut saadc = FakeSaadc::new([2200; 4]);
    saadc.silent_task = Some(TASKS_CALIBRATE_OFFSET);

    assert_eq!(read_millivolts(&mut saadc), None);
    assert_eq!(saadc.samples_taken, 0);
}
