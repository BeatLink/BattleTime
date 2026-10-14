//! The types the compiler infers, and the numbers that names such as fonts, icons and swipe
//! directions are stored as.
//!
//! The bytecode carries no type tags, so every value's type is known here. docs/compiler.md lists
//! each number mapping; the runtime must read them the same way.

use neotime_runtime::resources::ImageReference;
use neotime_runtime::ValueType;

// ============================================================================
// Types
// ============================================================================

/// What a single value holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A whole number; instants and durations are numbers too.
    Number,
    Bool,
    /// Text that may live in the call's text buffer, such as a formatted number.
    Text,
    /// Text that is always a string of the image, so it can be compared and stored.
    Name,
    Color,
    /// A shared library icon, by its number.
    Icon,
    /// One of a fixed set of names, stored as its position in the set.
    Choice(ChoiceSet),
    /// The `null` literal, before it meets the type it is compared with or stored in.
    Null,
}

/// The type of a single value: its kind, and whether it may also be the empty optional.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Type {
    pub kind: Kind,
    pub nullable: bool,
}

impl Type {
    pub const NUMBER: Type = Type::plain(Kind::Number);
    pub const BOOL: Type = Type::plain(Kind::Bool);
    pub const TEXT: Type = Type::plain(Kind::Text);
    pub const NAME: Type = Type::plain(Kind::Name);
    pub const COLOR: Type = Type::plain(Kind::Color);
    pub const ICON: Type = Type::plain(Kind::Icon);
    pub const NULL: Type = Type {
        kind: Kind::Null,
        nullable: true,
    };

    pub const fn plain(kind: Kind) -> Type {
        Type {
            kind,
            nullable: false,
        }
    }

    pub const fn choice(set: ChoiceSet) -> Type {
        Type::plain(Kind::Choice(set))
    }

    pub fn or_null(self) -> Type {
        Type {
            kind: self.kind,
            nullable: true,
        }
    }

    pub fn is_text(self) -> bool {
        matches!(self.kind, Kind::Text | Kind::Name)
    }

    /// The type both branches of a `? :` share, if they share one.
    pub fn unify(self, other: Type) -> Option<Type> {
        let nullable = self.nullable || other.nullable;
        let kind = match (self.kind, other.kind) {
            (Kind::Null, kind) | (kind, Kind::Null) => kind,
            (Kind::Name, Kind::Text) | (Kind::Text, Kind::Name) => Kind::Text,
            (first, second) if first == second => first,
            _ => return None,
        };
        Some(Type { kind, nullable })
    }

    /// Whether a value of this type may be used where `target` is expected, ignoring null as docs/compiler.md says.
    pub fn fits(self, target: Type) -> bool {
        if self.kind == Kind::Null {
            return target.nullable;
        }
        self.kind == target.kind || (self.kind == Kind::Name && target.kind == Kind::Text)
    }

    /// The type a state variable of this type is stored as.
    pub fn value_type(self, hint: NumberHint) -> ValueType {
        match self.kind {
            Kind::Bool => ValueType::Bool,
            Kind::Color => ValueType::Color,
            Kind::Text | Kind::Name => ValueType::Text,
            Kind::Number => match hint {
                NumberHint::Instant => ValueType::Instant,
                NumberHint::Duration => ValueType::Duration,
                NumberHint::Plain => ValueType::Int,
            },
            Kind::Icon | Kind::Choice(_) | Kind::Null => ValueType::Int,
        }
    }

    pub fn describe(self) -> String {
        let base = match self.kind {
            Kind::Number => "a number".to_string(),
            Kind::Bool => "a boolean".to_string(),
            Kind::Text => "text".to_string(),
            Kind::Name => "fixed text".to_string(),
            Kind::Color => "a colour".to_string(),
            Kind::Icon => "an icon".to_string(),
            Kind::Choice(set) => format!("a {}", set.description()),
            Kind::Null => "null".to_string(),
        };
        if self.nullable && self.kind != Kind::Null {
            format!("{base} or null")
        } else {
            base
        }
    }
}

/// Whether a number is an instant or a duration, which only matters for the state's type byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumberHint {
    Plain,
    Instant,
    Duration,
}

// ============================================================================
// Ranges
// ============================================================================

/// The lowest and highest a value can be, when the compiler can tell, for choosing array widths.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Range {
    pub lowest: i64,
    pub highest: i64,
}

impl Range {
    pub const ANY: Range = Range {
        lowest: i64::MIN,
        highest: i64::MAX,
    };

    pub fn exactly(value: i64) -> Range {
        Range {
            lowest: value,
            highest: value,
        }
    }

    pub fn union(self, other: Range) -> Range {
        Range {
            lowest: self.lowest.min(other.lowest),
            highest: self.highest.max(other.highest),
        }
    }

