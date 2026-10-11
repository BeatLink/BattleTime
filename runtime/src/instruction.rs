//! The bytecode's instructions, the names their operands refer to, and how to decode them.
//!
//! Every instruction is one opcode byte followed by fixed operand bytes, all little-endian.
//! docs/app-runtime.md lists them with their byte values.

// ============================================================================
// System values
// ============================================================================

/// A value the runtime keeps current, which code reads with `ReadSystem`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SystemValue {
    Now,
    UtcOffset,
    BatteryPercent,
    IsCharging,
    StepsToday,
    HeartRate,
    HeartRateMeasuredAt,
    IsPhoneConnected,
    UnreadNotifications,
    IsDoNotDisturb,
    Brightness,
    Uses24HourClock,
    UsesMetricUnits,
    StepGoal,
    IsHeartRateRunning,
    IsBluetoothEnabled,
    /// The next alarm that will ring, or none.
    NextAlarm,
    /// 0 stopped, 1 no skin, 2 settling, 3 measuring.
    HeartRateStatus,
    WeatherTemperature,
    WeatherCondition,
    WeatherLow,
    WeatherHigh,
    /// When the phone last sent the weather, or none.
    WeatherReceivedAt,
}

impl SystemValue {
    pub const COUNT: usize = 23;

    pub fn from_byte(byte: u8) -> Option<SystemValue> {
        use SystemValue::*;
        Some(match byte {
            0 => Now,
            1 => UtcOffset,
            2 => BatteryPercent,
            3 => IsCharging,
            4 => StepsToday,
            5 => HeartRate,
            6 => HeartRateMeasuredAt,
            7 => IsPhoneConnected,
            8 => UnreadNotifications,
            9 => IsDoNotDisturb,
            10 => Brightness,
            11 => Uses24HourClock,
            12 => UsesMetricUnits,
            13 => StepGoal,
            14 => IsHeartRateRunning,
            15 => IsBluetoothEnabled,
            16 => NextAlarm,
            17 => HeartRateStatus,
            18 => WeatherTemperature,
            19 => WeatherCondition,
            20 => WeatherLow,
            21 => WeatherHigh,
            22 => WeatherReceivedAt,
            _ => return None,
        })
    }

    pub fn bit(self) -> u32 {
        1 << (self as u32)
    }
}

// ============================================================================
// Time parts
// ============================================================================

/// A part of a local date and time, read with `TimePart`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimePart {
    Year,
    Month,
    Day,
    Hour,
    Minute,
    Second,
    /// Monday is 0 and Sunday is 6.
    Weekday,
}

impl TimePart {
    pub fn from_byte(byte: u8) -> Option<TimePart> {
        use TimePart::*;
        Some(match byte {
            0 => Year,
            1 => Month,
            2 => Day,
            3 => Hour,
            4 => Minute,
            5 => Second,
            6 => Weekday,
            _ => return None,
        })
    }
}

// ============================================================================
// Effects
// ============================================================================

/// Something a handler asks the system to do once it has finished.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectKind {
    Vibrate,
    VibratePattern,
    StopVibrating,
    SetBrightness,
    KeepScreenOn,
    WakeScreen,
    ScreenOff,
    Go,
    Back,
    OpenApp,
    Close,
    ShowToast,
    Notify,
    DismissNotification,
    MarkNotificationsRead,
    SendToPhone,
    MusicControl,
    CallControl,
    FindPhone,
    MeasureHeartRate,
    StartHeartRate,
    StopHeartRate,
    ResetSteps,
    SetSetting,
    SetDoNotDisturb,
    Log,
    RequestMusicState,
    SetHeartRateLed,
    SetBluetooth,
}

impl EffectKind {
    pub fn from_byte(byte: u8) -> Option<EffectKind> {
        use EffectKind::*;
        Some(match byte {
            0 => Vibrate,
            1 => VibratePattern,
            2 => StopVibrating,
            3 => SetBrightness,
            4 => KeepScreenOn,
            5 => WakeScreen,
            6 => ScreenOff,
            7 => Go,
            8 => Back,
            9 => OpenApp,
            10 => Close,
            11 => ShowToast,
            12 => Notify,
            13 => DismissNotification,
            14 => MarkNotificationsRead,
            15 => SendToPhone,
            16 => MusicControl,
            17 => CallControl,
            18 => FindPhone,
            19 => MeasureHeartRate,
            20 => StartHeartRate,
            21 => StopHeartRate,
            22 => ResetSteps,
            23 => SetSetting,
            24 => SetDoNotDisturb,
            25 => Log,
            26 => RequestMusicState,
            27 => SetHeartRateLed,
            28 => SetBluetooth,
            _ => return None,
        })
    }

