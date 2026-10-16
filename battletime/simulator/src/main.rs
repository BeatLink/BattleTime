//! A desktop simulator for BattleTime, in the spirit of InfiniTime's InfiniSim.
//!
//! The real drivers run unchanged against simulated hardware: the display driver talks to a model
//! of the ST7789, the vibrator driver to a motor pin and the heart rate driver to a simulated
//! HRS3300. The mouse is the touch panel and the space bar is the side button. With `--app` or
//! `--demo` it runs an app image instead of the demo pages, and with `--script` it runs a test
//! script headless through the test watch. With `--stream` an app runs with no window, taking
//! commands on standard input and writing frames to standard output, for the app builder.

mod app;
mod hardware;
mod heart_rate;
mod pages;
mod panel;
mod stream;
#[cfg(test)]
mod tests;

use std::{
    error::Error,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread,
    time::Duration,
};

use battletime_display::{Display, SCREEN_HEIGHT, SCREEN_WIDTH};
use battletime_vibrator::{OnOffMotor, Step, Vibrator, RING, TAP};
use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle},
};
use embedded_graphics_simulator::{
    sdl2::Keycode, OutputSettings, OutputSettingsBuilder, SimulatorDisplay, SimulatorEvent, Window,
};

use app::{parse_click, Click};
use hardware::{MotorPin, ThreadDelay, UnconnectedPin};
use pages::{draw_page, slide_step, SlideDirection, PAGE_COUNT, SLIDE_STEP_ROWS};
use panel::SimulatedPanel;

// ============================================================================
// Settings
// ============================================================================

const DEFAULT_WINDOW_SCALE: u32 = 2;
const SWIPE_DISTANCE_IN_PIXELS: i32 = 40;
const SLIDE_STEP_PAUSE: Duration = Duration::from_millis(16);
const FRAME_PAUSE: Duration = Duration::from_millis(16);
const MOTOR_BORDER_WIDTH: u32 = 6;

type WatchDisplay = Display<SimulatedPanel, UnconnectedPin>;

/// Where the app to run comes from.
enum AppSource {
    File(String),
    Demo,
}

struct Options {
    screenshot_path: Option<String>,
    first_page: usize,
    window_scale: u32,
    app: Option<AppSource>,
    library_path: Option<String>,
    icons_path: Option<String>,
    installed_paths: Vec<String>,
    storage_folder: Option<String>,
    clicks: Vec<Click>,
    script_path: Option<String>,
    stream: bool,
}

// ============================================================================
// Starting up
// ============================================================================