    /// The range of a sum; any when either side is unknown or the ends overflow.
    pub fn sum_with(self, other: Range) -> Range {
        self.combine(other, |a, b| a.checked_add(b))
    }

    pub fn difference_with(self, other: Range) -> Range {
        let negated = match (other.highest.checked_neg(), other.lowest.checked_neg()) {
            (Some(lowest), Some(highest)) => Range { lowest, highest },
            _ => return Range::ANY,
        };
        self.sum_with(negated)
    }

    pub fn product_with(self, other: Range) -> Range {
        self.combine(other, |a, b| a.checked_mul(b))
    }

    /// Applies an operation to every pair of ends, and spans the results.
    fn combine(self, other: Range, operation: impl Fn(i64, i64) -> Option<i64>) -> Range {
        if self == Range::ANY || other == Range::ANY {
            return Range::ANY;
        }
        let mut result: Option<Range> = None;
        for a in [self.lowest, self.highest] {
            for b in [other.lowest, other.highest] {
                let Some(value) = operation(a, b) else {
                    return Range::ANY;
                };
                let end = Range::exactly(value);
                result = Some(result.map_or(end, |range| range.union(end)));
            }
        }
        result.unwrap_or(Range::ANY)
    }

    /// The fewest bytes, 1, 2, 4 or 8, that hold every value in the range.
    pub fn element_bytes(self) -> u8 {
        let fits = |lowest: i64, highest: i64| self.lowest >= lowest && self.highest <= highest;
        if fits(i8::MIN as i64, i8::MAX as i64) {
            1
        } else if fits(i16::MIN as i64, i16::MAX as i64) {
            2
        } else if fits(i32::MIN as i64, i32::MAX as i64) {
            4
        } else {
            8
        }
    }

    /// The range of values stored in this many bytes.
    pub fn of_width(element_bytes: u8) -> Range {
        match element_bytes {
            1 => Range {
                lowest: i8::MIN as i64,
                highest: i8::MAX as i64,
            },
            2 => Range {
                lowest: i16::MIN as i64,
                highest: i16::MAX as i64,
            },
            4 => Range {
                lowest: i32::MIN as i64,
                highest: i32::MAX as i64,
            },
            _ => Range::ANY,
        }
    }
}

// ============================================================================
// Choice sets
// ============================================================================

/// A fixed set of names that is stored as numbers: the name's position in the set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChoiceSet {
    Font,
    Align,
    LongMode,
    SwipeDirection,
    VibrationPattern,
    MusicCommand,
    CallCommand,
    WeatherCondition,
    NavigationTurn,
    /// How a container spreads its children along it, for `justify`.
    Justify,
    /// Where a container puts its children across it, for `align_items`.
    AlignItems,
    /// A Label's `text_align`, stored as the matching `align` number.
    TextAlign,
    CellShape,
    HandStyle,
    /// The `kind` of a motion event.
    MotionKind,
    /// The `kind` of a call event.
    CallKind,
    /// How the launcher lays out its groups.
    LauncherStyle,
    /// How the launcher draws a group's apps.
    LauncherView,
}

