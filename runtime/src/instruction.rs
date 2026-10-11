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
    TimePart(TimePart),
    FormatNumber(u8),
    /// Formats a duration with the pattern in this string; see `pattern`.
    FormatDuration(u16),
    /// Formats a local time with the pattern in this string; see `pattern`.
    FormatTime(u16),
    Join,
    WriteState(u8),
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
            Drop | JumpIfFalse(_) | WriteState(_) => (1, 0),
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
        other => return Err(DecodeError::UnknownOpcode(other)),
    };
    Ok((instruction, offset + 1 + operand_bytes))
}