fn main() -> Result<(), Box<dyn Error>> {
    let options = read_options()?;
    if let Some(path) = &options.script_path {
        run_script(path);
    }
    let panel = SimulatedPanel::new();
    let mut display = Display::new(panel.clone(), UnconnectedPin, &mut ThreadDelay)
        .map_err(|error| format!("the display did not start: {error:?}"))?;
    let output_settings = OutputSettingsBuilder::new().scale(options.window_scale).build();
    let mut screen = SimulatorDisplay::<Rgb565>::new(Size::new(SCREEN_WIDTH as u32, SCREEN_HEIGHT as u32));

    if let Some(source) = &options.app {
        let image = match source {
            AppSource::File(path) => {
                std::fs::read(path).map_err(|error| format!("could not read {path}: {error}"))?
            }
            AppSource::Demo => battletime_app_host::demo::stopwatch_image(),
        };
        let library = match &options.library_path {
            Some(path) => std::fs::read(path).map_err(|error| format!("could not read {path}: {error}"))?,
            None => Vec::new(),
        };
        let icons = read_icons(&options)?.unwrap_or_default();
        let installed: Vec<Vec<u8>> = options
            .installed_paths
            .iter()
            .map(|path| std::fs::read(path).map_err(|error| format!("could not read {path}: {error}")))
            .collect::<Result<_, _>>()?;
        let installed_apps: Vec<&[u8]> = installed.iter().map(Vec::as_slice).collect();
        let installed_app_icons: Vec<&[u8]> = installed_apps
            .iter()
            .map(|bytes| {
                battletime_runtime::AppImage::parse(bytes)
                    .ok()
                    .filter(|image| image.has_icon)
                    .and_then(|image| image.resource_bytes(0))
                    .unwrap_or_default()
            })
            .collect();
        let files = app::AppFiles {
            image: &image,
            library: &library,
            installed_apps: &installed_apps,
            installed_app_icons: &installed_app_icons,
            icons: &icons,
            storage_folder: options.storage_folder.as_deref().map(std::path::Path::new),
        };
        if options.stream {
            let motor_running = Arc::new(AtomicBool::new(false));
            let system = app::SimulatedSystem::new(
                Some(start_vibrator(motor_running.clone())),
                Some(heart_rate::start_heart_rate_sensor()),
                app::wall_clock_in_milliseconds() as u32 | 1,
                &files,
            );
            let commands = stream::read_commands(std::io::BufReader::new(std::io::stdin()));
            return stream::run(
                files,
                panel,
                display,
                system,
                motor_running,
                commands,
                &mut std::io::stdout(),
            );
        }
        return match &options.screenshot_path {
            Some(path) => app::save_screenshot(
                files,
                &panel,
                &mut display,
                &mut screen,
                &output_settings,
                &options.clicks,
                path,
            ),
            None => app::run(files, panel, display, screen, output_settings),
        };
    }

    draw_page(&mut display, options.first_page).unwrap_or_else(|never| match never {});
    if let Some(screenshot_path) = options.screenshot_path {
        copy_panel_to_screen(&panel, &mut screen, false);
        screen
            .to_rgb_output_image(&output_settings)
            .save_png(&screenshot_path)?;
        println!("saved {screenshot_path}");
        return Ok(());
    }

    let motor_running = Arc::new(AtomicBool::new(false));
    let vibrator_requests = start_vibrator(motor_running.clone());
    let mut window = Window::new("BattleTime simulator", &output_settings);
    let mut simulator = Simulator {
        panel,
        display,
        screen,
        window_settings: output_settings,
        motor_running,
        vibrator_requests,
        current_page: options.first_page,
        touch_start: None,
    };

    print_controls();
    simulator.run(&mut window);
    Ok(())
}

/// The icons file `--icons` names, or the one `battletime-compiler library` wrote beside the library.
fn read_icons(options: &Options) -> Result<Option<Vec<u8>>, String> {
    if let Some(path) = &options.icons_path {
        return std::fs::read(path)
            .map(Some)
            .map_err(|error| format!("could not read {path}: {error}"));
    }
    let beside_library = options
        .library_path
        .as_ref()
        .map(|path| std::path::Path::new(path).with_extension("icons"));
    Ok(beside_library.and_then(|path| std::fs::read(path).ok()))
}

fn read_options() -> Result<Options, String> {
    let mut options = Options {
        screenshot_path: None,
        first_page: 0,
        window_scale: DEFAULT_WINDOW_SCALE,
        app: None,
        library_path: None,
        icons_path: None,
        installed_paths: Vec::new(),
        storage_folder: None,
        clicks: Vec::new(),
        script_path: None,
        stream: false,
    };
    let mut arguments = std::env::args().skip(1);

    while let Some(argument) = arguments.next() {
        let mut value = || arguments.next().ok_or(format!("{argument} needs a value"));
        match argument.as_str() {
            "--screenshot" => options.screenshot_path = Some(value()?),
            "--page" => options.first_page = parse_number(&value()?)?.saturating_sub(1).min(PAGE_COUNT - 1),
            "--scale" => options.window_scale = parse_number(&value()?)?.max(1) as u32,
            "--app" => options.app = Some(AppSource::File(value()?)),
            "--demo" => options.app = Some(AppSource::Demo),
            "--library" => options.library_path = Some(value()?),
            "--icons" => options.icons_path = Some(value()?),
            "--installed" => options.installed_paths.push(value()?),
            "--files" => options.storage_folder = Some(value()?),
            "--click" => options.clicks.push(parse_click(&value()?)?),
            "--script" => options.script_path = Some(value()?),
            "--stream" => options.stream = true,
            _ => {
                return Err(format!(
                    "unknown option {argument}; use --screenshot PATH, --page N, --scale N, --app PATH, \
                     --demo, --library PATH, --icons PATH, --installed PATH, --files FOLDER, --click X,Y, --script PATH, --stream"
                ))
            }
        }
    }

    Ok(options)
}

