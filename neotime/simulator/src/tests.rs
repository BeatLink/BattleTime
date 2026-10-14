use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle},
};
use neotime_display::{Display, SCREEN_HEIGHT};

use super::{slide_once, WatchDisplay};
use crate::{
    hardware::{ThreadDelay, UnconnectedPin},
    pages::{draw_page, SlideDirection, SLIDE_STEP_ROWS},
    panel::SimulatedPanel,
};

// ============================================================================
// Helpers
// ============================================================================

fn started_display() -> (SimulatedPanel, WatchDisplay) {
    let panel = SimulatedPanel::new();
    let display = Display::new(panel.clone(), UnconnectedPin, &mut ThreadDelay).unwrap();
    (panel, display)
}

fn fill(display: &mut WatchDisplay, area: Rectangle, colour: Rgb565) {
    area.into_styled(PrimitiveStyle::with_fill(colour))
        .draw(display)
        .unwrap();
}

fn shown_frame(panel: &SimulatedPanel) -> Vec<Rgb565> {
    let mut frame = Vec::new();
    panel.for_each_shown_pixel(|_column, _screen_row, colour| frame.push(colour));
    frame
}

// ============================================================================
// Tests
// ============================================================================

#[test]
fn drawn_colours_show_as_drawn() {
    let (panel, mut display) = started_display();

    fill(
        &mut display,
        Rectangle::new(Point::new(10, 20), Size::new(5, 5)),
        Rgb565::RED,
    );

    assert_eq!(panel.shown_pixel(12, 22), Rgb565::RED);
    assert_eq!(panel.shown_pixel(9, 22), Rgb565::BLACK);
}

#[test]
fn drawing_after_a_scroll_lands_at_the_screen_position() {
    let (panel, mut display) = started_display();
    fill(
        &mut display,
        Rectangle::new(Point::zero(), Size::new(240, 240)),
        Rgb565::BLACK,
    );

    display.scroll(300).unwrap();
    fill(
        &mut display,
        Rectangle::new(Point::new(0, 10), Size::new(240, 40)),
        Rgb565::GREEN,
    );

    assert_eq!(panel.shown_pixel(0, 9), Rgb565::BLACK);
    assert_eq!(panel.shown_pixel(0, 10), Rgb565::GREEN);
    assert_eq!(panel.shown_pixel(0, 49), Rgb565::GREEN);
    assert_eq!(panel.shown_pixel(0, 50), Rgb565::BLACK);
}

#[test]
fn sliding_up_shows_the_next_page_as_if_drawn_directly() {
    let (panel, mut display) = started_display();
    draw_page(&mut display, 0).unwrap();

    let mut revealed_rows = 0;
    while revealed_rows < SCREEN_HEIGHT {
        revealed_rows += SLIDE_STEP_ROWS;
        slide_once(&mut display, SlideDirection::Up, 1, revealed_rows);
    }

    let (expected_panel, mut expected_display) = started_display();
    draw_page(&mut expected_display, 1).unwrap();
    assert!(shown_frame(&panel) == shown_frame(&expected_panel));
}

#[test]
fn sliding_down_shows_the_previous_page_as_if_drawn_directly() {
    let (panel, mut display) = started_display();
    draw_page(&mut display, 1).unwrap();

    let mut revealed_rows = 0;
    while revealed_rows < SCREEN_HEIGHT {
        revealed_rows += SLIDE_STEP_ROWS;
        slide_once(&mut display, SlideDirection::Down, 0, revealed_rows);
    }

    let (expected_panel, mut expected_display) = started_display();
    draw_page(&mut expected_display, 0).unwrap();
    assert!(shown_frame(&panel) == shown_frame(&expected_panel));
}

#[test]
fn the_demo_app_draws_through_the_display_driver() {
    use neotime_app_host::{demo::stopwatch_image, AppHost, Touch};

    let (panel, mut display) = started_display();
    let image = stopwatch_image();
    let mut system = crate::app::test_system();
    let now = 1_000_000;
    let mut host = AppHost::open(&image, neotime_runtime::DEFAULT_THEME, now, &mut system).unwrap();

    host.touch(Touch::Click { x: 120, y: 138 }, now, &mut system);
    host.draw(&mut display).unwrap();

    assert_eq!(host.widget_text(2), Some("Stop"));
    let accent = Rgb565::new(0, 170 >> 2, 160 >> 3);
    assert_eq!(panel.shown_pixel(30, 180), accent);
    assert_eq!(panel.shown_pixel(2, 2), Rgb565::BLACK);
}

