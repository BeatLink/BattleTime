//! The widget tree an app image describes, and the checks it passes at install.
//!
//! Widgets are stored depth first, each with its number of children, so the tree needs no
//! pointers. Each root is one screen. A widget's properties are each a constant, the number of an
//! expression that works the value out, or, for an event, the number of a handler.

use crate::image::{AppImage, ImageError};
use crate::resources::ImageReference;
use crate::theme::ColorValue;

// ============================================================================
// Layout
// ============================================================================

const WIDGET_HEADER_BYTES: usize = 4;
const PROPERTY_BYTES: usize = 6;

/// The deepest a tree may nest, which bounds the stack the checks and the runtime need.
pub const DEEPEST_NESTING: usize = 16;

/// The most widgets an app may have.
pub const MOST_WIDGETS: usize = 128;

// ============================================================================
// Widget kinds
// ============================================================================

/// The widgets in docs/app-reference.md, in the order their kind byte numbers them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WidgetKind {
    Column,
    Row,
    Grid,
    Stack,
    Scroll,
    List,
    Pages,
    Tabs,
    Label,
    RichText,
    Image,
    Icon,
    AnimatedImage,
    Line,
    Arc,
    ArcLabel,
    Bar,
    Spinner,
    Led,
    Chart,
    Scale,
    AnalogClock,
    Table,
    Calendar,
    Button,
    ImageButton,
    ClickArea,
    Switch,
    Checkbox,
    Radio,
    Slider,
    Roller,
    Spinbox,
    Dropdown,
    ButtonMatrix,
    TextArea,
    Keyboard,
    /// A board of coloured cells drawn from one array, such as Life or Snake.
    CellGrid,
}

impl WidgetKind {
    const ALL: [WidgetKind; 38] = {
        use WidgetKind::*;
        [
            Column,
            Row,
            Grid,
            Stack,
            Scroll,
            List,
            Pages,
            Tabs,
            Label,
            RichText,
            Image,
            Icon,
            AnimatedImage,
            Line,
            Arc,
            ArcLabel,
            Bar,
            Spinner,
            Led,
            Chart,
            Scale,
            AnalogClock,
            Table,
            Calendar,
            Button,
            ImageButton,
            ClickArea,
            Switch,
            Checkbox,
            Radio,
            Slider,
            Roller,
            Spinbox,
            Dropdown,
            ButtonMatrix,
            TextArea,
            Keyboard,
            CellGrid,
        ]
    };

    pub fn from_byte(byte: u8) -> Option<WidgetKind> {
        Self::ALL.get(byte as usize).copied()
    }

    /// Whether the widget lays out other widgets inside it.
    pub fn holds_children(self) -> bool {
        use WidgetKind::*;
        matches!(
            self,
            Column | Row | Grid | Stack | Scroll | List | Pages | Tabs | Button | Arc | ClickArea
        )
    }
}

// ============================================================================
// Properties
// ============================================================================

/// What a property's value means, which decides how it is checked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PropertyValue {
    Number,
    Bool,
    Color,
    Text,
    Event,
    /// The number of a state array, given as a constant.
    Array,
    /// One of the app's images or a shared library image, as `ImageReference` numbers them.
    Image,
}

