//! A simulator of the Thumb-2 instructions the assembler writes, for running translated code on the computer.
//!
//! It knows only those instructions and stops at any other, so a test fails rather than running
//! something the watch would run differently.

use alloc::vec::Vec;

// ============================================================================
// Memory
// ============================================================================

/// A stretch of simulated memory at a fixed address.
pub struct Region {
    pub address: u32,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stop {
    /// The code returned to the address it was called from.
    Returned,
    UnknownInstruction {
        address: u32,
        halfword: u16,
    },
    BadAddress(u32),
    TooManyInstructions,
}

/// Where a simulated call returns to; nothing is ever placed there.
const RETURN_ADDRESS: u32 = 0xFFFF_FFFE;

const MOST_INSTRUCTIONS: u64 = 50_000_000;

// ============================================================================
// The processor
// ============================================================================

pub struct Machine {
    pub registers: [u32; 16],
    negative: bool,
    zero: bool,
    carry: bool,
    overflow: bool,
    pub regions: Vec<Region>,
    /// How many instructions ran, for comparing translations.
    pub instructions_run: u64,
}

impl Machine {
    pub fn new(regions: Vec<Region>) -> Machine {
        Machine {
            registers: [0; 16],
            negative: false,
            zero: false,
            carry: false,
            overflow: false,
            regions,
            instructions_run: 0,
        }
    }

    fn find(&self, address: u32, length: u32) -> Result<(usize, usize), Stop> {
        self.regions
            .iter()
            .enumerate()
            .find(|(_, region)| {
                address >= region.address
                    && (address - region.address) as usize + length as usize <= region.bytes.len()
            })
            .map(|(index, region)| (index, (address - region.address) as usize))
            .ok_or(Stop::BadAddress(address))
    }

