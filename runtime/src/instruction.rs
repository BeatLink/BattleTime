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
    /// Steps since the wearer last reset the trip count.
    StepTrip,
    /// How many days the step history holds: today and the six before it.
    StepDayCount,
    /// List: each day's steps, read by how many days back it is, today first.
    StepDayTotal,
    /// How many hours the step history holds, 24 for each day.
    StepHourCount,
    /// List: the steps in each hour, read at `days_back * 24 + hour`.
    StepHourSteps,
    /// Text: the firmware's version and commit, such as `0.1.0+1b19aed`.
    FirmwareVersion,
    /// When the watch last started, so `now - started_at` is how long it has been running.
    StartedAt,
    FreeStorageKilobytes,
    /// Text: why the watch last restarted, such as `watchdog` or `Fault at 0x0001a2b4`.
    ResetReason,
    /// Whether the watch takes firmware updates.
    AllowsUpdates,
    /// 0 only the button wakes the screen, 1 a tap does, 2 a double tap does.
    WakeOnTouch,
    /// Whether alarms and timers still ring in do not disturb.
    AlarmsInDoNotDisturb,
    /// How many apps and faces are installed, turned on or off, in id order.
    PackageCount,
    /// Text list: each installed package's name, then its id.
    PackageName,
    PackageId,
    /// List: whether each package is turned on.
    PackageEnabled,
    /// List: whether each package is a watch face.
    PackageIsFace,
    /// List: the heap each package needs while it is open, in bytes, as the package manager checks it.
    PackageMemory,
    /// List: why turning each package the other way would be refused, 0 when it would not; see `Refusal`.
    PackageRefusal,
    /// The most heap an open app may need, in bytes.
    AppMemoryBudget,
    /// How the launcher lays out its groups: 0 one list, 1 folders, 2 sections, 3 pages.
    LauncherStyle,
    /// How a group's apps are drawn: 0 a list, 1 a grid.
    LauncherView,
    /// How many groups hold at least one app, in the launcher's order.
    LauncherGroupCount,
    /// Text list: each group's name.
    LauncherGroupName,
    /// List: each group's icon, as an image reference: a shared library icon, or its first app's icon.
    LauncherGroupIcon,
    /// List: the place in `installed_apps` of each group's first app, and how many apps it holds.
    LauncherGroupFirst,
    LauncherGroupSize,
}

/// Bits 19 and 20 are free because every weather value uses bit 18, so music and navigation take them.
const MUSIC_BIT: u32 = 19;
const NAVIGATION_BIT: u32 = 20;
/// Bit 21 is free too, as every value numbered 21 to 31 is in a group.
const INSTALLED_APPS_BIT: u32 = 21;
/// The settings added after the first three share one bit, since a change to settings redraws them together.
const SETTINGS_BIT: u32 = 22;
/// The values a system info screen shows share one bit, as they seldom change.
const SYSTEM_INFO_BIT: u32 = 23;