/// Every property a widget can have, in the order their id byte numbers them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Property {
    // Common to every widget.
    X,
    Y,
    Width,
    Height,
    Grow,
    Padding,
    Margin,
    Background,
    Color,
    BorderColor,
    BorderWidth,
    Radius,
    Font,
    Align,
    Visible,
    Animate,
    /// The name of a screen, on the root widget of each screen.
    ScreenName,
    // Content.
    Text,
    Value,
    Minimum,
    Maximum,
    Step,
    Image,
    Icon,
    Time,
    ShowSeconds,
    StartAngle,
    EndAngle,
    LongMode,
    // Events.
    OnClick,
    OnLongPress,
    OnDoubleClick,
    OnPress,
    OnRelease,
    OnDrag,
    OnSwipe,
    OnButton,
    OnChange,
    OnPageChange,
    OnScrollEnd,
    OnShow,
    OnHide,
    // Added after the events, so earlier numbers stay the same.
    Opacity,
    Disabled,
    Gap,
    Justify,
    AlignItems,
    Columns,
    Rows,
    CellSize,
    CellGap,
    CellShape,
    /// The state array holding a CellGrid's cells.
    Cells,
    /// The state array holding a CellGrid's colours.
    Palette,
    /// The state array of text a Roller, Dropdown or ButtonMatrix offers.
    Options,
    /// The state array of x, y pairs a Line joins.
    Points,
    /// The state array of values a Chart draws.
    Series,
    Ticks,
    MajorEvery,
    /// The state array of text a Scale writes at its major ticks.
    Labels,
    /// The state array of tick numbers a Scale leaves out.
    Skip,
    NeedleColor,
    HourColor,
    MinuteColor,
    SecondColor,
    HourLength,
    MinuteLength,
    SecondLength,
    HandStyle,
    Recolor,
    /// The colour of a Scale's major ticks and labels.
    MajorColor,
}

impl Property {
    const ALL: [Property; 71] = {
        use Property::*;
        [
            X,
            Y,
            Width,
            Height,
            Grow,
            Padding,
            Margin,
            Background,
            Color,
            BorderColor,
            BorderWidth,
            Radius,
            Font,
            Align,
            Visible,
            Animate,
            ScreenName,
            Text,
            Value,
            Minimum,
            Maximum,
            Step,
            Image,
            Icon,
            Time,
            ShowSeconds,
            StartAngle,
            EndAngle,
            LongMode,
            OnClick,
            OnLongPress,
            OnDoubleClick,
            OnPress,
            OnRelease,
            OnDrag,
            OnSwipe,
            OnButton,
            OnChange,
            OnPageChange,
            OnScrollEnd,
            OnShow,
            OnHide,
            Opacity,
            Disabled,
            Gap,
            Justify,
            AlignItems,
            Columns,
            Rows,
            CellSize,
            CellGap,
            CellShape,
            Cells,
            Palette,
            Options,
            Points,
            Series,
            Ticks,
            MajorEvery,
            Labels,
            Skip,
            NeedleColor,
            HourColor,
            MinuteColor,
            SecondColor,
            HourLength,
            MinuteLength,
            SecondLength,
            HandStyle,
            Recolor,
            MajorColor,
        ]
    };

    pub fn from_byte(byte: u8) -> Option<Property> {
        Self::ALL.get(byte as usize).copied()
    }

    pub fn value(self) -> PropertyValue {
        use Property::*;
        match self {
            Visible | ShowSeconds => PropertyValue::Bool,
            Background | Color | BorderColor | NeedleColor | HourColor | MinuteColor | SecondColor
            | Recolor => PropertyValue::Color,
            Cells | Palette | Options | Points | Series | Labels | Skip => PropertyValue::Array,
            Image | Icon => PropertyValue::Image,
            Text | ScreenName => PropertyValue::Text,
            OnClick | OnLongPress | OnDoubleClick | OnPress | OnRelease | OnDrag | OnSwipe | OnButton
            | OnChange | OnPageChange | OnScrollEnd | OnShow | OnHide => PropertyValue::Event,
            _ => PropertyValue::Number,
        }
    }

    fn is_common(self) -> bool {
        (self as u8) <= (Property::ScreenName as u8)
    }