    /// How many values the effect takes from the stack.
    pub fn argument_count(self) -> usize {
        use EffectKind::*;
        match self {
            StopVibrating
            | WakeScreen
            | ScreenOff
            | Back
            | Close
            | MarkNotificationsRead
            | RequestMusicState
            | MeasureHeartRate
            | StartHeartRate
            | StopHeartRate
            | ResetSteps => 0,
            Vibrate | VibratePattern | SetBrightness | KeepScreenOn | Go | OpenApp | ShowToast
            | DismissNotification | MusicControl | CallControl | SetDoNotDisturb | Log | FindPhone
            | SetBluetooth => 1,
            Notify | SendToPhone | SetSetting => 2,
            SetHeartRateLed => 3,
        }
    }
}

/// The most arguments any effect takes.
pub const MOST_EFFECT_ARGUMENTS: usize = 3;

// ============================================================================
// Instructions
// ============================================================================

/// One decoded instruction with its operands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Instruction {
    PushSmall(i8),
    PushInt(i32),
    PushLong(i64),
    PushNone,
    PushText(u16),
    ReadState(u8),
    ReadSystem(SystemValue),
    ReadEvent(u8),
    Duplicate,
    Drop,
    Swap,
    /// Pushes one of the call's local values, which all start at zero.
    ReadLocal(u8),
    /// Takes a value and keeps it in a local.
    WriteLocal(u8),
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    Negate,
    Minimum,
    Maximum,
    Absolute,
    /// Takes a lowest and a highest value and leaves a random number between them, both included.
    RandomInt,
    BitAnd,
    BitOr,
    BitXor,
    BitNot,
    /// Shifts left by 0 to 63 places; any other amount fails the call.
    ShiftLeft,
    /// Shifts right by 0 to 63 places, keeping the sign.
    ShiftRight,
    Equal,
    NotEqual,
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
    Not,
    IsNone,
    /// Skips forward this many bytes, counted from the end of this instruction.
    Jump(u16),
    /// Takes a value and skips forward like `Jump` when it is false.
    JumpIfFalse(u16),
    /// Marks the start of a loop. The stack must be empty here.
    LoopStart,
    /// Goes back this many bytes, counted from the end of this instruction, to a `LoopStart`.
    LoopBack(u16),
    TimePart(TimePart),
    FormatNumber(u8),
    /// Formats a duration with the pattern in this string; see `pattern`.
    FormatDuration(u16),
    /// Formats a local time with the pattern in this string; see `pattern`.
    FormatTime(u16),
    Join,
    WriteState(u8),
    /// Takes an index and pushes that element of an array variable.
    ReadElement(u8),
    /// Takes an index and a value and writes that element. Handlers only.
    WriteElement(u8),
    /// Copies every element of the second array into the first. Handlers only.
    CopyArray {
        destination: u8,
        source: u8,
    },
    /// Takes a value and writes it to every element. Handlers only.
    FillArray(u8),
    Effect(EffectKind),
}

/// Why bytes could not be decoded as an instruction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    UnknownOpcode(u8),
    UnknownOperand(u8),
    Truncated,
}