/// Runs a test script through the test watch, printing each step, and exits non-zero if a step fails.
fn run_script(path: &str) -> ! {
    let script = match battletime_app_testing::load_script(std::path::Path::new(path)) {
        Ok(script) => script,
        Err(problem) => {
            eprintln!("{problem}");
            std::process::exit(2);
        }
    };
    let run = battletime_app_testing::run_script(&script, std::path::Path::new("."), |outcome| {
        println!("{outcome}")
    });
    match run.failure {
        None => {
            println!("passed all {} steps of {path}", run.passed_steps.len());
            std::process::exit(0);
        }
        Some(_) => std::process::exit(1),
    }
}

fn parse_number(text: &str) -> Result<usize, String> {
    text.parse().map_err(|_| format!("{text} is not a number"))
}

fn print_controls() {
    println!("BattleTime simulator");
    println!("  drag up or down, or press Up and Down   slide between pages");
    println!("  space                                   press the side button (one tap)");
    println!("  r                                       ring the motor once");
    println!("  s                                       save a screenshot");
    println!("  q or Escape                             quit");
}

/// The vibrator runs on its own thread so a pattern waits without freezing the window.
fn start_vibrator(motor_running: Arc<AtomicBool>) -> mpsc::Sender<&'static [Step]> {
    let (sender, receiver) = mpsc::channel::<&'static [Step]>();

    thread::spawn(move || {
        let motor = OnOffMotor {
            motor_pin: MotorPin { motor_running },
        };
        let mut vibrator = Vibrator::new(motor).unwrap_or_else(|never| match never {});

        let mut delay = ThreadDelay;
        for pattern in receiver {
            let playing = vibrator.play(pattern, &mut delay);
            embassy_futures::block_on(playing).unwrap_or_else(|never| match never {});
        }
    });

    sender
}

// ============================================================================
// Running the window
// ============================================================================

struct Simulator {
    panel: SimulatedPanel,
    display: WatchDisplay,
    screen: SimulatorDisplay<Rgb565>,
    window_settings: OutputSettings,
    motor_running: Arc<AtomicBool>,
    vibrator_requests: mpsc::Sender<&'static [Step]>,
    current_page: usize,
    touch_start: Option<Point>,
}

impl Simulator {
    fn run(&mut self, window: &mut Window) {
        loop {
            self.show(window);

            let events: Vec<SimulatorEvent> = window.events().collect();
            for event in events {
                if !self.handle(event, window) {
                    return;
                }
            }

            thread::sleep(FRAME_PAUSE);
        }
    }