    /// Whether a widget of this kind has this property.
    pub fn belongs_to(self, kind: WidgetKind) -> bool {
        use Property::*;
        use WidgetKind as Kind;
        if self.is_common() {
            return true;
        }
        let is_input = matches!(
            kind,
            Kind::Button
                | Kind::ImageButton
                | Kind::ClickArea
                | Kind::Switch
                | Kind::Checkbox
                | Kind::Radio
                | Kind::Slider
                | Kind::Roller
                | Kind::Spinbox
                | Kind::Dropdown
                | Kind::ButtonMatrix
                | Kind::TextArea
                | Kind::Keyboard
        );
        match self {
            Text => matches!(
                kind,
                Kind::Label
                    | Kind::RichText
                    | Kind::ArcLabel
                    | Kind::Button
                    | Kind::Checkbox
                    | Kind::Radio
                    | Kind::TextArea
            ),
            Value => matches!(
                kind,
                Kind::Arc
                    | Kind::Bar
                    | Kind::Led
                    | Kind::Switch
                    | Kind::Checkbox
                    | Kind::Radio
                    | Kind::Slider
                    | Kind::Roller
                    | Kind::Spinbox
                    | Kind::Dropdown
                    | Kind::Pages
                    | Kind::Tabs
                    | Kind::TextArea
                    | Kind::Scale
                    | Kind::ButtonMatrix
            ),
            Minimum | Maximum | Step => {
                matches!(
                    kind,
                    Kind::Arc | Kind::Bar | Kind::Slider | Kind::Spinbox | Kind::Scale | Kind::Chart
                )
            }
            Image => matches!(kind, Kind::Image | Kind::ImageButton | Kind::AnimatedImage),
            Icon => matches!(kind, Kind::Icon | Kind::Button),
            Time | ShowSeconds => kind == Kind::AnalogClock,
            StartAngle | EndAngle => matches!(kind, Kind::Arc | Kind::ArcLabel | Kind::Scale),
            LongMode => kind == Kind::Label,
            Opacity => true,
            Disabled => is_input,
            Gap | Justify | AlignItems => kind.holds_children(),
            Columns => matches!(kind, Kind::Grid | Kind::ButtonMatrix | Kind::CellGrid),
            Rows | CellSize | CellGap | CellShape | Cells | Palette => kind == Kind::CellGrid,
            Options => matches!(kind, Kind::Roller | Kind::Dropdown | Kind::ButtonMatrix),
            Points => kind == Kind::Line,
            Series => kind == Kind::Chart,
            Ticks | MajorEvery | Labels | Skip | NeedleColor | MajorColor => kind == Kind::Scale,
            HourColor | MinuteColor | SecondColor | HourLength | MinuteLength | SecondLength | HandStyle => {
                kind == Kind::AnalogClock
            }
            Recolor => matches!(kind, Kind::Image | Kind::Icon),
            OnClick | OnLongPress | OnDoubleClick | OnPress | OnRelease => {
                is_input || kind.holds_children() || kind == Kind::CellGrid
            }
            OnDrag => matches!(kind, Kind::Slider | Kind::Arc | Kind::CellGrid) || kind.holds_children(),
            OnChange => is_input || kind == Kind::Arc,
            OnPageChange => matches!(kind, Kind::Pages | Kind::Tabs),
            OnScrollEnd => matches!(kind, Kind::Scroll | Kind::List),
            // Screen-wide events go on a screen's root, which the tree check confirms.
            OnSwipe | OnButton | OnShow | OnHide => true,
            _ => false,
        }
    }

    fn is_screen_only(self) -> bool {
        matches!(
            self,
            Property::OnSwipe
                | Property::OnButton
                | Property::OnShow
                | Property::OnHide
                | Property::ScreenName
        )
    }
}

/// Where a property's value comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PropertySource {
    /// A fixed number, colour, flag, or for text the number of a string.
    Constant(i32),
    /// The number of the expression that works the value out.
    Expression(u16),
    /// The number of the handler an event calls.
    Handler(u16),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PropertyEntry {
    pub property: Property,
    pub source: PropertySource,
}

// ============================================================================
// Reading widgets
// ============================================================================

/// One widget as it lies in the image.
#[derive(Clone, Copy, Debug)]
pub struct Widget<'image> {
    pub kind: WidgetKind,
    pub child_count: u8,
    properties: &'image [u8],
}