impl SystemValue {
    pub const COUNT: usize = 81;

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
            54 => StepTrip,
            55 => StepDayCount,
            56 => StepDayTotal,
            57 => StepHourCount,
            58 => StepHourSteps,
            59 => FirmwareVersion,
            60 => StartedAt,
            61 => FreeStorageKilobytes,
            62 => ResetReason,
            63 => AllowsUpdates,
            64 => WakeOnTouch,
            65 => AlarmsInDoNotDisturb,
            66 => PackageCount,
            67 => PackageName,
            68 => PackageId,
            69 => PackageEnabled,
            70 => PackageIsFace,
            71 => PackageMemory,
            72 => PackageRefusal,
            73 => AppMemoryBudget,
            74 => LauncherStyle,
            75 => LauncherView,
            76 => LauncherGroupCount,
            77 => LauncherGroupName,
            78 => LauncherGroupIcon,
            79 => LauncherGroupFirst,
            80 => LauncherGroupSize,
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
            InstalledAppCount | InstalledAppName | InstalledAppId | InstalledAppIcon | PackageCount
            | PackageName | PackageId | PackageEnabled | PackageIsFace | PackageMemory | PackageRefusal
            | LauncherStyle | LauncherView | LauncherGroupCount | LauncherGroupName | LauncherGroupIcon
            | LauncherGroupFirst | LauncherGroupSize => INSTALLED_APPS_BIT,
            ScreenTimeoutSeconds | AlwaysOnScreen | Chimes | ThemePreset | AllowsUpdates | WakeOnTouch
            | AlarmsInDoNotDisturb => SETTINGS_BIT,
            NotificationCount | NotificationId | NotificationApp | NotificationTitle | NotificationBody
            | NotificationTime => UnreadNotifications as u32,
            StepTrip | StepDayCount | StepDayTotal | StepHourCount | StepHourSteps => StepsToday as u32,
            FirmwareVersion | StartedAt | FreeStorageKilobytes | ResetReason | AppMemoryBudget => {
                SYSTEM_INFO_BIT
            }
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
                | StepDayTotal
                | StepHourSteps
                | PackageName
                | PackageId
                | PackageEnabled
                | PackageIsFace
                | PackageMemory
                | PackageRefusal
                | LauncherGroupName
                | LauncherGroupIcon
                | LauncherGroupFirst
                | LauncherGroupSize
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
                | FirmwareVersion
                | ResetReason
                | PackageName
                | PackageId
                | LauncherGroupName
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

/// A question `AskArray` answers about one array, natively instead of in a bytecode loop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArrayQuestion {
    /// Takes a value and pushes how many elements equal it.
    CountOf,
    /// Takes a value and `n`, and pushes the index of the `n`th element equal to it, counting from 0, or -1.
    NthIndexOf,
    /// Takes a grid's width, a cell and a value, and pushes the longest line of that value through the cell
    /// across, down or along either diagonal, counting the cell itself as holding it. Lines stop at the edges.
    LineThrough,
    /// Takes a grid's width and a direction, and pushes 1 if sliding the tiles that way would change the grid.
    /// Direction 4 asks about every direction at once.
    SlideChanges,
    /// Takes a grid's width and a direction, and pushes the points the merges of that slide score.
    SlidePoints,
    /// Pushes how many pairs of elements are out of order, the earlier one greater; its parity says whether a
    /// sliding puzzle can be solved.
    Inversions,
}

impl ArrayQuestion {
    pub fn from_byte(byte: u8) -> Option<ArrayQuestion> {
        Some(match byte {
            0 => ArrayQuestion::CountOf,
            1 => ArrayQuestion::NthIndexOf,
            2 => ArrayQuestion::LineThrough,
            3 => ArrayQuestion::SlideChanges,
            4 => ArrayQuestion::SlidePoints,
            5 => ArrayQuestion::Inversions,
            _ => return None,
        })
    }

    /// How many values the question takes from the stack.
    pub fn argument_count(self) -> usize {
        match self {
            ArrayQuestion::Inversions => 0,
            ArrayQuestion::CountOf => 1,
            ArrayQuestion::NthIndexOf | ArrayQuestion::SlideChanges | ArrayQuestion::SlidePoints => 2,
            ArrayQuestion::LineThrough => 3,
        }
    }
}

/// Work `WorkArray` does natively on two arrays of the same length.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArrayWork {
    /// Pushes 1 if the two arrays hold the same elements.
    Equal,
    /// Takes a grid's width and a live value, and writes the next generation of Conway's Game of Life on a grid
    /// that wraps at its edges: any element not 0 is alive, and each live cell of the result holds the value.
    LifeStep,
    /// Takes a grid's width and a direction, and writes the grid with its tiles slid that way, each pair of equal
    /// tiles merging once into one a level higher, as in 2048.
    Slide,
    /// Takes a value and writes the source moved one place towards its start, the value in the last place.
    PushLast,
    /// Takes a value and writes the source moved one place towards its end, the value in the first place.
    PushFirst,
    /// Takes a first place and a count, and writes the source with the elements in those places in a random
    /// order.
    Shuffle,
    /// Takes a path and writes the first bytes of that file, one per element, with 0 past its end or for a
    /// missing file; `source` is the destination.
    FileBytes,
}

impl ArrayWork {
    pub fn from_byte(byte: u8) -> Option<ArrayWork> {
        Some(match byte {
            0 => ArrayWork::Equal,
            1 => ArrayWork::LifeStep,
            2 => ArrayWork::Slide,
            3 => ArrayWork::PushLast,
            4 => ArrayWork::PushFirst,
            5 => ArrayWork::Shuffle,
            6 => ArrayWork::FileBytes,
            _ => return None,
        })
    }