impl Instruction {
    /// How many values the instruction takes from the stack, and how many it leaves.
    pub fn stack_effect(self) -> StackEffect {
        use Instruction::*;
        let (taken, left) = match self {
            PushSmall(_) | PushInt(_) | PushLong(_) | PushNone | PushText(_) | ReadState(_)
            | ReadSystem(_) | ReadEvent(_) => (0, 1),
            Duplicate => (1, 2),
            Drop | JumpIfFalse(_) | WriteState(_) | WriteLocal(_) | FillArray(_) => (1, 0),
            ReadLocal(_) => (0, 1),
            ReadElement(_) => (1, 1),
            WriteElement(_) => (2, 0),
            LoopStart | LoopBack(_) | CopyArray { .. } => (0, 0),
            Swap => (2, 2),
            Add | Subtract | Multiply | Divide | Remainder | Minimum | Maximum | RandomInt | BitAnd
            | BitOr | BitXor | ShiftLeft | ShiftRight | Equal | NotEqual | Less | LessOrEqual | Greater
            | GreaterOrEqual | Join => (2, 1),
            Negate | Absolute | BitNot | Not | IsNone | TimePart(_) | FormatNumber(_) | FormatDuration(_)
            | FormatTime(_) => (1, 1),
            Jump(_) => (0, 0),
            Effect(kind) => (kind.argument_count(), 0),
        };
        StackEffect { taken, left }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StackEffect {
    pub taken: usize,
    pub left: usize,
}

/// Decodes the instruction at `offset`, and returns it with the offset of the next one.
pub fn decode(code: &[u8], offset: usize) -> Result<(Instruction, usize), DecodeError> {
    use Instruction::*;

    let opcode = *code.get(offset).ok_or(DecodeError::Truncated)?;
    let operands = &code[offset + 1..];
    let byte = || operands.first().copied().ok_or(DecodeError::Truncated);
    let bytes = |count: usize| operands.get(..count).ok_or(DecodeError::Truncated);
    let unknown = |operand: u8| DecodeError::UnknownOperand(operand);

    let (instruction, operand_bytes) = match opcode {
        0x01 => (PushSmall(byte()? as i8), 1),
        0x02 => (
            PushInt(i32::from_le_bytes(bytes(4)?.try_into().unwrap_or_default())),
            4,
        ),
        0x03 => (
            PushLong(i64::from_le_bytes(bytes(8)?.try_into().unwrap_or_default())),
            8,
        ),
        0x04 => (PushNone, 0),
        0x05 => (
            PushText(u16::from_le_bytes(bytes(2)?.try_into().unwrap_or_default())),
            2,
        ),
        0x06 => (ReadState(byte()?), 1),
        0x07 => (
            ReadSystem(SystemValue::from_byte(byte()?).ok_or(unknown(byte()?))?),
            1,
        ),
        0x08 => (ReadEvent(byte()?), 1),
        0x09 => (Duplicate, 0),
        0x0A => (Drop, 0),
        0x0B => (Swap, 0),
        0x0C => (ReadLocal(byte()?), 1),
        0x0D => (WriteLocal(byte()?), 1),
        0x10 => (Add, 0),
        0x11 => (Subtract, 0),
        0x12 => (Multiply, 0),
        0x13 => (Divide, 0),
        0x14 => (Remainder, 0),
        0x15 => (Negate, 0),
        0x16 => (Minimum, 0),
        0x17 => (Maximum, 0),
        0x18 => (Absolute, 0),
        0x19 => (RandomInt, 0),
        0x1A => (BitAnd, 0),
        0x1B => (BitOr, 0),
        0x1C => (BitXor, 0),
        0x1D => (BitNot, 0),
        0x1E => (ShiftLeft, 0),
        0x1F => (ShiftRight, 0),
        0x20 => (Equal, 0),
        0x21 => (NotEqual, 0),
        0x22 => (Less, 0),
        0x23 => (LessOrEqual, 0),
        0x24 => (Greater, 0),
        0x25 => (GreaterOrEqual, 0),
        0x26 => (Not, 0),
        0x27 => (IsNone, 0),
        0x28 => (
            Jump(u16::from_le_bytes(bytes(2)?.try_into().unwrap_or_default())),
            2,
        ),
        0x29 => (
            JumpIfFalse(u16::from_le_bytes(bytes(2)?.try_into().unwrap_or_default())),
            2,
        ),
        0x2A => (LoopStart, 0),
        0x2B => (
            LoopBack(u16::from_le_bytes(bytes(2)?.try_into().unwrap_or_default())),
            2,
        ),
        0x30 => (
            TimePart(self::TimePart::from_byte(byte()?).ok_or(unknown(byte()?))?),
            1,
        ),
        0x40 => (FormatNumber(byte()?), 1),
        0x41 => (
            FormatDuration(u16::from_le_bytes(bytes(2)?.try_into().unwrap_or_default())),
            2,
        ),
        0x42 => (
            FormatTime(u16::from_le_bytes(bytes(2)?.try_into().unwrap_or_default())),
            2,
        ),
        0x43 => (Join, 0),
        0x50 => (WriteState(byte()?), 1),
        0x51 => (Effect(EffectKind::from_byte(byte()?).ok_or(unknown(byte()?))?), 1),
        0x60 => (ReadElement(byte()?), 1),
        0x61 => (WriteElement(byte()?), 1),
        0x62 => {
            let variables = bytes(2)?;
            (
                CopyArray {
                    destination: variables[0],
                    source: variables[1],
                },
                2,
            )
        }
        0x63 => (FillArray(byte()?), 1),
        other => return Err(DecodeError::UnknownOpcode(other)),
    };
    Ok((instruction, offset + 1 + operand_bytes))
}

// ============================================================================
// Encoding
// ============================================================================

/// The longest an instruction can be: an opcode and an 8-byte number.
pub const LONGEST_INSTRUCTION_BYTES: usize = 9;

/// The bytes of one encoded instruction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EncodedInstruction {
    bytes: [u8; LONGEST_INSTRUCTION_BYTES],
    length: usize,
}

impl EncodedInstruction {
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.length]
    }
}

