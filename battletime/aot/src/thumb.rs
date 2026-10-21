//! A Thumb-2 assembler for the few instructions the translator writes.
//!
//! Code is built as halfwords. Branches name labels and are filled in by `finish`, once every label
//! has a place: long branches always use their 32-bit forms, so no branch ever grows, and short
//! ones are checked to reach.

use alloc::vec::Vec;

// ============================================================================
// Registers and conditions
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Register {
    pub number: u8,
}

pub const R0: Register = Register { number: 0 };
pub const R1: Register = Register { number: 1 };
pub const R2: Register = Register { number: 2 };
pub const R3: Register = Register { number: 3 };
pub const R4: Register = Register { number: 4 };
pub const R5: Register = Register { number: 5 };
pub const R6: Register = Register { number: 6 };
pub const R7: Register = Register { number: 7 };
pub const R12: Register = Register { number: 12 };
pub const LINK: Register = Register { number: 14 };
pub const PROGRAM_COUNTER: Register = Register { number: 15 };

/// A condition a branch tests the flags for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Condition {
    Equal = 0,
    NotEqual = 1,
    CarrySet = 2,
    CarryClear = 3,
    Negative = 4,
    PositiveOrZero = 5,
    Overflow = 6,
    NoOverflow = 7,
    UnsignedHigher = 8,
    UnsignedLowerOrSame = 9,
    GreaterOrEqual = 10,
    Less = 11,
    Greater = 12,
    LessOrEqual = 13,
}

impl Condition {
    pub fn opposite(self) -> Condition {
        use Condition::*;
        match self {
            Equal => NotEqual,
            NotEqual => Equal,
            CarrySet => CarryClear,
            CarryClear => CarrySet,
            Negative => PositiveOrZero,
            PositiveOrZero => Negative,
            Overflow => NoOverflow,
            NoOverflow => Overflow,
            UnsignedHigher => UnsignedLowerOrSame,
            UnsignedLowerOrSame => UnsignedHigher,
            GreaterOrEqual => Less,
            Less => GreaterOrEqual,
            Greater => LessOrEqual,
            LessOrEqual => Greater,
        }
    }
}

/// The two-register data operations of the 16-bit `0100 00` group, by their operation number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataOperation {
    And = 0,
    ExclusiveOr = 1,
    AddWithCarry = 5,
    SubtractWithCarry = 6,
    Or = 12,
    Not = 15,
}