    /// Whether the work writes its destination, which only handlers may.
    pub fn writes(self) -> bool {
        self != ArrayWork::Equal
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
    ResetStepTrip,
    SetSetting,
    SetDoNotDisturb,
    Log,
    RequestMusicState,
    SetHeartRateLed,
    SetBluetooth,
    RequestNavigationState,
    /// Turns an installed app or face on or off by its id, once the package manager's checks allow it.
    SetAppEnabled,
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
            22 => ResetStepTrip,
            23 => SetSetting,
            24 => SetDoNotDisturb,
            25 => Log,
            26 => RequestMusicState,
            27 => SetHeartRateLed,
            28 => SetBluetooth,
            29 => RequestNavigationState,
            30 => SetAppEnabled,
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
            | ResetStepTrip => 0,
            Vibrate | VibratePattern | SetBrightness | KeepScreenOn | Go | OpenApp | ShowToast
            | DismissNotification | MusicControl | CallControl | SetDoNotDisturb | Log | FindPhone
            | SetBluetooth => 1,
            Notify | SendToPhone | SetSetting | SetAppEnabled => 2,
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
    /// Runs one of the shared library's functions, which takes `arguments` values and leaves `results`.
    CallLibrary {
        function: u16,
        arguments: u8,
        results: u8,
    },
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
    /// Takes a number, a count of decimal places and a flag, and pushes the number as text with that many
    /// places, such as 1250 with 3 places as `1.250`. When the flag is set, trailing zero decimals are left off.
    FormatFixed,
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
    /// Answers a question about an array, taking its arguments and pushing the answer.
    AskArray {
        question: ArrayQuestion,
        array: u8,
    },
    /// Does work that reads `source` and, unless it only compares them, writes `destination`.
    WorkArray {
        work: ArrayWork,
        destination: u8,
        source: u8,
    },
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
            SliceText | PadStart | FormatFixed => (3, 1),
            // The function's own header gives what a call takes and leaves; the verifier reads it there.
            Jump(_) | Call(_) => (0, 0),
            CallLibrary {
                arguments, results, ..
            } => (arguments as usize, results as usize),
            AskArray { question, .. } => (question.argument_count(), 1),
            WorkArray {
                work: ArrayWork::Equal,
                ..
            } => (0, 1),
            WorkArray {
                work: ArrayWork::PushLast | ArrayWork::PushFirst | ArrayWork::FileBytes,
                ..
            } => (1, 0),
            WorkArray { .. } => (2, 0),
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
        0x2E => {
            let operands = bytes(4)?;
            (
                CallLibrary {
                    function: u16::from_le_bytes([operands[0], operands[1]]),
                    arguments: operands[2],
                    results: operands[3],
                },
                4,
            )
        }
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
        0x49 => (FormatFixed, 0),
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
        0x64 => {
            let operands = bytes(2)?;
            let question = ArrayQuestion::from_byte(operands[0]).ok_or(unknown(operands[0]))?;
            (
                AskArray {
                    question,
                    array: operands[1],
                },
                2,
            )
        }
        0x65 => {
            let operands = bytes(3)?;
            let work = ArrayWork::from_byte(operands[0]).ok_or(unknown(operands[0]))?;
            (
                WorkArray {
                    work,
                    destination: operands[1],
                    source: operands[2],
                },
                3,
            )
        }
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
            CallLibrary {
                function,
                arguments,
                results,
            } => {
                put(&[0x2E]);
                put(&function.to_le_bytes());
                put(&[arguments, results]);
            }
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
            FormatFixed => put(&[0x49]),
            WriteState(variable) => put(&[0x50, variable]),
            Effect(kind) => put(&[0x51, kind as u8]),
            ReadElement(variable) => put(&[0x60, variable]),
            WriteElement(variable) => put(&[0x61, variable]),
            CopyArray { destination, source } => put(&[0x62, destination, source]),
            FillArray(variable) => put(&[0x63, variable]),
            AskArray { question, array } => put(&[0x64, question as u8, array]),
            WorkArray {
                work,
                destination,
                source,
            } => put(&[0x65, work as u8, destination, source]),
        }
        encoded
    }
}