impl<'image> Widget<'image> {
    fn parse(bytes: &'image [u8]) -> Option<Widget<'image>> {
        let kind = WidgetKind::from_byte(*bytes.first()?)?;
        let child_count = *bytes.get(1)?;
        let property_count = *bytes.get(2)? as usize;
        let properties =
            bytes.get(WIDGET_HEADER_BYTES..WIDGET_HEADER_BYTES + property_count * PROPERTY_BYTES)?;
        if bytes.len() != WIDGET_HEADER_BYTES + properties.len() {
            return None;
        }
        Some(Widget {
            kind,
            child_count,
            properties,
        })
    }

    /// The widget's properties; an entry that cannot be read comes back as `None`.
    pub fn properties(&self) -> impl Iterator<Item = Option<PropertyEntry>> + 'image {
        self.properties
            .as_chunks::<PROPERTY_BYTES>()
            .0
            .iter()
            .map(|bytes| {
                let property = Property::from_byte(bytes[0])?;
                let value = i32::from_le_bytes([bytes[2], bytes[3], bytes[4], bytes[5]]);
                let source = match bytes[1] {
                    0 => PropertySource::Constant(value),
                    1 => PropertySource::Expression(u16::try_from(value).ok()?),
                    2 => PropertySource::Handler(u16::try_from(value).ok()?),
                    _ => return None,
                };
                Some(PropertyEntry { property, source })
            })
    }

    pub fn property(&self, wanted: Property) -> Option<PropertySource> {
        self.properties()
            .flatten()
            .find(|entry| entry.property == wanted)
            .map(|entry| entry.source)
    }
}

/// The widgets of an image, read in place.
#[derive(Clone, Copy, Debug)]
pub struct WidgetTree<'image> {
    image: AppImage<'image>,
}

impl<'image> WidgetTree<'image> {
    pub fn new(image: AppImage<'image>) -> Self {
        WidgetTree { image }
    }

    pub fn widget_count(&self) -> u16 {
        self.image.widget_count()
    }

    pub fn widget(&self, index: u16) -> Option<Widget<'image>> {
        Widget::parse(self.image.widget_bytes(index)?)
    }

    /// The index just past this widget and everything inside it.
    pub fn subtree_end(&self, index: u16) -> Option<u16> {
        let mut next = index;
        let mut still_to_visit: u32 = 1;
        while still_to_visit > 0 {
            let widget = self.widget(next)?;
            still_to_visit = still_to_visit - 1 + widget.child_count as u32;
            next = next.checked_add(1)?;
        }
        Some(next)
    }

    /// The direct children of a widget, in order.
    pub fn children(&self, index: u16) -> impl Iterator<Item = u16> + '_ {
        let child_count = self.widget(index).map_or(0, |widget| widget.child_count);
        let mut next = index + 1;
        (0..child_count).map_while(move |_| {
            let child = next;
            next = self.subtree_end(child)?;
            Some(child)
        })
    }

    /// The root widget of each screen, in order; the first is the screen an app opens on.
    pub fn screens(&self) -> impl Iterator<Item = u16> + '_ {
        let mut next = Some(0u16);
        core::iter::from_fn(move || {
            let root = next.filter(|&root| root < self.widget_count())?;
            next = self.subtree_end(root);
            Some(root)
        })
    }

    /// The screen whose root has this name.
    pub fn screen_named(&self, name: &str) -> Option<u16> {
        self.screens().find(|&root| {
            let named = self.widget(root).and_then(|widget| widget.property(Property::ScreenName));
            matches!(named, Some(PropertySource::Constant(string)) if self.image.string(string as u16) == Some(name))
        })
    }
}

// ============================================================================
// Checking the tree
// ============================================================================

/// What is wrong with the widget tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WidgetProblem {
    Unreadable,
    TooManyWidgets,
    TooDeep,
    ChildrenNotAllowed,
    UnknownProperty,
    PropertyNotOnWidget,
    ScreenPropertyBelowRoot,
    RepeatedProperty,
    ExpressionOutOfRange,
    HandlerOutOfRange,
    HandlerOnNonEvent,
    EventWithoutHandler,
    BadConstant,
    ArrayNotConstant,
}

