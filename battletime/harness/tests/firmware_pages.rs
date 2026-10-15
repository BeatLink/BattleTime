//! Draws the pages the firmware shows itself after a crash and when an app needs more memory than
//! is free, and compares each with its reference picture in `harness/snapshots/screens/`.

use battletime_app_testing::{check_screen_snapshot, Framebuffer};
use battletime_crash_record::{
    screen::{draw, Page},
    Crash,
};

/// Draws the page on a fresh screen and checks it against `firmware-<name>.png`.
fn check_page(name: &str, page: &Page) -> Result<(), String> {
    let mut frame = Framebuffer::new();
    draw(&mut frame, page).unwrap();
    assert_eq!(frame.pixels_drawn_off_screen(), 0, "{name} drew off the screen");
    check_screen_snapshot(&format!("firmware-{name}"), &frame)
}

#[test]
fn every_firmware_page_matches_its_reference_picture() {
    let out_of_memory = Crash::OutOfMemory {
        requested_bytes: 2048,
        free_bytes: 612,
        return_addresses: [0; 6],
    };
    let stack_guard = Crash::HardFault {
        program_counter: 0x1_A2B4,
        link_register: 0x1_0F03,
        fault_status: 0x12,
        fault_address: 0x2000_D000,
    };
    let pages = [
        (
            "crash",
            Page::Crash {
                crash: &out_of_memory,
                app_name: "Alarm",
            },
        ),
        (
            "crash-without-an-app",
            Page::Crash {
                crash: &stack_guard,
                app_name: "",
            },
        ),
        ("crash-waiting-for-companion", Page::WaitingForCompanion),
        (
            "not-enough-memory",
            Page::NotEnoughMemory {
                app_name: "Notes",
                needed_bytes: 14_000,
                free_bytes: 9_500,
            },
        ),
    ];

    let report: Vec<String> = pages
        .iter()
        .filter_map(|(name, page)| check_page(name, page).err())
        .collect();

    assert!(
        report.is_empty(),
        "\n{}\nRun with BATTLETIME_UPDATE_SNAPSHOTS=1 to accept the change.",
        report.join("\n")
    );
}