impl ChoiceSet {
    pub fn names(self) -> &'static [&'static str] {
        match self {
            ChoiceSet::Justify => &["start", "center", "end", "space_between"],
            ChoiceSet::AlignItems => &["start", "center", "end", "space_between", "stretch"],
            ChoiceSet::TextAlign => &["left", "center", "right"],
            ChoiceSet::MotionKind => &["wrist_raise", "shake", "tap", "double_tap", "still", "moving"],
            ChoiceSet::CallKind => &["incoming", "answered", "ended"],
            ChoiceSet::LauncherStyle => &["list", "folders", "sections", "pages"],
            ChoiceSet::LauncherView => &["list", "grid"],
            ChoiceSet::CellShape => &["square", "rounded", "round"],
            ChoiceSet::HandStyle => &["line", "tapered", "rounded"],
            ChoiceSet::Font => &[
                "small",
                "normal",
                "medium",
                "large",
                "huge",
                "giant",
                "seven_segment",
                "dot_matrix",
                "condensed",
            ],
            ChoiceSet::Align => &["start", "center", "end"],
            ChoiceSet::LongMode => &["wrap", "scroll", "clip"],
            ChoiceSet::SwipeDirection => &["up", "down", "left", "right"],
            ChoiceSet::VibrationPattern => {
                &["tap", "double_tap", "notification", "alarm", "success", "failure"]
            }
            ChoiceSet::MusicCommand => &["play", "pause", "next", "previous", "volume_up", "volume_down"],
            ChoiceSet::CallCommand => &["answer", "reject", "mute"],
            ChoiceSet::WeatherCondition => &[
                "clear",
                "few_clouds",
                "clouds",
                "heavy_clouds",
                "rain",
                "showers",
                "thunderstorm",
                "snow",
                "mist",
            ],
            ChoiceSet::NavigationTurn => &[
                "straight",
                "left",
                "right",
                "slight_left",
                "slight_right",
                "sharp_left",
                "sharp_right",
                "u_turn",
                "roundabout",
                "arrive",
            ],
        }
    }

    pub fn number_of(self, name: &str) -> Option<i64> {
        if matches!(self, ChoiceSet::Justify | ChoiceSet::AlignItems) && name == "between" {
            return Some(3);
        }
        self.names()
            .iter()
            .position(|candidate| *candidate == name)
            .map(|position| position as i64)
    }

    pub fn description(self) -> &'static str {
        match self {
            ChoiceSet::Font => "font",
            ChoiceSet::Align => "alignment",
            ChoiceSet::LongMode => "long text mode",
            ChoiceSet::SwipeDirection => "swipe direction",
            ChoiceSet::VibrationPattern => "vibration pattern",
            ChoiceSet::MusicCommand => "music command",
            ChoiceSet::CallCommand => "call command",
            ChoiceSet::WeatherCondition => "weather condition",
            ChoiceSet::NavigationTurn => "navigation turn",
            ChoiceSet::Justify => "way to spread children",
            ChoiceSet::AlignItems => "way to align children",
            ChoiceSet::TextAlign => "text alignment",
            ChoiceSet::MotionKind => "kind of motion",
            ChoiceSet::CallKind => "kind of call event",
            ChoiceSet::LauncherStyle => "launcher style",
            ChoiceSet::LauncherView => "launcher view",
            ChoiceSet::CellShape => "cell shape",
            ChoiceSet::HandStyle => "hand style",
        }
    }

    /// The set a type name from `neotime` stands for.
    pub fn for_type_name(name: &str) -> Option<ChoiceSet> {
        Some(match name {
            "Font" => ChoiceSet::Font,
            "SwipeDirection" => ChoiceSet::SwipeDirection,
            "VibrationPattern" => ChoiceSet::VibrationPattern,
            "MusicCommand" => ChoiceSet::MusicCommand,
            "CallCommand" => ChoiceSet::CallCommand,
            "WeatherCondition" => ChoiceSet::WeatherCondition,
            "NavigationTurn" => ChoiceSet::NavigationTurn,
            "MotionKind" => ChoiceSet::MotionKind,
            _ => return None,
        })
    }
}

// ============================================================================
// Icons and lengths
// ============================================================================

/// The shared library's icons, numbered in this order; icons added later go at the end so numbers stay.
pub const ICON_NAMES: [&str; 66] = [
    "alarm",
    "backspace",
    "battery",
    "bell",
    "bell_off",
    "bluetooth",
    "calculator",
    "charging",
    "chart",
    "check",
    "clock",
    "close",
    "clouds",
    "dice",
    "few_clouds",
    "flag",
    "flashlight",
    "game",
    "heart",
    "heavy_clouds",
    "level",
    "clear",
    "metronome",
    "minus",
    "mist",
    "moon",
    "music",
    "navigation",
    "next",
    "notification",
    "pause",
    "phone",
    "play",
    "plus",
    "previous",
    "rain",
    "settings",
    "showers",
    "snow",
    "steps",
    "stop",
    "stopwatch",
    "sun",
    "thunderstorm",
    "timer",
    "turn_arrive",
    "turn_left",
    "turn_right",
    "turn_roundabout",
    "turn_sharp_left",
    "turn_sharp_right",
    "turn_slight_left",
    "turn_slight_right",
    "turn_straight",
    "turn_u_turn",
    "volume_down",
    "volume_up",
    "weather",
    "none",
    "weather_night_clear",
    "weather_night_cloudy",
    "pine_logo",
    "info",
    "remote",
    "note",
    "qr_code",
];

/// An icon's value: a reference to its image in the shared library, which numbers icons in this order.
pub fn icon_number(name: &str) -> Option<i64> {
    ICON_NAMES
        .iter()
        .position(|candidate| *candidate == name)
        .map(|position| ImageReference::SharedLibrary(position as u16).to_value() as i64)
}

/// A length written as a percentage of the parent, such as `"100%"`, is stored as minus the percentage.
pub fn percentage_length(text: &str) -> Option<i64> {
    let percent: i64 = text.strip_suffix('%')?.parse().ok()?;
    (1..=100).contains(&percent).then_some(-percent)
}

// ============================================================================
// Colours
// ============================================================================

/// Reads `#rrggbb` into RGB565.
pub fn parse_hex_color(text: &str) -> Option<u16> {
    let digits = text.strip_prefix('#')?;
    if digits.len() != 6 || !digits.chars().all(|digit| digit.is_ascii_hexdigit()) {
        return None;
    }
    let channel = |start: usize| u8::from_str_radix(&digits[start..start + 2], 16).ok();
    Some(neotime_runtime::theme::rgb565(
        channel(0)?,
        channel(2)?,
        channel(4)?,
    ))
}