/// Checks that the widgets form whole trees and every property fits its widget.
pub fn verify_widgets(image: &AppImage) -> Result<(), ImageError> {
    let tree = WidgetTree::new(*image);
    let count = tree.widget_count();
    let fail = |widget: u16, problem: WidgetProblem| ImageError::BadWidget { widget, problem };
    if count as usize > MOST_WIDGETS {
        return Err(fail(0, WidgetProblem::TooManyWidgets));
    }

    // How many children are still to come at each open level of nesting.
    let mut children_left = [0u8; DEEPEST_NESTING];
    let mut depth = 0;

    for index in 0..count {
        let widget = tree.widget(index).ok_or(fail(index, WidgetProblem::Unreadable))?;
        let is_root = depth == 0;
        check_properties(image, &widget, is_root).map_err(|problem| fail(index, problem))?;

        if depth > 0 {
            children_left[depth - 1] -= 1;
        }
        if widget.child_count > 0 {
            if !widget.kind.holds_children() {
                return Err(fail(index, WidgetProblem::ChildrenNotAllowed));
            }
            if depth == DEEPEST_NESTING {
                return Err(fail(index, WidgetProblem::TooDeep));
            }
            children_left[depth] = widget.child_count;
            depth += 1;
        }
        while depth > 0 && children_left[depth - 1] == 0 {
            depth -= 1;
        }
    }
    if depth != 0 {
        return Err(fail(count, WidgetProblem::Unreadable));
    }
    Ok(())
}

fn check_properties(image: &AppImage, widget: &Widget, is_root: bool) -> Result<(), WidgetProblem> {
    let mut seen = 0u128;
    for entry in widget.properties() {
        let entry = entry.ok_or(WidgetProblem::UnknownProperty)?;
        let property = entry.property;
        let bit = 1u128 << (property as u8);
        if seen & bit != 0 {
            return Err(WidgetProblem::RepeatedProperty);
        }
        seen |= bit;

        if !property.belongs_to(widget.kind) {
            return Err(WidgetProblem::PropertyNotOnWidget);
        }
        if property.is_screen_only() && !is_root {
            return Err(WidgetProblem::ScreenPropertyBelowRoot);
        }

        let is_event = property.value() == PropertyValue::Event;
        match entry.source {
            PropertySource::Handler(_) if !is_event => return Err(WidgetProblem::HandlerOnNonEvent),
            PropertySource::Handler(handler) if handler >= image.handler_count() => {
                return Err(WidgetProblem::HandlerOutOfRange)
            }
            PropertySource::Handler(_) => {}
            _ if is_event => return Err(WidgetProblem::EventWithoutHandler),
            PropertySource::Expression(_) if property.value() == PropertyValue::Array => {
                return Err(WidgetProblem::ArrayNotConstant)
            }
            PropertySource::Expression(expression) if expression >= image.expression_count() => {
                return Err(WidgetProblem::ExpressionOutOfRange)
            }
            PropertySource::Expression(_) => {}
            PropertySource::Constant(value) => {
                if !constant_fits(image, property.value(), value) {
                    return Err(WidgetProblem::BadConstant);
                }
            }
        }
    }
    Ok(())
}

fn constant_fits(image: &AppImage, value_kind: PropertyValue, value: i32) -> bool {
    let as_string = || u16::try_from(value).ok().and_then(|index| image.string(index));
    match value_kind {
        PropertyValue::Number | PropertyValue::Event => true,
        PropertyValue::Bool => value == 0 || value == 1,
        PropertyValue::Color => ColorValue::from_value(value as i64).is_some(),
        PropertyValue::Text => as_string().is_some(),
        PropertyValue::Image => match ImageReference::from_value(value) {
            Some(ImageReference::App(index)) => index < image.resource_count(),
            Some(ImageReference::SharedLibrary(_) | ImageReference::InstalledApp(_)) => true,
            None => false,
        },
        PropertyValue::Array => u16::try_from(value)
            .ok()
            .and_then(|variable| image.state_variable(variable))
            .is_some_and(|variable| variable.is_array()),
    }
}
