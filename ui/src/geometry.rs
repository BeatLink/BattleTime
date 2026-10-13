//! Places on the screen and colours, the values every drawing call takes.

// ============================================================================
// Rectangles
// ============================================================================

/// A rectangle of whole pixels, from its top-left corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Rect {
    /// The smallest rectangle holding both.
    pub fn union(self, other: Rect) -> Rect {
        let left = self.x.min(other.x);
        let top = self.y.min(other.y);
        let right = (self.x + self.width).max(other.x + other.width);
        let bottom = (self.y + self.height).max(other.y + other.height);
        Rect {
            x: left,
            y: top,
            width: right - left,
            height: bottom - top,
        }
    }

    /// The part both share, or nothing if they do not overlap.
    pub fn intersect(self, other: Rect) -> Option<Rect> {
        let left = self.x.max(other.x);
        let top = self.y.max(other.y);
        let right = (self.x + self.width).min(other.x + other.width);
        let bottom = (self.y + self.height).min(other.y + other.height);
        if right <= left || bottom <= top {
            return None;
        }
        Some(Rect {
            x: left,
            y: top,
            width: right - left,
            height: bottom - top,
        })
    }

    /// Whether the rectangle covers no pixel at all.
    pub fn is_empty(self) -> bool {
        self.width <= 0 || self.height <= 0
    }

    /// The row just below the rectangle.
    pub fn bottom(self) -> i32 {
        self.y + self.height
    }

    /// The column just right of the rectangle.
    pub fn right(self) -> i32 {
        self.x + self.width
    }
}

// ============================================================================
// Points between pixels
// ============================================================================

/// A point with a fraction of a pixel, where pixel centres sit at half steps.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct PointF {
    pub x: f32,
    pub y: f32,
}

impl PointF {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

// ============================================================================
// Colours
// ============================================================================

/// Red, green, blue and opacity, eight bits each.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color(pub u8, pub u8, pub u8, pub u8);

/// The opacity of a colour that hides what is behind it.
pub const OPAQUE: u8 = 255;

impl Color {
    /// The same colour with its opacity scaled by `opacity`.
    pub fn with_alpha(self, opacity: u8) -> Color {
        Color(self.0, self.1, self.2, scale(self.3, opacity))
    }
}

/// `value` scaled by `share` out of 255, rounding down.
pub fn scale(value: u8, share: u8) -> u8 {
    (u16::from(value) * u16::from(share) / u16::from(OPAQUE)) as u8
}