// ============================================================================
// Labels and errors
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Label {
    index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssembleError {
    LabelNeverPlaced,
    BranchTooFar,
    OperandTooLarge,
}

#[derive(Clone, Copy)]
enum BranchKind {
    Long,
    LongIf(Condition),
    ShortIf(Condition),
    Short,
}

struct PendingBranch {
    /// Where the branch's first halfword is.
    at_halfword: usize,
    kind: BranchKind,
    target: Label,
}

// ============================================================================
// The assembler
// ============================================================================

#[derive(Default)]
pub struct Assembler {
    halfwords: Vec<u16>,
    label_places: Vec<Option<usize>>,
    branches: Vec<PendingBranch>,
    /// The first operand that did not fit its instruction, reported by `finish`.
    operand_error: Option<AssembleError>,
}

impl Assembler {
    pub fn new() -> Assembler {
        Assembler::default()
    }

    /// How many bytes of code are written so far.
    pub fn len_in_bytes(&self) -> usize {
        self.halfwords.len() * 2
    }

    pub fn label(&mut self) -> Label {
        self.label_places.push(None);
        Label {
            index: self.label_places.len() - 1,
        }
    }

    pub fn place(&mut self, label: Label) {
        self.label_places[label.index] = Some(self.halfwords.len());
    }

    fn halfword(&mut self, value: u16) {
        self.halfwords.push(value);
    }

    fn check(&mut self, fits: bool) {
        if !fits && self.operand_error.is_none() {
            self.operand_error = Some(AssembleError::OperandTooLarge);
        }
    }

    fn low(&mut self, register: Register) -> u16 {
        self.check(register.number < 8);
        (register.number & 7) as u16
    }

    // ------------------------------------------------------------------------
    // Moves and constants
    // ------------------------------------------------------------------------

    /// `MOVS rd, #value`, which sets N and Z.
    pub fn move_small(&mut self, destination: Register, value: u32) {
        self.check(value < 256);
        let destination = self.low(destination);
        self.halfword(0x2000 | destination << 8 | (value & 0xFF) as u16);
    }

    /// `MOV rd, rm`, for any registers, leaving the flags alone.
    pub fn move_register(&mut self, destination: Register, source: Register) {
        let destination = destination.number as u16;
        self.halfword(0x4600 | (destination >> 3) << 7 | (source.number as u16) << 3 | (destination & 7));
    }

    /// `MOVW rd, #value`, the low half, clearing the high half.
    pub fn move_wide(&mut self, destination: Register, value: u16) {
        self.wide_immediate(0xF240, destination, value);
    }

    /// `MOVT rd, #value`, the high half, keeping the low half.
    pub fn move_top(&mut self, destination: Register, value: u16) {
        self.wide_immediate(0xF2C0, destination, value);
    }

    fn wide_immediate(&mut self, first: u16, destination: Register, value: u16) {
        let top_four = value >> 12;
        let eleventh = (value >> 11) & 1;
        let next_three = (value >> 8) & 7;
        let low_eight = value & 0xFF;
        self.halfword(first | eleventh << 10 | top_four);
        self.halfword(next_three << 12 | (destination.number as u16) << 8 | low_eight);
    }

    /// Puts any 32-bit value in a register in as few instructions as it takes; may set N and Z.
    pub fn load_constant(&mut self, destination: Register, value: u32) {
        if value < 256 && destination.number < 8 {
            self.move_small(destination, value);
            return;
        }
        self.move_wide(destination, value as u16);
        if value >> 16 != 0 {
            self.move_top(destination, (value >> 16) as u16);
        }
    }

    // ------------------------------------------------------------------------
    // Arithmetic and logic, all on low registers and setting the flags
    // ------------------------------------------------------------------------

    /// `ADDS rd, rn, rm`.
    pub fn add(&mut self, destination: Register, first: Register, second: Register) {
        let (destination, first, second) = (self.low(destination), self.low(first), self.low(second));
        self.halfword(0x1800 | second << 6 | first << 3 | destination);
    }

    /// `SUBS rd, rn, rm`.
    pub fn subtract(&mut self, destination: Register, first: Register, second: Register) {
        let (destination, first, second) = (self.low(destination), self.low(first), self.low(second));
        self.halfword(0x1A00 | second << 6 | first << 3 | destination);
    }

    /// `ADDS rdn, #value`.
    pub fn add_small(&mut self, register: Register, value: u32) {
        self.check(value < 256);
        let register = self.low(register);
        self.halfword(0x3000 | register << 8 | (value & 0xFF) as u16);
    }

    /// `SUBS rdn, #value`.
    pub fn subtract_small(&mut self, register: Register, value: u32) {
        self.check(value < 256);
        let register = self.low(register);
        self.halfword(0x3800 | register << 8 | (value & 0xFF) as u16);
    }

    /// `CMP rn, #value`.
    pub fn compare_small(&mut self, register: Register, value: u32) {
        self.check(value < 256);
        let register = self.low(register);
        self.halfword(0x2800 | register << 8 | (value & 0xFF) as u16);
    }

    /// `CMP rn, rm`, for low registers.
    pub fn compare(&mut self, first: Register, second: Register) {
        let (first, second) = (self.low(first), self.low(second));
        self.halfword(0x4280 | second << 3 | first);
    }

    /// One of the `rdn = rdn <operation> rm` instructions, such as `ADCS` or `ORRS`.
    pub fn data(&mut self, operation: DataOperation, register: Register, other: Register) {
        let (register, other) = (self.low(register), self.low(other));
        self.halfword(0x4000 | (operation as u16) << 6 | other << 3 | register);
    }

    /// `RSBS rd, rn, #0`, the negation.
    pub fn negate(&mut self, destination: Register, source: Register) {
        let (destination, source) = (self.low(destination), self.low(source));
        self.halfword(0x4240 | source << 3 | destination);
    }

    /// `LSLS rd, rm, #places`.
    pub fn shift_left(&mut self, destination: Register, source: Register, places: u32) {
        self.check(places < 32);
        let (destination, source) = (self.low(destination), self.low(source));
        self.halfword(((places & 31) as u16) << 6 | source << 3 | destination);
    }

    // ------------------------------------------------------------------------
    // Memory
    // ------------------------------------------------------------------------

    /// `LDR rt, [rn, #offset]`, with a word-aligned offset below 128.
    pub fn load_word(&mut self, target: Register, base: Register, offset: u32) {
        self.word_access(0x6800, target, base, offset);
    }

    /// `STR rt, [rn, #offset]`, with a word-aligned offset below 128.
    pub fn store_word(&mut self, source: Register, base: Register, offset: u32) {
        self.word_access(0x6000, source, base, offset);
    }

    fn word_access(&mut self, first: u16, target: Register, base: Register, offset: u32) {
        self.check(offset.is_multiple_of(4) && offset < 128);
        let (target, base) = (self.low(target), self.low(base));
        self.halfword(first | (((offset / 4) & 31) as u16) << 6 | base << 3 | target);
    }

    /// `LDRD rt, rt2, [rn, #offset]`, with a word-aligned offset below 1024.
    pub fn load_pair(&mut self, low: Register, high: Register, base: Register, offset: u32) {
        self.pair_access(0xE9D0, low, high, base, offset);
    }

    /// `STRD rt, rt2, [rn, #offset]`, with a word-aligned offset below 1024.
    pub fn store_pair(&mut self, low: Register, high: Register, base: Register, offset: u32) {
        self.pair_access(0xE9C0, low, high, base, offset);
    }

    fn pair_access(&mut self, first: u16, low: Register, high: Register, base: Register, offset: u32) {
        self.check(offset.is_multiple_of(4) && offset < 1024);
        self.halfword(first | base.number as u16);
        self.halfword((low.number as u16) << 12 | (high.number as u16) << 8 | ((offset / 4) & 0xFF) as u16);
    }

    /// `PUSH {registers}`, of low registers and the link register.
    pub fn push(&mut self, registers: &[Register]) {
        let list = self.register_list(registers, LINK);
        self.halfword(0xB400 | list);
    }

    /// `POP {registers}`, of low registers and the program counter.
    pub fn pop(&mut self, registers: &[Register]) {
        let list = self.register_list(registers, PROGRAM_COUNTER);
        self.halfword(0xBC00 | list);
    }

    fn register_list(&mut self, registers: &[Register], extra: Register) -> u16 {
        registers
            .iter()
            .fold(0, |list, register| match *register == extra {
                true => list | 1 << 8,
                false => list | 1 << self.low(*register),
            })
    }

    // ------------------------------------------------------------------------
    // Branches
    // ------------------------------------------------------------------------

    /// `B.W label`, reaching anywhere in the code.
    pub fn branch(&mut self, target: Label) {
        self.pending(BranchKind::Long, target, 2);
    }

    /// `B<condition>.W label`, reaching a megabyte either way.
    pub fn branch_if(&mut self, condition: Condition, target: Label) {
        self.pending(BranchKind::LongIf(condition), target, 2);
    }

    /// `B<condition> label`, reaching 256 bytes either way.
    pub fn branch_short_if(&mut self, condition: Condition, target: Label) {
        self.pending(BranchKind::ShortIf(condition), target, 1);
    }

    /// `B label`, reaching 2 KB either way.
    pub fn branch_short(&mut self, target: Label) {
        self.pending(BranchKind::Short, target, 1);
    }

    /// `BLX rm`, calling the address in a register.
    pub fn call_register(&mut self, register: Register) {
        self.halfword(0x4780 | (register.number as u16) << 3);
    }

    fn pending(&mut self, kind: BranchKind, target: Label, halfwords: usize) {
        self.branches.push(PendingBranch {
            at_halfword: self.halfwords.len(),
            kind,
            target,
        });
        for _ in 0..halfwords {
            self.halfword(0);
        }
    }

    /// The finished code as little-endian bytes, with every branch filled in.
    pub fn finish(mut self) -> Result<Vec<u8>, AssembleError> {
        if let Some(error) = self.operand_error {
            return Err(error);
        }
        for branch in &self.branches {
            let target = self.label_places[branch.target.index].ok_or(AssembleError::LabelNeverPlaced)?;
            // The processor reads the program counter as the branch's address plus 4.
            let distance = (target as i64 - branch.at_halfword as i64) * 2 - 4;
            let at = branch.at_halfword;
            match branch.kind {
                BranchKind::Short => {
                    fits_signed(distance, 12)?;
                    self.halfwords[at] = 0xE000 | ((distance >> 1) & 0x7FF) as u16;
                }
                BranchKind::ShortIf(condition) => {
                    fits_signed(distance, 9)?;
                    self.halfwords[at] = 0xD000 | (condition as u16) << 8 | ((distance >> 1) & 0xFF) as u16;
                }
                BranchKind::LongIf(condition) => {
                    fits_signed(distance, 21)?;
                    let sign = ((distance >> 20) & 1) as u16;
                    let second_jump = ((distance >> 19) & 1) as u16;
                    let first_jump = ((distance >> 18) & 1) as u16;
                    let upper = ((distance >> 12) & 0x3F) as u16;
                    let lower = ((distance >> 1) & 0x7FF) as u16;
                    self.halfwords[at] = 0xF000 | sign << 10 | (condition as u16) << 6 | upper;
                    self.halfwords[at + 1] = 0x8000 | first_jump << 13 | second_jump << 11 | lower;
                }
                BranchKind::Long => {
                    fits_signed(distance, 25)?;
                    let sign = ((distance >> 24) & 1) as u16;
                    let first = ((distance >> 23) & 1) as u16;
                    let second = ((distance >> 22) & 1) as u16;
                    let first_jump = (1 - (first ^ sign)) & 1;
                    let second_jump = (1 - (second ^ sign)) & 1;
                    let upper = ((distance >> 12) & 0x3FF) as u16;
                    let lower = ((distance >> 1) & 0x7FF) as u16;
                    self.halfwords[at] = 0xF000 | sign << 10 | upper;
                    self.halfwords[at + 1] = 0x9000 | first_jump << 13 | second_jump << 11 | lower;
                }
            }
        }
        Ok(self
            .halfwords
            .iter()
            .flat_map(|halfword| halfword.to_le_bytes())
            .collect())
    }
}

/// Whether a branch distance fits a signed field of `bits` bits.
fn fits_signed(distance: i64, bits: u32) -> Result<(), AssembleError> {
    let limit = 1i64 << (bits - 1);
    match (-limit..limit).contains(&distance) {
        true => Ok(()),
        false => Err(AssembleError::BranchTooFar),
    }
}
