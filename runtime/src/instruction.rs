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
    /// Today's sunrise and sunset from the last forecast, or none.
    WeatherSunrise,
    WeatherSunset,
    /// Text: the place the forecast is for.
    WeatherLocation,
    MusicIsPlaying,
    /// Text: the playing track's artist.
    MusicArtist,
    /// Text: the playing track's title.
    MusicTrack,
    MusicPosition,
    MusicLength,
    NavigationTurn,
    /// Text: what the next step says, such as a street name.
    NavigationInstruction,
    NavigationDistance,
    NavigationProgressPercent,
    /// How many coming days the forecast holds, up to 5.
    WeatherForecastLength,
    /// One coming day of the forecast, read with `ReadSystemElement` and the day's number.
    WeatherForecastCondition,
    WeatherForecastLow,
    WeatherForecastHigh,
    /// How many apps the launcher lists.
    InstalledAppCount,
    /// Text list: each app's name, then each app's id, for `open_app`.
    InstalledAppName,
    InstalledAppId,
    /// List: each app's icon, as an image reference from `ImageReference::InstalledApp`.
    InstalledAppIcon,
    ScreenTimeoutSeconds,
    AlwaysOnScreen,
    /// 0 off, 1 every hour, 2 every half hour.
    Chimes,
    /// The number of the preset theme in use, in `PRESET_THEMES` order, or -1 for colours of the user's own.
    ThemePreset,
    /// The battery's smoothed voltage in millivolts, which changes with `battery_percent`.
    BatteryMillivolts,
    /// How many notifications the watch keeps, newest first.
    NotificationCount,
    /// One kept notification, read with `ReadSystemElement` and its place: its id, its app, title and body as text, and when it came.
    NotificationId,
    NotificationApp,
    NotificationTitle,
    NotificationBody,
    NotificationTime,
}

/// Bits 19 and 20 are free because every weather value uses bit 18, so music and navigation take them.
const MUSIC_BIT: u32 = 19;
const NAVIGATION_BIT: u32 = 20;
/// Bit 21 is free too, as every value numbered 21 to 31 is in a group.
const INSTALLED_APPS_BIT: u32 = 21;
/// The settings added after the first three share one bit, since a change to settings redraws them together.
const SETTINGS_BIT: u32 = 22;

impl SystemValue {
    pub const COUNT: usize = 54;

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
            23 => WeatherSunrise,
            24 => WeatherSunset,
            25 => WeatherLocation,
            26 => MusicIsPlaying,
            27 => MusicArtist,
            28 => MusicTrack,
            29 => MusicPosition,
            30 => MusicLength,
            31 => NavigationTurn,
            32 => NavigationInstruction,
            33 => NavigationDistance,
            34 => NavigationProgressPercent,
            35 => WeatherForecastLength,
            36 => WeatherForecastCondition,
            37 => WeatherForecastLow,
            38 => WeatherForecastHigh,
            39 => InstalledAppCount,
            40 => InstalledAppName,
            41 => InstalledAppId,
            42 => InstalledAppIcon,
            43 => ScreenTimeoutSeconds,
            44 => AlwaysOnScreen,
            45 => Chimes,
            46 => ThemePreset,
            47 => BatteryMillivolts,
            48 => NotificationCount,
            49 => NotificationId,
            50 => NotificationApp,
            51 => NotificationTitle,
            52 => NotificationBody,
            53 => NotificationTime,
            _ => return None,
        })
    }

    /// The value's bit in an input mask. Values that the phone sends together share one bit.
    pub fn bit(self) -> u32 {
        use SystemValue::*;
        let group = match self {
            HeartRate | HeartRateMeasuredAt => HeartRate as u32,
            BatteryPercent | BatteryMillivolts => BatteryPercent as u32,
            WeatherTemperature
            | WeatherCondition
            | WeatherLow
            | WeatherHigh
            | WeatherReceivedAt
            | WeatherSunrise
            | WeatherSunset
            | WeatherLocation
            | WeatherForecastLength
            | WeatherForecastCondition
            | WeatherForecastLow
            | WeatherForecastHigh => WeatherTemperature as u32,
            MusicIsPlaying | MusicArtist | MusicTrack | MusicPosition | MusicLength => MUSIC_BIT,
            NavigationTurn | NavigationInstruction | NavigationDistance | NavigationProgressPercent => {
                NAVIGATION_BIT
            }
            InstalledAppCount | InstalledAppName | InstalledAppId | InstalledAppIcon => INSTALLED_APPS_BIT,
            ScreenTimeoutSeconds | AlwaysOnScreen | Chimes | ThemePreset => SETTINGS_BIT,
            NotificationCount | NotificationId | NotificationApp | NotificationTitle | NotificationBody
            | NotificationTime => UnreadNotifications as u32,
            other => other as u32,
        };
        1 << group
    }

    /// Whether the value is a list, read one element at a time with `ReadSystemElement`.
    pub fn is_list(self) -> bool {
        use SystemValue::*;
        matches!(
            self,
            WeatherForecastCondition
                | WeatherForecastLow
                | WeatherForecastHigh
                | InstalledAppName
                | InstalledAppId
                | InstalledAppIcon
                | NotificationId
                | NotificationApp
                | NotificationTitle
                | NotificationBody
                | NotificationTime
        )
    }

    /// Whether the value is text, which the system gives through `SystemValues::read_text`.
    pub fn is_text(self) -> bool {
        use SystemValue::*;
        matches!(
            self,
            WeatherLocation
                | MusicArtist
                | MusicTrack
                | NavigationInstruction
                | InstalledAppName
                | InstalledAppId
                | NotificationApp
                | NotificationTitle
                | NotificationBody
        )
    }
}