#[test]
fn mouse_gestures_become_touches() {
    use crate::app::TouchTracker;
    use neotime_app_host::Touch;
    use neotime_runtime::events::SwipeDirection;

    let mut tracker = TouchTracker::default();
    tracker.press(Point::new(100, 100));
    let click = tracker.release(Point::new(101, 100));
    tracker.press(Point::new(100, 100));
    let swipe = tracker.release(Point::new(190, 110));

    assert_eq!(click, Some(Touch::Click { x: 101, y: 100 }));
    assert_eq!(swipe, Some(Touch::Swipe(SwipeDirection::Right)));
}

#[test]
fn effects_print_with_their_names() {
    use crate::app::named_argument;
    use neotime_runtime::EffectKind;

    assert_eq!(named_argument(EffectKind::VibratePattern, 3), Some("alarm"));
    assert_eq!(named_argument(EffectKind::MusicControl, 5), Some("volume_down"));
    assert_eq!(named_argument(EffectKind::CallControl, 2), Some("mute"));
    assert_eq!(named_argument(EffectKind::Vibrate, 0), None);
}

#[test]
fn the_simulator_gives_its_own_version_and_start() {
    use neotime_runtime::{SystemValue, SystemValues};

    let system = crate::app::test_system();

    assert!(system
        .read_text(SystemValue::FirmwareVersion)
        .ends_with("+simulator"));
    assert_eq!(system.read_text(SystemValue::ResetReason), "power on");
    assert_eq!(system.read(SystemValue::StartedAt), 0);
    assert!(system.read(SystemValue::FreeStorageKilobytes) > 0);
    assert_eq!(system.read(SystemValue::TaskCount), 1);
    assert_eq!(system.read_element_text(SystemValue::TaskName, 0), "simulator");
    assert_eq!(system.read_element_text(SystemValue::TaskName, 1), "");
    assert_eq!(system.read(SystemValue::TouchChipId), 0xB4);
    assert_eq!(system.read(SystemValue::FlashId), 0);
    assert_eq!(system.read(SystemValue::BootloaderCrc), neotime_runtime::NONE);
}

#[test]
fn low_power_mode_shows_eight_colours_and_keeps_the_rest() {
    let (panel, mut display) = started_display();
    let area = Rectangle::new(Point::new(10, 20), Size::new(5, 5));
    let dark_orange = Rgb565::new(20, 20, 4);
    fill(&mut display, area, dark_orange);

    display.enter_low_power_mode().unwrap();
    let shown_in_low_power = panel.shown_pixel(12, 22);
    display.leave_low_power_mode().unwrap();

    assert_eq!(shown_in_low_power, Rgb565::new(Rgb565::MAX_R, 0, 0));
    assert_eq!(panel.shown_pixel(12, 22), dark_orange);
}

// ============================================================================
// The bootloader's status screen, sending bytes over the panel's SPI wires
// ============================================================================

mod status_screen {
    use embedded_graphics::{
        mono_font::ascii::FONT_10X20,
        pixelcolor::{raw::RawU16, Rgb565},
        prelude::RgbColor,
    };
    use neotime_status_screen::{
        page::{self, LINE_CHARACTERS},
        Screen,
    };

    use crate::{
        hardware::{ThreadDelay, UnconnectedPin},
        panel::{DataOrCommandPin, PanelSpi, SimulatedPanel},
    };

    const BLUE: u16 = 0x00EB;
    const LEFT_MARGIN: u16 = 12;
    const CHARACTER_WIDTH: u16 = 10;

    fn started_screen() -> (SimulatedPanel, Screen<PanelSpi, DataOrCommandPin>) {
        let panel = SimulatedPanel::new();
        let (spi, data_or_command) = panel.spi_wires();
        let screen = Screen::start(
            spi,
            data_or_command,
            UnconnectedPin,
            Rgb565::from(RawU16::new(BLUE)),
            &mut ThreadDelay,
        )
        .unwrap();
        (panel, screen)
    }

    /// The screen rows a line's text covers: the 20-pixel font cell at the page's baselines.
    fn line_rows(line: u16) -> core::ops::Range<u16> {
        let baseline = 30 + line * 24;
        let top = baseline - FONT_10X20.baseline as u16;
        top..top + 20
    }

    fn count(
        panel: &SimulatedPanel,
        columns: core::ops::Range<u16>,
        rows: core::ops::Range<u16>,
        colour: Rgb565,
    ) -> usize {
        let mut found = 0;
        panel.for_each_shown_pixel(|column, row, shown| {
            if columns.contains(&column) && rows.contains(&row) && shown == colour {
                found += 1;
            }
        });
        found
    }

    #[test]
    fn it_wakes_the_panel_and_shows_the_background_everywhere() {
        let (panel, _) = started_screen();
        let blue = Rgb565::from(RawU16::new(BLUE));
        assert_eq!(count(&panel, 0..240, 0..240, blue), 240 * 240);
    }

