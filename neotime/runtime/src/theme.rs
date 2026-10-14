//! Theme colours: how an app names a colour from the user's theme, and how the runtime turns that
//! name into the colour it draws.
//!
//! A colour value is either a fixed RGB565 colour or a reference to one of the theme's named
//! colours. References stay as they are in state, constants and expression results, and are only
//! looked up when a widget is drawn, so changing the theme redraws every widget without running any
//! app code again.

// ============================================================================
// Theme colours
// ============================================================================

/// The named colours every theme gives, in the order a theme reference numbers them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeColor {
    /// Behind everything on the screen.
    Background,
    /// Cards, buttons and other raised areas.
    Surface,
    /// Ordinary text and icons.
    Text,
    /// Less important text, such as labels and units.
    Muted,
    /// The highlight colour: selected items, progress, the main button.
    Accent,
    Success,
    Warning,
    Danger,
}

impl ThemeColor {
    pub const COUNT: usize = 8;

    pub const ALL: [ThemeColor; ThemeColor::COUNT] = [
        ThemeColor::Background,
        ThemeColor::Surface,
        ThemeColor::Text,
        ThemeColor::Muted,
        ThemeColor::Accent,
        ThemeColor::Success,
        ThemeColor::Warning,
        ThemeColor::Danger,
    ];

    /// The colour's name in app source, such as `"accent"`.
    pub fn name(self) -> &'static str {
        match self {
            ThemeColor::Background => "background",
            ThemeColor::Surface => "surface",
            ThemeColor::Text => "text",
            ThemeColor::Muted => "muted",
            ThemeColor::Accent => "accent",
            ThemeColor::Success => "success",
            ThemeColor::Warning => "warning",
            ThemeColor::Danger => "danger",
        }
    }

    pub fn from_name(name: &str) -> Option<ThemeColor> {
        Self::ALL.into_iter().find(|color| color.name() == name)
    }
}

// ============================================================================
// Colour values
// ============================================================================

/// Marks a colour value as a reference to a theme colour; the low bits give which one.
const THEME_REFERENCE: i64 = 1 << 16;

/// What a colour value stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorValue {
    Fixed(u16),
    Theme(ThemeColor),
}

impl ColorValue {
    /// The 64-bit value a colour is stored as: RGB565 itself, or a marked theme colour number.
    pub fn to_value(self) -> i64 {
        match self {
            ColorValue::Fixed(rgb565) => rgb565 as i64,
            ColorValue::Theme(color) => THEME_REFERENCE | color as i64,
        }
    }

    /// Reads a colour value, or returns `None` if it is neither form.
    pub fn from_value(value: i64) -> Option<ColorValue> {
        if (0..=0xFFFF).contains(&value) {
            return Some(ColorValue::Fixed(value as u16));
        }
        let index = value.checked_sub(THEME_REFERENCE)?;
        let color = *ThemeColor::ALL.get(usize::try_from(index).ok()?)?;
        Some(ColorValue::Theme(color))
    }

    /// The RGB565 colour to draw in the given theme.
    pub fn resolve(self, theme: &Theme) -> u16 {
        match self {
            ColorValue::Fixed(rgb565) => rgb565,
            ColorValue::Theme(color) => theme.color(color),
        }
    }
}

/// Packs 8-bit red, green and blue into RGB565.
pub const fn rgb565(red: u8, green: u8, blue: u8) -> u16 {
    ((red as u16 >> 3) << 11) | ((green as u16 >> 2) << 5) | (blue as u16 >> 3)
}

// ============================================================================
// Themes
// ============================================================================

/// One theme: an RGB565 colour for each named colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Theme {
    colors: [u16; ThemeColor::COUNT],
}

impl Theme {
    pub const fn new(colors: [u16; ThemeColor::COUNT]) -> Theme {
        Theme { colors }
    }

    pub fn color(&self, color: ThemeColor) -> u16 {
        self.colors[color as usize]
    }

    pub fn set_color(&mut self, color: ThemeColor, rgb565: u16) {
        self.colors[color as usize] = rgb565;
    }

    /// The colours in `ThemeColor` order, as a theme is saved in settings.
    pub fn colors(&self) -> [u16; ThemeColor::COUNT] {
        self.colors
    }
}

/// The theme the watch starts with: light text on black, with a teal accent.
pub const DEFAULT_THEME: Theme = Theme::new([
    rgb565(0, 0, 0),
    rgb565(40, 44, 52),
    rgb565(255, 255, 255),
    rgb565(140, 146, 156),
    rgb565(0, 170, 160),
    rgb565(60, 190, 90),
    rgb565(240, 170, 30),
    rgb565(230, 70, 60),
]);

// ============================================================================
// Preset themes
// ============================================================================

/// A theme the user can pick by name, such as from the companion app.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PresetTheme {
    pub name: &'static str,
    pub theme: Theme,
}

/// Every preset, the default first, in the order a settings screen lists them.
pub const PRESET_THEMES: [PresetTheme; 6] = [
    PresetTheme {
        name: "dark",
        theme: DEFAULT_THEME,
    },
    PresetTheme {
        name: "light",
        theme: Theme::new([
            rgb565(255, 255, 255),
            rgb565(222, 226, 230),
            rgb565(0, 0, 0),
            rgb565(104, 110, 120),
            rgb565(0, 128, 150),
            rgb565(30, 150, 70),
            rgb565(200, 120, 0),
            rgb565(210, 40, 40),
        ]),
    },
    PresetTheme {
        name: "ocean",
        theme: Theme::new([
            rgb565(0, 10, 30),
            rgb565(10, 40, 80),
            rgb565(220, 240, 255),
            rgb565(110, 150, 190),
            rgb565(40, 160, 255),
            rgb565(40, 210, 170),
            rgb565(250, 200, 60),
            rgb565(255, 90, 90),
        ]),
    },
    PresetTheme {
        name: "forest",
        theme: Theme::new([
            rgb565(8, 20, 10),
            rgb565(30, 60, 36),
            rgb565(230, 240, 220),
            rgb565(140, 160, 130),
            rgb565(120, 200, 80),
            rgb565(90, 210, 120),
            rgb565(230, 190, 60),
            rgb565(230, 90, 60),
        ]),
    },
    PresetTheme {
        name: "sunset",
        theme: Theme::new([
            rgb565(24, 8, 16),
            rgb565(70, 30, 50),
            rgb565(255, 236, 220),
            rgb565(190, 140, 140),
            rgb565(255, 120, 60),
            rgb565(120, 210, 120),
            rgb565(255, 200, 60),
            rgb565(240, 60, 90),
        ]),
    },
    PresetTheme {
        name: "contrast",
        theme: Theme::new([
            rgb565(0, 0, 0),
            rgb565(40, 40, 40),
            rgb565(255, 255, 255),
            rgb565(200, 200, 200),
            rgb565(255, 220, 0),
            rgb565(0, 255, 0),
            rgb565(255, 160, 0),
            rgb565(255, 0, 0),
        ]),
    },
];

/// The preset with this name, if there is one.
pub fn preset_named(name: &str) -> Option<&'static PresetTheme> {
    PRESET_THEMES.iter().find(|preset| preset.name == name)
}

/// The preset whose colours these are, so a theme picked by name is reported by name.
pub fn preset_matching(theme: &Theme) -> Option<&'static PresetTheme> {
    PRESET_THEMES.iter().find(|preset| preset.theme == *theme)
}
