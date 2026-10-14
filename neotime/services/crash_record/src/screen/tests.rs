extern crate std;

use std::{string::String, vec, vec::Vec};

use super::*;

/// A whole screen in memory.
struct FrameBuffer {
    pixels: Vec<Rgb565>,
}

impl FrameBuffer {
    fn new() -> FrameBuffer {
        FrameBuffer {
            pixels: vec![Rgb565::BLACK; (SCREEN_SIZE * SCREEN_SIZE) as usize],
        }
    }

    fn colour_at(&self, point: Point) -> Rgb565 {
        self.pixels[(point.y * SCREEN_SIZE as i32 + point.x) as usize]
    }

    fn count(&self, area: Rectangle, colour: Rgb565) -> usize {
        area.points()
            .filter(|&point| self.colour_at(point) == colour)
            .count()
    }
}

impl OriginDimensions for FrameBuffer {
    fn size(&self) -> Size {
        Size::new(SCREEN_SIZE, SCREEN_SIZE)
    }
}

impl DrawTarget for FrameBuffer {
    type Color = Rgb565;
    type Error = core::convert::Infallible;

    fn draw_iter<Pixels: IntoIterator<Item = Pixel<Rgb565>>>(
        &mut self,
        pixels: Pixels,
    ) -> Result<(), Self::Error> {
        for Pixel(point, colour) in pixels {
            assert!(
                self.bounding_box().contains(point),
                "drew off the screen at {point:?}"
            );
            self.pixels[(point.y * SCREEN_SIZE as i32 + point.x) as usize] = colour;
        }
        Ok(())
    }
}

const OUT_OF_MEMORY: Crash = Crash::OutOfMemory {
    requested_bytes: 1200,
    free_bytes: 300,
    return_addresses: [0; 6],
};

fn crash_page(app_name: &str) -> Page<'_> {
    Page::Crash {
        crash: &OUT_OF_MEMORY,
        app_name,
    }
}

fn texts(page: &Page) -> Vec<String> {
    lines(page)
        .iter()
        .map(|line| String::from(line.as_str()))
        .collect()
}

fn centre_of(area: Rectangle) -> Point {
    area.center()
}

// ============================================================================
// Words
// ============================================================================

#[test]
fn the_crash_page_says_what_stopped_the_watch_and_which_app_was_open() {
    assert_eq!(
        texts(&crash_page("alarm")),
        ["NeoTime stopped:", "out of memory", "while alarm", "was open."]
    );
}

#[test]
fn without_an_open_app_the_crash_page_names_only_the_cause() {
    assert_eq!(texts(&crash_page("")), ["NeoTime stopped:", "out of memory"]);
}

#[test]
fn the_waiting_page_asks_for_the_companion_app() {
    assert_eq!(
        texts(&Page::WaitingForCompanion),
        [
            "Connect the",
            "companion app and",
            "save a debug report.",
            "Waiting..."
        ]
    );
}

#[test]
fn the_memory_page_gives_the_need_rounded_up_and_the_free_rounded_down() {
    let page = Page::NotEnoughMemory {
        app_name: "Notes",
        needed_bytes: 12 * 1024 + 1,
        free_bytes: 9 * 1024 + 1000,
    };

    assert_eq!(
        texts(&page),
        [
            "Not enough memory",
            "to open Notes.",
            "It needs 13 KB,",
            "9 KB is free."
        ]
    );
}

#[test]
fn every_line_fits_across_the_screen() {
    let pages = [
        crash_page("a-long-app-id-16"),
        Page::WaitingForCompanion,
        Page::NotEnoughMemory {
            app_name: "Notes",
            needed_bytes: 99_999,
            free_bytes: 99_999,
        },
    ];
    for page in pages {
        for line in lines(&page) {
            assert!(
                line.len() <= neotime_status_screen::page::SCREEN_CHARACTERS,
                "{line} is too long"
            );
        }
    }
}

// ============================================================================
// Choices
// ============================================================================

#[test]
fn the_crash_page_offers_restart_above_send_debug_info() {
    let page = crash_page("alarm");

    assert_eq!(
        page.choice_at(centre_of(button_area(0, 2))),
        Some(Choice::Restart)
    );
    assert_eq!(
        page.choice_at(centre_of(button_area(1, 2))),
        Some(Choice::SendDebugInfo)
    );
    assert_eq!(page.choice_at(Point::new(120, 20)), None);
    assert_eq!(page.button_press_choice(), Choice::Restart);
}

#[test]
fn the_waiting_and_memory_pages_are_left_with_their_one_button() {
    let memory_page = Page::NotEnoughMemory {
        app_name: "Notes",
        needed_bytes: 1,
        free_bytes: 0,
    };

    for page in [Page::WaitingForCompanion, memory_page] {
        assert_eq!(page.choice_at(Point::new(120, 210)), Some(Choice::Dismiss));
        assert_eq!(page.choice_at(Point::new(120, 150)), None);
        assert_eq!(page.button_press_choice(), Choice::Dismiss);
    }
}

#[test]
fn buttons_sit_below_the_lines_and_inside_the_screen() {
    let lowest_line_bottom = FIRST_LINE_TOP + (MOST_LINES as i32 - 1) * LINE_HEIGHT + TEXT_HEIGHT as i32;
    let screen = Rectangle::new(Point::zero(), Size::new(SCREEN_SIZE, SCREEN_SIZE));

    for place in 0..2 {
        let area = button_area(place, 2);
        assert!(
            area.top_left.y >= lowest_line_bottom - LINE_HEIGHT,
            "button {place} covers the text"
        );
        assert!(screen.contains(area.bottom_right().unwrap()));
    }
}

// ============================================================================
// Drawing
// ============================================================================

#[test]
fn the_crash_page_paints_its_background_buttons_and_text() {
    let mut screen = FrameBuffer::new();

    draw(&mut screen, &crash_page("alarm")).unwrap();

    assert_eq!(screen.colour_at(Point::new(2, 2)), CRASH_BACKGROUND);
    for place in 0..2 {
        let area = button_area(place, 2);
        assert!(screen.count(area, BUTTON_COLOUR) > 2000);
        assert!(
            screen.count(area, TEXT_COLOUR) > 50,
            "button {place} has no label"
        );
    }
    let first_line = Rectangle::new(Point::new(0, FIRST_LINE_TOP), Size::new(SCREEN_SIZE, TEXT_HEIGHT));
    assert!(screen.count(first_line, TEXT_COLOUR) > 100);
}

#[test]
fn the_memory_page_has_its_own_background() {
    let mut screen = FrameBuffer::new();
    let page = Page::NotEnoughMemory {
        app_name: "Notes",
        needed_bytes: 1,
        free_bytes: 0,
    };

    draw(&mut screen, &page).unwrap();

    assert_eq!(screen.colour_at(Point::new(2, 2)), NOTICE_BACKGROUND);
}