impl Instruction {
    /// The instruction's bytes, which `decode` reads back as the same instruction.
    pub fn encode(self) -> EncodedInstruction {
        use Instruction::*;

        let mut encoded = EncodedInstruction {
            bytes: [0; LONGEST_INSTRUCTION_BYTES],
            length: 0,
        };
        let mut put = |bytes: &[u8]| {
            encoded.bytes[encoded.length..encoded.length + bytes.len()].copy_from_slice(bytes);
            encoded.length += bytes.len();
        };

        match self {
            PushSmall(value) => put(&[0x01, value as u8]),
            PushInt(value) => {
                put(&[0x02]);
                put(&value.to_le_bytes());
            }
            PushLong(value) => {
                put(&[0x03]);
                put(&value.to_le_bytes());
            }
            PushNone => put(&[0x04]),
            PushText(index) => {
                put(&[0x05]);
                put(&index.to_le_bytes());
            }
            ReadState(variable) => put(&[0x06, variable]),
            ReadSystem(value) => put(&[0x07, value as u8]),
            ReadEvent(field) => put(&[0x08, field]),
            Duplicate => put(&[0x09]),
            Drop => put(&[0x0A]),
            Swap => put(&[0x0B]),
            ReadLocal(local) => put(&[0x0C, local]),
            WriteLocal(local) => put(&[0x0D, local]),
            Add => put(&[0x10]),
            Subtract => put(&[0x11]),
            Multiply => put(&[0x12]),
            Divide => put(&[0x13]),
            Remainder => put(&[0x14]),
            Negate => put(&[0x15]),
            Minimum => put(&[0x16]),
            Maximum => put(&[0x17]),
            Absolute => put(&[0x18]),
            RandomInt => put(&[0x19]),
            BitAnd => put(&[0x1A]),
            BitOr => put(&[0x1B]),
            BitXor => put(&[0x1C]),
            BitNot => put(&[0x1D]),
            ShiftLeft => put(&[0x1E]),
            ShiftRight => put(&[0x1F]),
            Equal => put(&[0x20]),
            NotEqual => put(&[0x21]),
            Less => put(&[0x22]),
            LessOrEqual => put(&[0x23]),
            Greater => put(&[0x24]),
            GreaterOrEqual => put(&[0x25]),
            Not => put(&[0x26]),
            IsNone => put(&[0x27]),
            Jump(distance) => {
                put(&[0x28]);
                put(&distance.to_le_bytes());
            }
            JumpIfFalse(distance) => {
                put(&[0x29]);
                put(&distance.to_le_bytes());
            }
            LoopStart => put(&[0x2A]),
            LoopBack(distance) => {
                put(&[0x2B]);
                put(&distance.to_le_bytes());
            }
            TimePart(part) => put(&[0x30, part as u8]),
            FormatNumber(minimum_digits) => put(&[0x40, minimum_digits]),
            FormatDuration(pattern) => {
                put(&[0x41]);
                put(&pattern.to_le_bytes());
            }
            FormatTime(pattern) => {
                put(&[0x42]);
                put(&pattern.to_le_bytes());
            }
            Join => put(&[0x43]),
            WriteState(variable) => put(&[0x50, variable]),
            Effect(kind) => put(&[0x51, kind as u8]),
            ReadElement(variable) => put(&[0x60, variable]),
            WriteElement(variable) => put(&[0x61, variable]),
            CopyArray { destination, source } => put(&[0x62, destination, source]),
            FillArray(variable) => put(&[0x63, variable]),
        }
        encoded
    }
}
