//! The backlight: three pins, each turned on by pulling it low, for low, middle and high brightness.

use embassy_nrf::gpio::Output;

pub const DEFAULT_LEVEL: u8 = 2;

pub struct Backlight {
    low: Output<'static>,
    middle: Output<'static>,
    high: Output<'static>,
    level: u8,
    is_on: bool,
}

impl Backlight {
    /// Takes the three pins and turns the light on at the default level.
    pub fn new(low: Output<'static>, middle: Output<'static>, high: Output<'static>) -> Backlight {
        let mut backlight = Backlight {
            low,
            middle,
            high,
            level: DEFAULT_LEVEL,
            is_on: true,
        };
        backlight.apply();
        backlight
    }

    /// The level, from 1 to 3.
    pub fn level(&self) -> u8 {
        self.level
    }

    pub fn set_level(&mut self, level: u8) {
        self.level = level.clamp(1, 3);
        self.apply();
    }

    pub fn turn_on(&mut self) {
        self.is_on = true;
        self.apply();
    }

    pub fn turn_off(&mut self) {
        self.is_on = false;
        self.apply();
    }

    fn apply(&mut self) {
        let level = if self.is_on { self.level } else { 0 };
        for (pin, pin_level) in [(&mut self.low, 1), (&mut self.middle, 2), (&mut self.high, 3)] {
            if level == pin_level {
                pin.set_low();
            } else {
                pin.set_high();
            }
        }
    }
}