    /// Returns false when the simulator should close.
    fn handle(&mut self, event: SimulatorEvent, window: &mut Window) -> bool {
        match event {
            SimulatorEvent::Quit => return false,
            SimulatorEvent::KeyDown {
                keycode,
                repeat: false,
                ..
            } => match keycode {
                Keycode::Q | Keycode::Escape => return false,
                Keycode::Space => self.vibrate(TAP),
                Keycode::R => self.vibrate(RING),
                Keycode::S => self.save_screenshot(),
                Keycode::Up => self.slide(SlideDirection::Up, window),
                Keycode::Down => self.slide(SlideDirection::Down, window),
                _ => {}
            },
            SimulatorEvent::MouseButtonDown { point, .. } => self.touch_start = Some(point),
            SimulatorEvent::MouseButtonUp { point, .. } => {
                if let Some(touch_start) = self.touch_start.take() {
                    let distance_moved_down = point.y - touch_start.y;
                    if distance_moved_down <= -SWIPE_DISTANCE_IN_PIXELS {
                        self.slide(SlideDirection::Up, window);
                    } else if distance_moved_down >= SWIPE_DISTANCE_IN_PIXELS {
                        self.slide(SlideDirection::Down, window);
                    }
                }
            }
            _ => {}
        }

        true
    }

    fn vibrate(&self, pattern: &'static [Step]) {
        let _ = self.vibrator_requests.send(pattern);
    }

    fn slide(&mut self, direction: SlideDirection, window: &mut Window) {
        let incoming_page = match direction {
            SlideDirection::Up => (self.current_page + 1) % PAGE_COUNT,
            SlideDirection::Down => (self.current_page + PAGE_COUNT - 1) % PAGE_COUNT,
        };

        let mut revealed_rows = 0;
        while revealed_rows < SCREEN_HEIGHT {
            revealed_rows += SLIDE_STEP_ROWS;
            slide_once(&mut self.display, direction, incoming_page, revealed_rows);
            self.show(window);
            thread::sleep(SLIDE_STEP_PAUSE);
        }

        self.current_page = incoming_page;
    }

    fn show(&mut self, window: &mut Window) {
        copy_panel_to_screen(
            &self.panel,
            &mut self.screen,
            self.motor_running.load(Ordering::Relaxed),
        );
        window.update(&self.screen);
    }

    fn save_screenshot(&mut self) {
        copy_panel_to_screen(&self.panel, &mut self.screen, false);
        let seconds_since_epoch = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_secs());
        let path = format!("battletime-{seconds_since_epoch}.png");

        match self
            .screen
            .to_rgb_output_image(&self.window_settings)
            .save_png(&path)
        {
            Ok(()) => println!("saved {path}"),
            Err(error) => eprintln!("could not save {path}: {error}"),
        }
    }
}

/// Scrolls one band and draws the part of the incoming page that has just come on screen.
fn slide_once(
    display: &mut WatchDisplay,
    direction: SlideDirection,
    incoming_page: usize,
    revealed_rows: u16,
) {
    let memory_rows_in_scroll = battletime_display::MEMORY_ROWS;
    let scroll_offset = match direction {
        SlideDirection::Up => display.scroll_offset() + SLIDE_STEP_ROWS,
        SlideDirection::Down => display.scroll_offset() + memory_rows_in_scroll - SLIDE_STEP_ROWS,
    };
    display
        .scroll(scroll_offset)
        .unwrap_or_else(|never| match never {});

    let step = slide_step(direction, revealed_rows);
    let mut band = display.clipped(&step.band);
    let mut incoming = band.translated(Point::new(0, step.page_top_row));
    draw_page(&mut incoming, incoming_page).unwrap_or_else(|never| match never {});
}

/// Copies what the panel shows into the window, with a red border while the motor runs.
fn copy_panel_to_screen(panel: &SimulatedPanel, screen: &mut SimulatorDisplay<Rgb565>, motor_running: bool) {
    panel.for_each_shown_pixel(|column, screen_row, colour| {
        let pixel = Pixel(Point::new(column as i32, screen_row as i32), colour);
        pixel.draw(screen).unwrap_or_else(|never| match never {});
    });

    if motor_running {
        let whole_screen = Rectangle::new(Point::zero(), screen.size());
        let border = PrimitiveStyle::with_stroke(Rgb565::RED, MOTOR_BORDER_WIDTH);
        whole_screen
            .into_styled(border)
            .draw(screen)
            .unwrap_or_else(|never| match never {});
    }
}