// ============================================================================
// Time parts
// ============================================================================

/// A unit a local time is cut back to, with `StartOf`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeUnit {
    Minute,
    Hour,
    Day,
}

impl TimeUnit {
    pub fn from_byte(byte: u8) -> Option<TimeUnit> {
        Some(match byte {
            0 => TimeUnit::Minute,
            1 => TimeUnit::Hour,
            2 => TimeUnit::Day,
            _ => return None,
        })
    }
}

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
    RequestNavigationState,
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
            29 => RequestNavigationState,
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
            | RequestNavigationState
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
    /// Pops an index and pushes that element of a list value, such as one day of the forecast, or none.
    ReadSystemElement(SystemValue),
    ReadEvent(u8),
    /// Pushes a text field of the event, such as a notification's title. Handlers only.
    ReadEventText(u8),
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
    /// Runs one of the app's functions, which takes its arguments from the stack and leaves its result.
    Call(u8),
    /// Pushes the number of the row of a Repeat the code runs for, or 0 outside one.
    ReadRow,
    TimePart(TimePart),
    /// Takes an instant and pushes the start of its local minute, hour or day.
    StartOf(TimeUnit),
    FormatNumber(u8),
    /// Formats a duration with the pattern in this string; see `pattern`.
    FormatDuration(u16),
    /// Formats a local time with the pattern in this string; see `pattern`.
    FormatTime(u16),
    Join,
    /// Takes a count and pushes text made of the first `count` elements of an array of character codes.
    TextFromCodes(u8),
    /// Takes text and pushes it in capitals.
    ToUpper,
    /// Takes text and pushes how many characters it has.
    TextLength,
    /// Takes text, a start and an end, and pushes the characters between them, as JavaScript's `slice`.
    SliceText,
    /// Takes text, a width and a character code, and pushes the text padded in front up to the width.
    PadStart,
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
            | ReadSystem(_) | ReadEvent(_) | ReadEventText(_) | ReadRow => (0, 1),
            ReadSystemElement(_) => (1, 1),
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
            Negate | Absolute | BitNot | Not | IsNone | TimePart(_) | StartOf(_) | TextFromCodes(_)
            | ToUpper | TextLength | FormatNumber(_) | FormatDuration(_) | FormatTime(_) => (1, 1),
            SliceText | PadStart => (3, 1),
            // The function's own header gives what a call takes and leaves; the verifier reads it there.
            Jump(_) | Call(_) => (0, 0),
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
        0x0E => (ReadEventText(byte()?), 1),
        0x0F => (
            ReadSystemElement(SystemValue::from_byte(byte()?).ok_or(unknown(byte()?))?),
            1,
        ),
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
        0x2C => (Call(byte()?), 1),
        0x2D => (ReadRow, 0),
        0x30 => (
            TimePart(self::TimePart::from_byte(byte()?).ok_or(unknown(byte()?))?),
            1,
        ),
        0x31 => (StartOf(TimeUnit::from_byte(byte()?).ok_or(unknown(byte()?))?), 1),
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
        0x44 => (TextFromCodes(byte()?), 1),
        0x45 => (ToUpper, 0),
        0x46 => (TextLength, 0),
        0x47 => (SliceText, 0),
        0x48 => (PadStart, 0),
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
            ReadEventText(field) => put(&[0x0E, field]),
            ReadSystemElement(value) => put(&[0x0F, value as u8]),
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
            Call(function) => put(&[0x2C, function]),
            ReadRow => put(&[0x2D]),
            TimePart(part) => put(&[0x30, part as u8]),
            StartOf(unit) => put(&[0x31, unit as u8]),
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
            TextFromCodes(array) => put(&[0x44, array]),
            ToUpper => put(&[0x45]),
            TextLength => put(&[0x46]),
            SliceText => put(&[0x47]),
            PadStart => put(&[0x48]),
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