    #[test]
    fn a_line_shows_white_text_inside_its_rows_only() {
        let (panel, mut screen) = started_screen();
        screen.write_line(page::ACTION, "Installing");
        let rows = line_rows(page::ACTION as u16);
        assert!(count(&panel, 0..240, rows.clone(), Rgb565::WHITE) > 100);
        assert_eq!(count(&panel, 0..240, 0..rows.start, Rgb565::WHITE), 0);
        assert_eq!(count(&panel, 0..240, rows.end..240, Rgb565::WHITE), 0);
    }

    #[test]
    fn a_shorter_line_covers_what_the_longer_one_said() {
        let (panel, mut screen) = started_screen();
        screen.write_line(page::TITLE, "Installing firmware");
        screen.write_line(page::TITLE, "Done");
        let right_of_done = LEFT_MARGIN + 4 * CHARACTER_WIDTH;
        assert_eq!(count(&panel, right_of_done..240, line_rows(0), Rgb565::WHITE), 0);
    }

    #[test]
    fn text_past_the_line_is_cut_at_the_edge() {
        let (panel, mut screen) = started_screen();
        screen.write_line(2 + page::ACTION, "A line far longer than twenty-two characters");
        let end_of_line = LEFT_MARGIN + LINE_CHARACTERS as u16 * CHARACTER_WIDTH;
        let rows = line_rows(2 + page::ACTION as u16);
        assert!(count(&panel, LEFT_MARGIN..end_of_line, rows.clone(), Rgb565::WHITE) > 100);
        assert_eq!(count(&panel, end_of_line..240, rows, Rgb565::WHITE), 0);
    }

    #[test]
    fn text_that_is_not_ascii_does_not_panic() {
        let (_, mut screen) = started_screen();
        screen.write_line(page::FIRMWARE, "é€ ünïcödé line that runs on and on");
    }
}

#[test]
fn watch_files_are_read_from_the_storage_folder() {
    use crate::app::read_storage_file;
    use neotime_runtime::SystemValues;

    let folder = std::env::temp_dir().join(format!("neotime-simulator-storage-{}", std::process::id()));
    std::fs::create_dir_all(folder.join("remote")).unwrap();
    std::fs::write(folder.join("remote/buttons"), [1, 3, 26, 32, 28]).unwrap();
    let mut out = [0u8; 4];

    assert_eq!(
        read_storage_file(&folder, "/remote/buttons", 1, &mut out),
        Some(4)
    );
    assert_eq!(out, [3, 26, 32, 28]);
    assert_eq!(
        read_storage_file(&folder, "/remote/buttons", 3, &mut out),
        Some(2)
    );
    assert_eq!(read_storage_file(&folder, "/remote/missing", 0, &mut out), None);
    assert_eq!(
        read_storage_file(&folder, "/remote/../remote/buttons", 0, &mut out),
        None
    );
    assert_eq!(read_storage_file(&folder, "remote/buttons", 0, &mut out), None);
    assert_eq!(
        crate::app::test_system().read_file("/remote/buttons", 0, &mut out),
        None
    );
    std::fs::remove_dir_all(&folder).unwrap();
}

#[test]
fn a_simulated_reading_reaches_the_app_and_its_values() {
    use crate::{
        hardware::SimulatedHeartRateChip,
        heart_rate::{SimulatedSensor, Update},
    };
    use neotime_app_host::{demo::stopwatch_image, AppHost};
    use neotime_heart_rate::monitor::{Reading, Status};
    use neotime_runtime::{SystemValue, SystemValues};

    let (_sensor, link, updates) = SimulatedSensor::new(SimulatedHeartRateChip::new(72.0));
    let mut system = crate::app::test_system_with_heart_rate(link);
    let image = stopwatch_image();
    let now = 1_000_000;
    let mut host = AppHost::open(&image, neotime_runtime::DEFAULT_THEME, now, &mut system).unwrap();
    assert_eq!(system.read(SystemValue::IsHeartRateRunning), 0);

    updates.send(Update::Status(Status::Settling)).unwrap();
    updates
        .send(Update::Reading(Reading {
            beats_per_minute: 75,
            confidence_percent: 100,
        }))
        .unwrap();
    system.take_heart_rate_updates(&mut host, now);

    assert_eq!(system.read(SystemValue::HeartRate), 75);
    assert_eq!(system.read(SystemValue::HeartRateMeasuredAt), now);
    assert_eq!(
        system.read(SystemValue::HeartRateStatus),
        Status::Measuring as i64
    );
    assert_eq!(system.read(SystemValue::IsHeartRateRunning), 1);
}
