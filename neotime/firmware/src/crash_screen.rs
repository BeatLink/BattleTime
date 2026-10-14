//! The pages the shell shows in place of an app when something went wrong, drawn by `neotime_crash_record::screen`.
//!
//! After a crash the shell starts on the crash page rather than the face. Restart forgets the
//! crash and opens the face; Send debug info waits for the companion app to save the crash, then
//! opens the face. Before an app opens, its memory need is checked against the free heap, and an
//! app that would not fit gets the not-enough-memory page instead of running the heap dry.

use embedded_graphics::prelude::Point;
use neotime_crash_record::screen::{self, Choice, Page};
use neotime_runtime::{estimate_memory_need, AppImage};

use crate::{apps::InstalledApps, restart, ScreenDisplay};

/// Room for an app's name on the not-enough-memory page.
const APP_NAME_BYTES: usize = 24;

// ============================================================================
// Pages
// ============================================================================

#[derive(Clone, PartialEq, Eq)]
pub enum FirmwarePage {
    Crash,
    WaitingForCompanion,
    NotEnoughMemory {
        app_name: heapless::String<APP_NAME_BYTES>,
        needed_bytes: u32,
        free_bytes: u32,
        /// Whether the refused one was the face, so leaving the page shows the help screen instead.
        refused_face: bool,
    },
}

/// A page the shell shows, and whether it is on the screen yet.
pub struct ShownPage {
    pub page: FirmwarePage,
    pub drawn: bool,
}

impl ShownPage {
    pub fn new(page: FirmwarePage) -> ShownPage {
        ShownPage { page, drawn: false }
    }

    /// Paints the page once, until something asks for it again.
    pub fn draw_if_needed(&mut self, display: &mut ScreenDisplay, installed: &InstalledApps) {
        if !self.drawn {
            with_page(&self.page, installed, |page| {
                let _ = screen::draw(display, page);
            });
            self.drawn = true;
        }
    }

    pub fn choice_at(&self, x: i32, y: i32, installed: &InstalledApps) -> Option<Choice> {
        with_page(&self.page, installed, |page| page.choice_at(Point::new(x, y)))
    }

    pub fn button_press_choice(&self, installed: &InstalledApps) -> Choice {
        with_page(&self.page, installed, |page| page.button_press_choice())
    }

    /// Whether the page is about a crash the companion app has since saved, or the wearer dismissed.
    pub fn crash_is_dealt_with(&self) -> bool {
        matches!(self.page, FirmwarePage::Crash | FirmwarePage::WaitingForCompanion)
            && restart::pending_crash().is_none()
    }
}

/// Builds the drawing crate's page, with the open app's name looked up among the installed apps.
fn with_page<Answer>(
    page: &FirmwarePage,
    installed: &InstalledApps,
    use_page: impl FnOnce(&Page) -> Answer,
) -> Answer {
    match page {
        FirmwarePage::Crash => {
            let Some(recorded) = restart::pending_crash() else {
                return use_page(&Page::WaitingForCompanion);
            };
            let id = recorded.open_app_id();
            let app_name = installed
                .packages
                .iter()
                .find(|entry| entry.id == id)
                .map_or(id, |entry| entry.name.as_str());
            use_page(&Page::Crash {
                crash: &recorded.crash,
                app_name,
            })
        }
        FirmwarePage::WaitingForCompanion => use_page(&Page::WaitingForCompanion),
        FirmwarePage::NotEnoughMemory {
            app_name,
            needed_bytes,
            free_bytes,
            ..
        } => use_page(&Page::NotEnoughMemory {
            app_name,
            needed_bytes: *needed_bytes,
            free_bytes: *free_bytes,
        }),
    }
}

// ============================================================================
// The memory check before an app opens
// ============================================================================

/// The not-enough-memory page when the app's need, less its image already read onto the heap, is more than the heap has free.
pub fn refusal_for(image_bytes: &[u8], refused_face: bool) -> Option<FirmwarePage> {
    let image = AppImage::parse(image_bytes).ok()?;
    let need = estimate_memory_need(&image);
    let needed_bytes = need.total_bytes.saturating_sub(need.image_bytes);
    let free_bytes = crate::free_heap_bytes() as u32;
    if needed_bytes <= free_bytes {
        return None;
    }
    let mut name = heapless::String::new();
    let app_name = image.string(image.name_string).unwrap_or("");
    for character in app_name.chars() {
        if name.push(character).is_err() {
            break;
        }
    }
    Some(FirmwarePage::NotEnoughMemory {
        app_name: name,
        needed_bytes,
        free_bytes,
        refused_face,
    })
}