    pub fn read_word(&self, address: u32) -> Result<u32, Stop> {
        let (region, at) = self.find(address, 4)?;
        let bytes = &self.regions[region].bytes[at..at + 4];
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    pub fn write_word(&mut self, address: u32, value: u32) -> Result<(), Stop> {
        let (region, at) = self.find(address, 4)?;
        self.regions[region].bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
        Ok(())
    }

    fn read_halfword(&self, address: u32) -> Result<u16, Stop> {
        let (region, at) = self.find(address, 2)?;
        let bytes = &self.regions[region].bytes[at..at + 2];
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    /// Calls the code at `entry` with `argument` in r0 and the stack at `stack_top`, until it returns.
    pub fn call(&mut self, entry: u32, argument: u32, stack_top: u32) -> Stop {
        self.registers[0] = argument;
        self.registers[13] = stack_top;
        self.registers[14] = RETURN_ADDRESS | 1;
        self.registers[15] = entry & !1;
        loop {
            if self.registers[15] == RETURN_ADDRESS {
                return Stop::Returned;
            }
            if self.instructions_run >= MOST_INSTRUCTIONS {
                return Stop::TooManyInstructions;
            }
            self.instructions_run += 1;
            if let Err(stop) = self.step() {
                return stop;
            }
        }
    }

    fn set_logic_flags(&mut self, result: u32) {
        self.negative = result >> 31 == 1;
        self.zero = result == 0;
    }

    /// Adds with a carry in, setting every flag as the processor does.
    fn add_with_carry(&mut self, first: u32, second: u32, carry_in: bool) -> u32 {
        let unsigned = first as u64 + second as u64 + carry_in as u64;
        let signed = first as i32 as i64 + second as i32 as i64 + carry_in as i64;
        let result = unsigned as u32;
        self.set_logic_flags(result);
        self.carry = unsigned >> 32 != 0;
        self.overflow = result as i32 as i64 != signed;
        result
    }

    fn holds(&self, condition: u16) -> bool {
        match condition {
            0 => self.zero,
            1 => !self.zero,
            2 => self.carry,
            3 => !self.carry,
            4 => self.negative,
            5 => !self.negative,
            6 => self.overflow,
            7 => !self.overflow,
            8 => self.carry && !self.zero,
            9 => !self.carry || self.zero,
            10 => self.negative == self.overflow,
            11 => self.negative != self.overflow,
            12 => !self.zero && self.negative == self.overflow,
            13 => self.zero || self.negative != self.overflow,
            _ => true,
        }
    }

    fn branch_to(&mut self, address: u32) {
        self.registers[15] = address & !1;
    }

    fn step(&mut self) -> Result<(), Stop> {
        let address = self.registers[15];
        let first = self.read_halfword(address)?;
        let unknown = Stop::UnknownInstruction {
            address,
            halfword: first,
        };
        // A 32-bit instruction starts with 0b11101, 0b11110 or 0b11111.
        if first >> 11 >= 0b11101 {
            let second = self.read_halfword(address + 2)?;
            self.registers[15] = address + 4;
            return self.step_wide(address, first, second).map_err(|_| unknown);
        }
        self.registers[15] = address + 2;
        let program_counter = address + 4;
        let low = |shift: u16| (first >> shift & 7) as usize;
        let immediate8 = (first & 0xFF) as u32;

        match first >> 11 {
            // LSLS rd, rm, #places
            0b00000 => {
                let places = (first >> 6 & 31) as u32;
                let value = self.registers[low(3)];
                if places > 0 {
                    self.carry = value >> (32 - places) & 1 == 1;
                }
                let result = value << places;
                self.registers[low(0)] = result;
                self.set_logic_flags(result);
            }
            // ADDS and SUBS with three registers
            0b00011 => {
                let first_operand = self.registers[low(3)];
                let second_operand = self.registers[low(6)];
                let result = match first >> 9 & 3 {
                    0 => self.add_with_carry(first_operand, second_operand, false),
                    1 => self.add_with_carry(first_operand, !second_operand, true),
                    _ => return Err(unknown),
                };
                self.registers[low(0)] = result;
            }
            0b00100 => {
                self.registers[low(8)] = immediate8;
                self.set_logic_flags(immediate8);
            }
            0b00101 => {
                let _ = self.add_with_carry(self.registers[low(8)], !immediate8, true);
            }
            0b00110 => {
                self.registers[low(8)] = self.add_with_carry(self.registers[low(8)], immediate8, false);
            }
            0b00111 => {
                self.registers[low(8)] = self.add_with_carry(self.registers[low(8)], !immediate8, true);
            }
            0b01000 if first >> 10 & 1 == 0 => {
                let register = low(0);
                let other = self.registers[low(3)];
                let value = self.registers[register];
                let result = match first >> 6 & 15 {
                    0 => value & other,
                    1 => value ^ other,
                    5 => self.add_with_carry(value, other, self.carry),
                    6 => self.add_with_carry(value, !other, self.carry),
                    9 => {
                        self.registers[register] = self.add_with_carry(0, !other, true);
                        return Ok(());
                    }
                    10 => {
                        let _ = self.add_with_carry(value, !other, true);
                        return Ok(());
                    }
                    12 => value | other,
                    15 => !other,
                    _ => return Err(unknown),
                };
                if !matches!(first >> 6 & 15, 5 | 6) {
                    self.set_logic_flags(result);
                }
                self.registers[register] = result;
            }
            0b01000 => match first >> 8 & 3 {
                // MOV rd, rm
                2 => {
                    let destination = ((first >> 7 & 1) << 3 | first & 7) as usize;
                    let value = self.registers[(first >> 3 & 15) as usize];
                    match destination {
                        15 => self.branch_to(value),
                        _ => self.registers[destination] = value,
                    }
                }
                // BLX rm and BX rm
                3 => {
                    let target = self.registers[(first >> 3 & 15) as usize];
                    if first >> 7 & 1 == 1 {
                        self.registers[14] = (address + 2) | 1;
                    }
                    self.branch_to(target);
                }
                _ => return Err(unknown),
            },
            // STR and LDR with an immediate offset
            0b01100 | 0b01101 => {
                let at = self.registers[low(3)].wrapping_add((first >> 6 & 31) as u32 * 4);
                match first >> 11 & 1 {
                    0 => self.write_word(at, self.registers[low(0)])?,
                    _ => self.registers[low(0)] = self.read_word(at)?,
                }
            }
            // PUSH and POP
            0b10110 | 0b10111 if first >> 9 & 3 == 2 => {
                let list = first & 0xFF;
                let extra = first >> 8 & 1 == 1;
                if first >> 11 & 1 == 0 {
                    let count = list.count_ones() + extra as u32;
                    let mut at = self.registers[13] - count * 4;
                    self.registers[13] = at;
                    for register in 0..8 {
                        if list >> register & 1 == 1 {
                            self.write_word(at, self.registers[register])?;
                            at += 4;
                        }
                    }
                    if extra {
                        self.write_word(at, self.registers[14])?;
                    }
                } else {
                    let mut at = self.registers[13];
                    for register in 0..8 {
                        if list >> register & 1 == 1 {
                            self.registers[register] = self.read_word(at)?;
                            at += 4;
                        }
                    }
                    if extra {
                        let target = self.read_word(at)?;
                        at += 4;
                        self.branch_to(target);
                    }
                    self.registers[13] = at;
                }
            }
            // B<condition>
            0b11010 | 0b11011 => {
                let condition = first >> 8 & 15;
                if condition >= 14 {
                    return Err(unknown);
                }
                if self.holds(condition) {
                    let distance = (first & 0xFF) as i8 as i32 * 2;
                    self.branch_to(program_counter.wrapping_add(distance as u32));
                }
            }
            // B
            0b11100 => {
                let distance = (((first & 0x7FF) as i32) << 21) >> 20;
                self.branch_to(program_counter.wrapping_add(distance as u32));
            }
            _ => return Err(unknown),
        }
        Ok(())
    }

    fn step_wide(&mut self, address: u32, first: u16, second: u16) -> Result<(), ()> {
        let program_counter = address + 4;
        // LDRD and STRD with an offset, no write-back
        if first & 0xFFE0 == 0xE9C0 {
            let base = self.registers[(first & 15) as usize];
            let at = base.wrapping_add((second & 0xFF) as u32 * 4);
            let low = (second >> 12) as usize;
            let high = (second >> 8 & 15) as usize;
            if first >> 4 & 1 == 1 {
                self.registers[low] = self.read_word(at).map_err(|_| ())?;
                self.registers[high] = self.read_word(at + 4).map_err(|_| ())?;
            } else {
                self.write_word(at, self.registers[low]).map_err(|_| ())?;
                self.write_word(at + 4, self.registers[high]).map_err(|_| ())?;
            }
            return Ok(());
        }
        // MOVW and MOVT
        if first & 0xFB70 == 0xF240 && second & 0x8000 == 0 {
            let value =
                (first & 15) << 12 | (first >> 10 & 1) << 11 | (second >> 12 & 7) << 8 | second & 0xFF;
            let destination = (second >> 8 & 15) as usize;
            match first >> 7 & 1 {
                0 => self.registers[destination] = value as u32,
                _ => {
                    self.registers[destination] = self.registers[destination] & 0xFFFF | (value as u32) << 16
                }
            }
            return Ok(());
        }
        if first >> 11 == 0b11110 && second >> 14 == 0b10 {
            let sign = (first >> 10 & 1) as u32;
            let first_jump = (second >> 13 & 1) as u32;
            let second_jump = (second >> 11 & 1) as u32;
            let lower = (second & 0x7FF) as u32;
            match second >> 12 & 1 {
                // B.W
                1 => {
                    let upper = (first & 0x3FF) as u32;
                    let first_bit = 1 - (first_jump ^ sign);
                    let second_bit = 1 - (second_jump ^ sign);
                    let distance = sign << 24 | first_bit << 23 | second_bit << 22 | upper << 12 | lower << 1;
                    let distance = ((distance << 7) as i32) >> 7;
                    self.branch_to(program_counter.wrapping_add(distance as u32));
                }
                // B<condition>.W
                _ => {
                    let condition = first >> 6 & 15;
                    if condition >= 14 {
                        return Err(());
                    }
                    let upper = (first & 0x3F) as u32;
                    let distance =
                        sign << 20 | second_jump << 19 | first_jump << 18 | upper << 12 | lower << 1;
                    let distance = ((distance << 11) as i32) >> 11;
                    if self.holds(condition) {
                        self.branch_to(program_counter.wrapping_add(distance as u32));
                    }
                }
            }
            return Ok(());
        }
        Err(())
    }
}
