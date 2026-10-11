//! A stopwatch app image built by hand with the runtime's image writer, so the host can be tried
//! before the compiler exists.
//!
//! It is the stopwatch from docs/app-runtime.md: a label bound to the elapsed time as `mm:ss`, a
//! start and stop button, and a reset button shown only while stopped.

use alloc::vec::Vec;

use neotime_runtime::{
    instruction::Instruction,
    widgets::Property,
    writer::{CodeWriter, ImageWriter},
    AppKind, ColorValue, EffectKind, PropertySource, SystemValue, ThemeColor, ValueType, WidgetKind,
};

// ============================================================================
// Numbers in the image
// ============================================================================

/// The widget numbers of the stopwatch's parts.
pub mod stopwatch_widgets {
    pub const SCREEN: u16 = 0;
    pub const ELAPSED_LABEL: u16 = 1;
    pub const START_BUTTON: u16 = 2;
    pub const RESET_BUTTON: u16 = 3;
}

/// The state variable numbers of the stopwatch.
pub mod stopwatch_variables {
    pub const STARTED_AT: u8 = 0;
    pub const STOPPED_AFTER: u8 = 1;
}

const LABEL_FONT_HUGE: i32 = 4;
const ALIGN_CENTER: i32 = 1;
const SCREEN_PADDING: i32 = 12;
const BUTTON_HEIGHT: i32 = 52;
const BUTTON_MARGIN: i32 = 4;
const TAP_IN_MILLISECONDS: i8 = 30;

fn constant(value: i32) -> PropertySource {
    PropertySource::Constant(value)
}

fn theme(color: ThemeColor) -> i32 {
    ColorValue::Theme(color).to_value() as i32
}

// ============================================================================
// The stopwatch
// ============================================================================

/// The stopwatch app image.
pub fn stopwatch_image() -> Vec<u8> {
    use stopwatch_variables::*;
    use Instruction::*;

    let mut writer = ImageWriter::new("stopwatch", "Stopwatch", AppKind::App, 1);
    let started_at = writer.variable("started_at", ValueType::Instant, true, neotime_runtime::NONE);
    let stopped_after = writer.variable("stopped_after", ValueType::Duration, true, 0);
    debug_assert_eq!((started_at, stopped_after), (STARTED_AT, STOPPED_AFTER));
    let pattern = writer.string("mm:ss");
    let start = writer.string("Start");
    let stop = writer.string("Stop");
    let reset = writer.string("Reset");
    let main = writer.string("main");

    // The elapsed time: `now - started_at` while running, else the time it stopped after.
    let mut code = CodeWriter::new();
    let running = code.label();
    let format = code.label();
    code.emit(ReadState(STARTED_AT))
        .emit(IsNone)
        .jump_if_false(running);
    code.emit(ReadState(STOPPED_AFTER)).jump(format);
    code.place(running)
        .emit(ReadSystem(SystemValue::Now))
        .emit(ReadState(STARTED_AT))
        .emit(Subtract);
    code.place(format).emit(FormatDuration(pattern));
    let elapsed = writer.expression(code.finish().expect("the elapsed time assembles"));

    // The start button's text.
    let mut code = CodeWriter::new();
    let running = code.label();
    let done = code.label();
    code.emit(ReadState(STARTED_AT))
        .emit(IsNone)
        .jump_if_false(running);
    code.emit(PushText(start)).jump(done);
    code.place(running).emit(PushText(stop));
    code.place(done);
    let start_text = writer.expression(code.finish().expect("the button text assembles"));

    // Reset shows only while stopped.
    let mut code = CodeWriter::new();
    code.emit(ReadState(STARTED_AT)).emit(IsNone);
    let is_stopped = writer.expression(code.finish().expect("the reset check assembles"));

    // The time is drawn in the accent colour while running.
    let mut code = CodeWriter::new();
    let running = code.label();
    let done = code.label();
    code.emit(ReadState(STARTED_AT))
        .emit(IsNone)
        .jump_if_false(running);
    code.emit(PushInt(theme(ThemeColor::Text))).jump(done);
    code.place(running).emit(PushInt(theme(ThemeColor::Accent)));
    code.place(done);
    let time_color = writer.expression(code.finish().expect("the time colour assembles"));

    // Start or stop, with a short tap of the motor.
    let mut code = CodeWriter::new();
    let stop_running = code.label();
    let done = code.label();
    code.emit(ReadState(STARTED_AT))
        .emit(IsNone)
        .jump_if_false(stop_running);
    code.emit(ReadSystem(SystemValue::Now))
        .emit(ReadState(STOPPED_AFTER))
        .emit(Subtract)
        .emit(WriteState(STARTED_AT))
        .jump(done);
    code.place(stop_running)
        .emit(ReadSystem(SystemValue::Now))
        .emit(ReadState(STARTED_AT))
        .emit(Subtract)
        .emit(WriteState(STOPPED_AFTER))
        .emit(PushNone)
        .emit(WriteState(STARTED_AT));
    code.place(done)
        .emit(PushSmall(TAP_IN_MILLISECONDS))
        .emit(Effect(EffectKind::Vibrate));
    let toggle = writer.handler(code.finish().expect("the toggle handler assembles"));

    let mut code = CodeWriter::new();
    code.emit(PushSmall(0)).emit(WriteState(STOPPED_AFTER));
    let reset_handler = writer.handler(code.finish().expect("the reset handler assembles"));

    writer.widget(
        WidgetKind::Column,
        3,
        &[
            (Property::ScreenName, constant(main as i32)),
            (Property::Padding, constant(SCREEN_PADDING)),
        ],
    );
    writer.widget(
        WidgetKind::Label,
        0,
        &[
            (Property::Text, PropertySource::Expression(elapsed)),
            (Property::Color, PropertySource::Expression(time_color)),
            (Property::Font, constant(LABEL_FONT_HUGE)),
            (Property::Align, constant(ALIGN_CENTER)),
            (Property::Grow, constant(1)),
        ],
    );
    writer.widget(
        WidgetKind::Button,
        0,
        &[
            (Property::Text, PropertySource::Expression(start_text)),
            (Property::Background, constant(theme(ThemeColor::Accent))),
            (Property::Color, constant(theme(ThemeColor::Background))),
            (Property::Height, constant(BUTTON_HEIGHT)),
            (Property::Margin, constant(BUTTON_MARGIN)),
            (Property::OnClick, PropertySource::Handler(toggle)),
        ],
    );
    writer.widget(
        WidgetKind::Button,
        0,
        &[
            (Property::Text, constant(reset as i32)),
            (Property::Visible, PropertySource::Expression(is_stopped)),
            (Property::Height, constant(BUTTON_HEIGHT)),
            (Property::Margin, constant(BUTTON_MARGIN)),
            (Property::OnClick, PropertySource::Handler(reset_handler)),
        ],
    );

    writer.build()
}
