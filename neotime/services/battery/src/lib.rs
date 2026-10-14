//! The battery's charge, worked out from its voltage and the charger's two pins.
//!
//! The firmware hands each reading to `Battery::measure`, which smooths the voltage, reads the
//! charge along InfiniTime's measured curve, and keeps the shown charge steady: it only rises on
//! the charger and only falls off it, stops at 99 % while charging, and is 100 % once the charger
//! stays connected but the battery has stopped taking charge. Each reading also says what changed
//! and which events it stands for, such as the charger being plugged in or the charge going low.

#![no_std]

// ============================================================================
// Readings
// ============================================================================

/// The SAADC's full scale with gain 1/6 and the 0.6 V reference, in millivolts.
const FULL_SCALE_IN_MILLIVOLTS: i32 = 3600;
const FULL_SCALE_READING: i32 = 4095;
/// The board halves the battery's voltage before the pin.
const VOLTAGE_DIVIDER: i32 = 2;

/// The battery's voltage for a 12-bit reading of its pin; a negative reading is noise below zero.
pub fn millivolts_from_reading(reading: i16) -> i32 {
    reading.max(0) as i32 * FULL_SCALE_IN_MILLIVOLTS * VOLTAGE_DIVIDER / FULL_SCALE_READING
}

/// One look at the battery: its voltage and the charger's two pins.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Measurement {
    pub millivolts: i32,
    /// Power reaches the charger, from P0.19.
    pub is_power_present: bool,
    /// The charger is putting charge into the battery, from P0.12.
    pub is_charge_pin_active: bool,
}

// ============================================================================
// The charge curve
// ============================================================================

/// InfiniTime's measured charge at each voltage, in millivolts and percent.
const CHARGE_CURVE: [CurvePoint; 6] = [
    CurvePoint {
        millivolts: 3500,
        percent: 0,
    },
    CurvePoint {
        millivolts: 3616,
        percent: 3,
    },
    CurvePoint {
        millivolts: 3723,
        percent: 22,
    },
    CurvePoint {
        millivolts: 3776,
        percent: 48,
    },
    CurvePoint {
        millivolts: 3979,
        percent: 79,
    },
    CurvePoint {
        millivolts: 4180,
        percent: 100,
    },
];

#[derive(Clone, Copy)]
struct CurvePoint {
    millivolts: i32,
    percent: i32,
}

/// The charge at a voltage, read along the curve between its two nearest points.
pub fn percent_at(millivolts: i32) -> u8 {
    if millivolts <= CHARGE_CURVE[0].millivolts {
        return 0;
    }
    for pair in CHARGE_CURVE.windows(2) {
        let (low, high) = (pair[0], pair[1]);
        if millivolts <= high.millivolts {
            let percent = low.percent
                + (high.percent - low.percent) * (millivolts - low.millivolts)
                    / (high.millivolts - low.millivolts);
            return percent as u8;
        }
    }
    100
}

// ============================================================================
// Events
// ============================================================================

/// The charge below which, off the charger, the watch warns once.
pub const LOW_PERCENT: u8 = 10;

/// Something a reading found, in the order the app `battery` subscription numbers them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BatteryEvent {
    /// The charge went below `LOW_PERCENT` off the charger; sent once until the charger is next connected.
    Low,
    /// The charger was plugged in.
    ChargingStarts,
    /// The charger was taken off.
    ChargingStops,
    /// The battery is full while the charger stays connected.
    ChargingCompletes,
}

impl BatteryEvent {
    const ALL: [BatteryEvent; 4] = [
        BatteryEvent::Low,
        BatteryEvent::ChargingStarts,
        BatteryEvent::ChargingStops,
        BatteryEvent::ChargingCompletes,
    ];

    fn bit(self) -> u8 {
        1 << self as u8
    }

    /// Whether the event lights the screen and shows the battery; the low warning keeps quiet in do not disturb.
    pub fn wakes_screen(self, is_do_not_disturb: bool) -> bool {
        match self {
            BatteryEvent::ChargingStarts | BatteryEvent::ChargingStops => true,
            BatteryEvent::Low => !is_do_not_disturb,
            BatteryEvent::ChargingCompletes => false,
        }
    }
}

/// The events one reading found, at most one of each kind.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Events {
    bits: u8,
}

impl Events {
    fn add(&mut self, event: BatteryEvent) {
        self.bits |= event.bit();
    }

    pub fn contains(self, event: BatteryEvent) -> bool {
        self.bits & event.bit() != 0
    }

    pub fn is_empty(self) -> bool {
        self.bits == 0
    }

    pub fn iter(self) -> impl Iterator<Item = BatteryEvent> {
        BatteryEvent::ALL
            .into_iter()
            .filter(move |event| self.contains(*event))
    }
}

/// What one reading changed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Changes {
    /// The shown charge or whether it is charging, which faces and apps show.
    pub charge_changed: bool,
    /// The smoothed voltage, to the hundredth of a volt that screens show.
    pub millivolts_changed: bool,
    pub events: Events,
}

// ============================================================================
// The battery
// ============================================================================

/// How far each reading moves the smoothed voltage: a quarter of the way.
const SMOOTHING_DIVISOR: i32 = 4;
/// The smoothed voltage is kept in sixteenths of a millivolt, so small steps are not lost.
const SMOOTHING_SCALE: i32 = 16;
/// The highest charge shown while still charging, since the voltage reads high on the charger.
const HIGHEST_PERCENT_WHILE_CHARGING: u8 = 99;

/// The battery as the watch shows it, built up from one reading after another.
#[derive(Clone, Copy, Debug, Default)]
pub struct Battery {
    /// None until the first reading.
    smoothed_scaled_millivolts: Option<i32>,
    percent: u8,
    is_power_present: bool,
    /// The last reading had power present but the charge pin idle; a second one in a row means full.
    was_idle_on_power: bool,
    is_full: bool,
    low_warning_given: bool,
}

impl Battery {
    pub const fn new() -> Battery {
        Battery {
            smoothed_scaled_millivolts: None,
            percent: 0,
            is_power_present: false,
            was_idle_on_power: false,
            is_full: false,
            low_warning_given: false,
        }
    }

    /// The charge shown, 0 to 100.
    pub fn percent(&self) -> u8 {
        self.percent
    }

    /// The smoothed voltage, or 0 before the first reading.
    pub fn millivolts(&self) -> i32 {
        self.smoothed_scaled_millivolts.unwrap_or(0) / SMOOTHING_SCALE
    }

    /// On the charger and not yet full.
    pub fn is_charging(&self) -> bool {
        self.is_power_present && !self.is_full
    }

    pub fn is_full(&self) -> bool {
        self.is_full
    }

    /// Takes in one reading, and says what it changed.
    pub fn measure(&mut self, measurement: Measurement) -> Changes {
        let is_first = self.smoothed_scaled_millivolts.is_none();
        let shown_volts_before = self.millivolts() / 10;
        let was_charging = self.is_charging();
        let was_power_present = self.is_power_present;
        let was_full = self.is_full;
        let percent_before = self.percent;

        self.smooth(measurement.millivolts);
        self.follow_charger(measurement);
        self.follow_charge(is_first);

        let mut events = Events::default();
        if !is_first && !was_power_present && self.is_power_present {
            events.add(BatteryEvent::ChargingStarts);
        }
        if was_power_present && !self.is_power_present {
            events.add(BatteryEvent::ChargingStops);
        }
        if !was_full && self.is_full {
            events.add(BatteryEvent::ChargingCompletes);
        }
        if self.is_power_present {
            self.low_warning_given = false;
        } else if self.percent < LOW_PERCENT && !self.low_warning_given {
            self.low_warning_given = true;
            events.add(BatteryEvent::Low);
        }

        Changes {
            charge_changed: is_first || self.percent != percent_before || self.is_charging() != was_charging,
            millivolts_changed: is_first || self.millivolts() / 10 != shown_volts_before,
            events,
        }
    }

    /// Moves the smoothed voltage part of the way towards the new reading.
    fn smooth(&mut self, millivolts: i32) {
        let scaled = millivolts * SMOOTHING_SCALE;
        self.smoothed_scaled_millivolts = Some(match self.smoothed_scaled_millivolts {
            None => scaled,
            Some(smoothed) => smoothed + (scaled - smoothed) / SMOOTHING_DIVISOR,
        });
    }

    /// Full once two readings in a row find power with the charge pin idle, until the charger is taken off.
    fn follow_charger(&mut self, measurement: Measurement) {
        let is_idle_on_power = measurement.is_power_present && !measurement.is_charge_pin_active;
        if !measurement.is_power_present {
            self.is_full = false;
        } else if is_idle_on_power && self.was_idle_on_power {
            self.is_full = true;
        }
        self.was_idle_on_power = is_idle_on_power;
        self.is_power_present = measurement.is_power_present;
    }

    /// Moves the shown charge only up on the charger and only down off it.
    fn follow_charge(&mut self, is_first: bool) {
        let percent = if self.is_full {
            100
        } else if self.is_charging() {
            percent_at(self.millivolts()).min(HIGHEST_PERCENT_WHILE_CHARGING)
        } else {
            percent_at(self.millivolts())
        };
        let rises = self.is_power_present && percent > self.percent;
        let falls = !self.is_power_present && percent < self.percent;
        if is_first || rises || falls {
            self.percent = percent;
        }
    }
}

// ============================================================================
// Single readings, for the programs that run before the firmware
// ============================================================================

/// The lowest voltage, read on the charger, at which the bootloader's pages may be erased.
pub const MINIMUM_MILLIVOLTS_TO_WRITE_BOOTLOADER: i32 = 3700;

/// Whether the battery can carry the half second in which the watch has no bootloader: on the charger and not near empty.
pub fn can_write_bootloader(measurement: Measurement) -> bool {
    measurement.is_power_present && measurement.millivolts >= MINIMUM_MILLIVOLTS_TO_WRITE_BOOTLOADER
}

/// The longest battery line, "Battery 100% unplugged", which just fits a status page line.
pub const BATTERY_LINE_LENGTH: usize = 22;

/// The status page line for one reading, such as "Battery 87% charging".
pub fn battery_line(measurement: Measurement, buffer: &mut [u8; BATTERY_LINE_LENGTH]) -> &str {
    let mut percent = percent_at(measurement.millivolts);
    let state = match (measurement.is_power_present, measurement.is_charge_pin_active) {
        (true, true) => {
            percent = percent.min(HIGHEST_PERCENT_WHILE_CHARGING);
            "charging"
        }
        (true, false) => "charged",
        (false, _) => "unplugged",
    };

    let digits = [
        b'0' + percent / 100,
        b'0' + percent / 10 % 10,
        b'0' + percent % 10,
    ];
    let first_digit = match percent {
        100.. => 0,
        10.. => 1,
        _ => 2,
    };
    let pieces: [&[u8]; 4] = [b"Battery ", &digits[first_digit..], b"% ", state.as_bytes()];
    let mut length = 0;
    for &byte in pieces.iter().flat_map(|piece| piece.iter()) {
        if let Some(slot) = buffer.get_mut(length) {
            *slot = byte;
            length += 1;
        }
    }
    // Every byte written above is ASCII.
    unsafe { core::str::from_utf8_unchecked(&buffer[..length]) }
}

#[cfg(test)]
mod tests;
