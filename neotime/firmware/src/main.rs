//! NeoTime's firmware: the first one, which proves the chain from bootloader to Bluetooth.
//!
//! It takes over the bootloader's watchdog and feeds it only while the side button is up, shows a
//! page on the screen, mounts the file system, advertises as "NeoTime", takes firmware updates over
//! legacy DFU, and confirms itself to the bootloader once a phone has exchanged data with it.
//! After start-up the shell takes the screen and runs the watch face, the launcher and the apps the
//! companion app installed in `/apps`; a phone sets the clock through the Current Time service.
//!
//! The `recovery` feature builds the small recovery firmware: the status page, Bluetooth and
//! updates, without the shell.

#![no_std]
#![no_main]

extern crate alloc;

use core::cell::RefCell;

use display_dma::DmaInterface;
use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_executor::Spawner;
use embassy_futures::select::{select, select3, select4, Either, Either3, Either4};
use embassy_nrf::{
    bind_interrupts,
    config::LfclkSource,
    gpio::{Input, Level, Output, OutputDrive, Pull},
    gpiote::{OutputChannel, OutputChannelPolarity},
    peripherals,
    spim::{self, Spim},
    twim,
    wdt::{self, SleepConfig, Watchdog, WatchdogHandle},
};
use embassy_sync::blocking_mutex::{raw::NoopRawMutex, Mutex};
use embassy_time::{Delay, Duration, Instant, Timer};
use embedded_alloc::LlffHeap;
use embedded_graphics::{pixelcolor::Rgb565, prelude::*};
use neotime_blefs::service::FileTransferService;
use neotime_boot::{FIRMWARE_ALIVE_MAGIC, FIRMWARE_ALIVE_MARKER, WATCHDOG_TIMEOUT_IN_TICKS};
use neotime_button::Button;
use neotime_companion::link_pace::LinkPace;
use neotime_companion::LineAssembler;
#[cfg(with_shell)]
use neotime_companion::MOST_NEW_ALERT_BYTES;
use neotime_crash_record::Crash;
use neotime_dfu::{
    service::DfuWrite, service::LegacyDfuService, write_progress, Activity, LegacyDfu, Outcome,
};
use neotime_display::Display;
use neotime_settings::Setting;
use neotime_updates::ImageKind;
use nrf_pac as pac;
use static_cell::StaticCell;
use trouble_host::prelude::*;

#[path = "storage/watch.rs"]
mod storage;
mod storage_requests;

#[cfg_attr(not(with_shell), allow(dead_code))]
mod about;
// The recovery firmware runs without the shell, which leaves these unused there.
#[cfg_attr(not(with_shell), allow(dead_code))]
mod app_events;
#[cfg_attr(not(with_shell), allow(dead_code))]
mod apps;
#[cfg_attr(not(with_shell), allow(dead_code))]
mod backlight;
#[cfg_attr(not(with_shell), allow(dead_code))]
mod battery;
mod clock;
#[cfg_attr(not(with_shell), allow(dead_code))]
mod crash_screen;
mod diagnostics;
mod display_dma;
mod drawing_screen;
#[cfg_attr(not(with_shell), allow(dead_code))]
mod foreground;
#[cfg_attr(not(with_shell), allow(dead_code))]
mod heart_rate;
#[cfg_attr(not(with_shell), allow(dead_code))]
mod help;
#[cfg_attr(not(with_shell), allow(dead_code))]
mod i2c_bus;
mod infinitime_services;
mod restart;
#[cfg_attr(not(with_shell), allow(dead_code))]
mod settings;
#[cfg_attr(not(with_shell), allow(dead_code))]
mod shell;
mod stack_guard;
// Under wasp-os the firmware is never rolled back or installed as recovery.
#[cfg_attr(not(with_shell), allow(dead_code))]
mod motion;
mod packages;
#[cfg_attr(not(with_shell), allow(dead_code))]
mod phone;
#[cfg(with_shell)]
mod phone_services;
mod status;
#[cfg_attr(not(with_shell), allow(dead_code))]
mod system_values;
mod tasks;
#[cfg_attr(not(with_shell), allow(dead_code))]
mod touch;
#[cfg_attr(not(with_shell), allow(dead_code))]
mod vibration;

// ============================================================================
// PineTime wiring
// ============================================================================

bind_interrupts!(struct Interrupts {
    TWISPI0 => spim::InterruptHandler<peripherals::TWISPI0>;
    TWISPI1 => twim::InterruptHandler<peripherals::TWISPI1>;
    SAADC => embassy_nrf::saadc::InterruptHandler;
    RADIO => neotime_bluetooth::RadioInterruptHandler;
    TIMER1 => display_dma::StripSentHandler;
});

/// The package version and the git commit, such as `0.1.0+1b19aed`, set by `build.rs`.
const FIRMWARE_VERSION: &str = env!("NEOTIME_FIRMWARE_VERSION");
/// The UTC day `build.rs` ran, such as `2026-10-13`.
#[cfg_attr(not(with_shell), allow(dead_code))]
const FIRMWARE_BUILD_DATE: &str = env!("NEOTIME_BUILD_DATE");
const DEVICE_NAME: &str = "NeoTime";
/// The shell's share of the heap, which the open app's widgets and text are allocated from.
#[cfg(with_shell)]
const UI_HEAP_SIZE: usize = 20 * 1024 + 256;
#[cfg(not(with_shell))]
const UI_HEAP_SIZE: usize = 0;
const HEAP_SIZE: usize = storage::HEAP_SIZE + UI_HEAP_SIZE;
/// How often the watchdog task reads the button, and feeds the watchdog when it is up.
const WATCHDOG_CHECK_INTERVAL: Duration = Duration::from_millis(500);
/// The side button's pin on port 0, which reads high while the button is held.
const SIDE_BUTTON_PIN: usize = 13;

use display_dma::SpiBus;
#[cfg_attr(not(with_shell), allow(dead_code))]
type I2cBus = embassy_sync::mutex::Mutex<NoopRawMutex, i2c_bus::SleepingTwim>;
/// One user of the I2C bus the touch panel, the accelerometer and the heart rate sensor share.
type I2cDeviceOnBus = I2cDevice<'static, NoopRawMutex, i2c_bus::SleepingTwim>;

#[global_allocator]
static HEAP: WatchHeap = WatchHeap {
    heap: LlffHeap::empty(),
};

/// The heap, which tells `restart` about each allocation, so a panic after a refused one is recorded as running out of memory.
struct WatchHeap {
    heap: LlffHeap,
}

unsafe impl core::alloc::GlobalAlloc for WatchHeap {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        let pointer = unsafe { self.heap.alloc(layout) };
        restart::note_allocation(layout.size(), pointer.is_null(), || self.heap.free());
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: core::alloc::Layout) {
        unsafe { self.heap.dealloc(pointer, layout) }
    }
}

/// The heap's free bytes now, in all its free blocks together.
#[cfg_attr(not(with_shell), allow(dead_code))]
fn free_heap_bytes() -> usize {
    HEAP.heap.free()
}

static SPI_BUS: StaticCell<SpiBus> = StaticCell::new();
/// One connection and no L2CAP channels of our own, since every service speaks over GATT.
static HOST_RESOURCES: StaticCell<HostResources<DefaultPacketPool, 1, 0>> = StaticCell::new();

// ============================================================================
// Start
// ============================================================================

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    stack_guard::start();
    crate::tasks::name_this_task("main");
    let mut config = embassy_nrf::config::Config::default();
    config.lfclk_source = LfclkSource::ExternalXtal;
    // The PineTime board carries the DCDC inductor, and the regulator draws less than the LDO while the radio runs.
    config.dcdc.reg1 = true;
    // Leave the debug port's settings alone, so start-up never writes the protection word.
    config.debug = embassy_nrf::config::Debug::NotConfigured;
    let chip = embassy_nrf::init(config);
    // The chip starts with its flash cache off, so every instruction fetched from flash waits.
    // Profiling only counts hits and misses, for the diagnostics.
    pac::NVMC.icachecnf().write(|w| {
        w.set_cacheen(true);
        w.set_cacheprofen(true);
    });

    start_heap();
    mark_alive();
    let watchdog = take_over_watchdog(chip.WDT);

    let button_pin = Input::new(chip.P0_13, Pull::Down);
    let power_pin = Output::new(chip.P0_15, Level::Low, OutputDrive::Standard);
    let Ok(button) = Button::new(button_pin, power_pin);
    spawner.spawn(feed_watchdog_while_button_is_up(watchdog).unwrap());
    spawner.spawn(send_button_gestures(button).unwrap());

    let mut spi_config = spim::Config::default();
    spi_config.frequency = spim::Frequency::M8;
    let spi_bus = SPI_BUS.init(Mutex::new(RefCell::new(display_dma::SharedBus::new(
        Spim::new(
            chip.TWISPI0,
            Interrupts,
            chip.P0_02,
            chip.P0_04,
            chip.P0_03,
            spi_config,
        ),
        Output::new(chip.P0_25, Level::High, OutputDrive::Standard),
        display_dma::ChainPeripherals {
            counter: chip.TIMER1,
            restart_channel: chip.PPI_CH5,
            count_channel: chip.PPI_CH6,
            stop_channel: chip.PPI_CH7,
            restart_group: chip.PPI_GROUP0,
        },
    ))));

    let mut screen = start_screen(
        spi_bus,
        Output::new(chip.P0_18, Level::Low, OutputDrive::Standard),
        Output::new(chip.P0_26, Level::High, OutputDrive::Standard),
    );
    let backlight = backlight::Backlight::new(
        OutputChannel::new(
            chip.GPIOTE_CH0,
            chip.P0_14,
            Level::High,
            OutputDrive::Standard,
            OutputChannelPolarity::Toggle,
        ),
        Output::new(chip.P0_22, Level::High, OutputDrive::Standard),
        Output::new(chip.P0_23, Level::High, OutputDrive::Standard),
        chip.RTC2,
        chip.PPI_CH1,
        chip.PPI_CH2,
    );
    let mut title = StatusLine::new();
    let _ = core::fmt::write(&mut title, format_args!("NeoTime {FIRMWARE_VERSION}"));
    screen.write_line(line::TITLE, &title);
    screen.write_line(line::FILES, "Starting");
    screen.write_line(line::BLUETOOTH, "Off");
    let last_restart = restart::take_last_restart();
    clock::restore_kept();
    show_last_restart(&mut screen, last_restart);

    let storage_peripherals = storage::StoragePeripherals {
        flash_chip_select: chip.P0_05,
        flash_controller: chip.NVMC,
    };
    let Some((targets, confirmation, mode, files)) =
        storage::start(spi_bus, storage_peripherals, &mut screen).await
    else {
        return;
    };
    screen.set_mode(mode);

    #[cfg(with_shell)]
    {
        // The touch panel, accelerometer and heart rate sensor share one I2C bus; 32 bytes fits the largest write.
        static I2C_BUFFER: StaticCell<[u8; 32]> = StaticCell::new();
        static I2C_BUS: StaticCell<I2cBus> = StaticCell::new();
        let mut i2c_config = twim::Config::default();
        i2c_config.frequency = twim::Frequency::K400;
        let i2c_bus: &'static I2cBus = I2C_BUS.init(embassy_sync::mutex::Mutex::new(
            i2c_bus::SleepingTwim::new(twim::Twim::new(
                chip.TWISPI1,
                Interrupts,
                chip.P0_06,
                chip.P0_07,
                i2c_config,
                I2C_BUFFER.init([0; 32]),
            )),
        ));
        spawner.spawn(
            touch::read_touches(
                I2cDevice::new(i2c_bus),
                Input::new(chip.P0_28, Pull::Up),
                Output::new(chip.P0_10, Level::High, OutputDrive::Standard),
            )
            .unwrap(),
        );
        spawner.spawn(
            battery::watch_battery(
                battery::battery_adc(chip.SAADC, chip.P0_31),
                Input::new(chip.P0_12, Pull::None),
                Input::new(chip.P0_19, Pull::None),
            )
            .unwrap(),
        );
        spawner.spawn(motion::run_accelerometer(I2cDevice::new(i2c_bus)).unwrap());
        spawner.spawn(heart_rate::run_heart_rate_sensor(I2cDevice::new(i2c_bus)).unwrap());
        spawner.spawn(
            vibration::run_vibrator(Output::new(chip.P0_16, Level::High, OutputDrive::Standard)).unwrap(),
        );
        spawner.spawn(shell::run_shell(screen.display, backlight, files).unwrap());
    }
    // Kept for the whole run without a shell to own it: dropping the backlight releases its pins and the screen goes dark.
    #[cfg(not(with_shell))]
    core::mem::forget(backlight);
    #[cfg(not(with_shell))]
    let _unused = files;
    let radio = neotime_bluetooth::RadioPeripherals {
        radio: chip.RADIO,
        timer: chip.TIMER0,
        random_number_generator: chip.RNG,
        start_transmit_channel: chip.PPI_CH20,
        start_receive_channel: chip.PPI_CH21,
        give_up_channel: chip.PPI_CH22,
        address_time_channel: chip.PPI_CH26,
        end_time_channel: chip.PPI_CH27,
        real_time_counter: chip.RTC0,
        start_crystal_channel: chip.PPI_CH3,
        start_timer_channel: chip.PPI_CH4,
    };
    start_bluetooth(
        spawner,
        radio,
        BluetoothParts::new(targets, confirmation, screen),
        files,
    );
}

/// Starts the radio and the tasks that serve Bluetooth and storage.
// Kept out of `main`, so the tasks built here never share its stack frame with what start-up holds.
#[inline(never)]
fn start_bluetooth(
    spawner: Spawner,
    radio: neotime_bluetooth::RadioPeripherals,
    parts: BluetoothParts,
    files: &'static storage::Files,
) {
    let controller = neotime_bluetooth::start(radio, Interrupts, free_default_packets);
    let parts = BLUETOOTH_PARTS.init(parts);
    spawner.spawn(storage::run_storage(files).unwrap());
    spawner.spawn(run_bluetooth(controller, parts, files).unwrap());
}

// ============================================================================
// What happens on a panic or a fault
// ============================================================================

const CONFIGURABLE_FAULT_STATUS: *const u32 = 0xE000_ED28 as *const u32;
const BUS_FAULT_ADDRESS: *const u32 = 0xE000_ED38 as *const u32;

/// Halting lets the watchdog reset the watch, which a reset loop would never do.
#[panic_handler]
fn halt_on_panic(_information: &core::panic::PanicInfo) -> ! {
    // Taken first, so the search starts above the frames that record the panic.
    let stack_pointer = cortex_m::register::msp::read();
    restart::record_panic(stack_pointer);
    loop {
        cortex_m::asm::wfe();
    }
}

#[cortex_m_rt::exception]
unsafe fn HardFault(frame: &cortex_m_rt::ExceptionFrame) -> ! {
    let fault_status = unsafe { CONFIGURABLE_FAULT_STATUS.read_volatile() };
    restart::record_crash(&Crash::HardFault {
        program_counter: frame.pc(),
        link_register: frame.lr(),
        fault_status,
        fault_address: unsafe { BUS_FAULT_ADDRESS.read_volatile() },
    });
    loop {
        cortex_m::asm::wfe();
    }
}

fn start_heap() {
    static mut HEAP_MEMORY: [u8; HEAP_SIZE] = [0; HEAP_SIZE];
    unsafe {
        HEAP.heap
            .init(core::ptr::addr_of_mut!(HEAP_MEMORY) as usize, HEAP_SIZE)
    }
}

/// Lets an emulator see that the firmware reached `main`; the linker keeps these words free.
fn mark_alive() {
    let marker = FIRMWARE_ALIVE_MARKER.start as *mut u32;
    unsafe {
        let boot_count = marker.add(1).read_volatile();
        marker.write_volatile(FIRMWARE_ALIVE_MAGIC);
        marker.add(1).write_volatile(boot_count.wrapping_add(1));
    }
}

// ============================================================================
// The watchdog, fed only while the button is up
// ============================================================================

/// The bootloader started it and it cannot be stopped, so take it over with the same settings.
fn take_over_watchdog(watchdog_peripheral: embassy_nrf::Peri<'static, peripherals::WDT>) -> WatchdogHandle {
    let mut config = wdt::Config::default();
    config.timeout_ticks = WATCHDOG_TIMEOUT_IN_TICKS;
    config.action_during_sleep = SleepConfig::Run;
    config.action_during_debug_halt = storage::WATCHDOG_HALT;

    match Watchdog::try_new(watchdog_peripheral, config) {
        Ok((_watchdog, [handle])) => handle,
        Err(_) => loop {
            cortex_m::asm::wfe();
        },
    }
}

/// Holding the button lets the watchdog reset the watch into the bootloader; see docs/bootloader.md.
/// A shell or a radio that stopped making progress lets it reset the watch too.
///
/// It reads the pin itself rather than through the button task, so it works whatever that task does.
#[embassy_executor::task]
async fn feed_watchdog_while_button_is_up(mut watchdog: WatchdogHandle) {
    crate::tasks::name_this_task("watchdog");
    let mut liveness = neotime_liveness::Liveness::new();
    loop {
        // The radio's events end a silent connection; this ends one whose radio stopped sending them.
        neotime_bluetooth::check_supervision();
        let (radio_events, radio_idle) = neotime_bluetooth::radio_progress();
        let verdict = liveness.check(neotime_liveness::Reading {
            shell_loops: shell_loops(),
            radio_events,
            radio_idle,
        });
        if verdict.ping_shell {
            ping_shell();
        }
        if verdict.alive && !side_button_is_held() {
            watchdog.pet();
        }
        clock::keep();
        Timer::after(WATCHDOG_CHECK_INTERVAL).await;
    }
}

/// The shell's loops so far, or None before its first loop, while the start-up page shows.
#[cfg(with_shell)]
fn shell_loops() -> Option<u32> {
    let loops = diagnostics::get(diagnostics::Counter::ShellLoops);
    (loops > 0).then_some(loops)
}

/// The recovery firmware has no shell to wait for.
#[cfg(not(with_shell))]
fn shell_loops() -> Option<u32> {
    None
}

/// Wakes the shell for one loop with nothing changed, which proves it still runs.
#[cfg(with_shell)]
fn ping_shell() {
    system_values::changed(&[]);
}

#[cfg(not(with_shell))]
fn ping_shell() {}

/// Reads the side button's pin straight from the port, without waiting out contact bounce.
fn side_button_is_held() -> bool {
    nrf_pac::P0.in_().read().pin(SIDE_BUTTON_PIN)
}

// ============================================================================
// The button's gestures
// ============================================================================

/// Sends each click, double click and long press to the shell; it sleeps until the pin changes.
#[embassy_executor::task]
async fn send_button_gestures(mut button: Button<Input<'static>, Output<'static>>) {
    crate::tasks::name_this_task("button");
    loop {
        let Ok(gesture) = button.next_gesture(&mut Delay).await;
        diagnostics::add_one(diagnostics::Counter::ButtonPresses);
        let _ = shell::BUTTON_GESTURES.try_send(gesture);
    }
}

// ============================================================================
// The screen
// ============================================================================

type ScreenDisplay = Display<DmaInterface, Output<'static>>;

/// The display, shared between the status page and the shell; a lock is never held across an await.
type SharedDisplay = Mutex<NoopRawMutex, RefCell<ScreenDisplay>>;

/// Where each kind of status sits on the page, shared with the bootloader so every page looks alike.
use neotime_status_screen::page as line;

/// One line's value, as long as the widest line allows.
type StatusLine = heapless::String<{ line::LINE_CHARACTERS }>;

/// The start-up and update status page, coloured by the firmware's mode; it keeps its lines while the shell has the screen.
struct Screen {
    display: &'static SharedDisplay,
    mode: status::Mode,
    lines: [StatusLine; line::LINE_COUNT],
}

impl Screen {
    /// Sets a line's value, cut to the room its label leaves.
    fn write_line(&mut self, line: usize, text: &str) {
        let mut stored = StatusLine::new();
        let _ = stored.push_str(line::fitted(line, text));
        self.lines[line] = stored;
        if shell::status_is_shown() {
            self.draw_line(line);
        }
    }

    fn draw_line(&self, line: usize) {
        self.display.lock(|display| {
            let _ = line::draw_line(
                &mut *display.borrow_mut(),
                line,
                &self.lines[line],
                self.mode.background(),
            );
        });
    }

    fn redraw(&self) {
        self.display.lock(|display| {
            let _ = display.borrow_mut().clear(self.mode.background());
        });
        for line in 0..line::LINE_COUNT {
            self.draw_line(line);
        }
    }

    /// Shows the mode as the page's colour, the firmware's line and what the mode means.
    fn set_mode(&mut self, mode: status::Mode) {
        self.mode = mode;
        self.write_line(line::FIRMWARE, mode.word());
        self.write_line(line::ACTION, mode.detail());
        if shell::status_is_shown() {
            self.redraw();
        }
    }

    /// Takes the screen back from the shell and keeps it lit, such as for an update's progress.
    fn show_status(&mut self) {
        if shell::show_status() {
            self.redraw();
        }
    }

    /// Lets the wearer dismiss the page with a touch or a press, and the screen time out again.
    fn release_status(&mut self) {
        shell::release_status();
    }
}

fn start_screen(
    spi_bus: &'static SpiBus,
    data_or_command: Output<'static>,
    reset: Output<'static>,
) -> Screen {
    static DISPLAY: StaticCell<SharedDisplay> = StaticCell::new();
    let interface = DmaInterface::new(spi_bus, data_or_command);

    let mut display = Display::new(interface, reset, &mut Delay).unwrap_or_else(|_| panic!("display"));
    let _ = display.clear(Rgb565::BLACK);
    Screen {
        display: DISPLAY.init(Mutex::new(RefCell::new(display))),
        mode: status::Mode::Confirmed,
        lines: Default::default(),
    }
}

/// Puts the previous run's panic or fault on the page, or else why the watch restarted.
fn show_last_restart(screen: &mut Screen, last_restart: restart::LastRestart) {
    // Room for the whole text, because a short buffer drops whole pieces and `write_line` cuts it to fit anyway.
    let mut text = heapless::String::<96>::new();
    let _ = match &last_restart.crash {
        Some(recorded) => core::fmt::write(&mut text, format_args!("{}", recorded.crash)),
        None => core::fmt::write(&mut text, format_args!("Reset: {}", last_restart.reasons)),
    };
    screen.write_line(line::LAST_RUN, &text);
}

// ============================================================================
// Bluetooth: advertise, serve GATT, take updates
// ============================================================================

#[gatt_server]
struct Server {
    dfu: LegacyDfuService,
    time: CurrentTimeService,
    neotime: NeoTimeService,
    uart: NordicUartService,
    battery: BatteryService,
    file_transfer: FileTransferService,
    device_information: DeviceInformationService,
    alert_notification: AlertNotificationService,
    heart_rate: HeartRateService,
    #[cfg(weather_service)]
    simple_weather: infinitime_services::SimpleWeatherService,
    #[cfg(music_service)]
    music: infinitime_services::MusicService,
    #[cfg(navigation_service)]
    navigation: infinitime_services::NavigationService,
    #[cfg(alert_service)]
    immediate_alert: infinitime_services::ImmediateAlertService,
    #[cfg(motion_service)]
    motion: infinitime_services::MotionService,
}

/// The standard Alert Notification service: any Bluetooth tool writes a New Alert to show a notification.
#[gatt_service(uuid = "1811")]
struct AlertNotificationService {
    /// Writes are read as they arrive and never stored, so one byte of storage takes any length.
    #[characteristic(uuid = "2a46", write)]
    new_alert: u8,
}

/// The id the next New Alert gets, counted above Gadgetbridge's 32-bit ids so the two never meet.
#[cfg(with_shell)]
static NEXT_NEW_ALERT: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);

/// Shows a New Alert, written to the watch or notified by the phone, as a notification, or as a call when it is one.
#[cfg(with_shell)]
fn read_new_alert(alert: &[u8], parse: fn(&[u8], i64) -> Option<neotime_companion::Message<'static>>) {
    let count = NEXT_NEW_ALERT.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    let id = (1 << 32) + i64::from(count);
    match parse(alert, id) {
        Some(neotime_companion::Message::Notification(notification)) => {
            app_events::notification_arrived(notification)
        }
        Some(call) => app_events::phone_message_arrived(call),
        None => {}
    }
}

/// The standard Device Information service, with the same characteristics InfiniTime serves.
#[gatt_service(uuid = "180a")]
struct DeviceInformationService {
    #[characteristic(uuid = "2a29", read, value = neotime_companion::MANUFACTURER_NAME)]
    manufacturer_name: &'static str,
    #[characteristic(uuid = "2a24", read, value = neotime_companion::MODEL_NUMBER)]
    model_number: &'static str,
    /// Filled from the chip's factory id once the server is built.
    #[characteristic(uuid = "2a25", read)]
    serial_number: [u8; 16],
    /// The firmware's version, as the status page shows it.
    #[characteristic(uuid = "2a26", read, value = FIRMWARE_VERSION)]
    firmware_revision: &'static str,
    #[characteristic(uuid = "2a27", read, value = neotime_companion::HARDWARE_REVISION)]
    hardware_revision: &'static str,
    #[characteristic(uuid = "2a28", read, value = neotime_companion::SOFTWARE_REVISION)]
    software_revision: &'static str,
}

/// The standard Battery service: the charge, 0 to 100, read or notified when it changes.
#[gatt_service(uuid = "180f")]
struct BatteryService {
    #[characteristic(uuid = "2a19", read, notify)]
    level: u8,
}

/// The standard Heart Rate service: the latest reading, read or notified as each one is taken.
#[gatt_service(uuid = "180d")]
struct HeartRateService {
    /// A flags byte and the beats per minute, as `neotime_companion::heart_rate_measurement` builds it.
    #[characteristic(uuid = "2a37", read, notify)]
    measurement: [u8; 2],
}

/// The Nordic UART service, which carries the companion app's JSON lines; see docs/companion-protocol.md.
#[gatt_service(uuid = "6e400001-b5a3-f393-e0a9-e50e24dcca9e")]
struct NordicUartService {
    #[characteristic(uuid = "6e400002-b5a3-f393-e0a9-e50e24dcca9e", write, write_without_response)]
    receive: [u8; 20],
    #[characteristic(uuid = "6e400003-b5a3-f393-e0a9-e50e24dcca9e", notify)]
    transmit: [u8; 20],
}

/// The longest packages command kept; a longer one is cut and then refused as unreadable.
const PACKAGES_LINE_BYTES: usize = 256;

/// The bytes one notification carries at the smallest MTU, 23, less the 3-byte header.
const NOTIFICATION_BYTES: usize = 20;

/// NeoTime's own service: the counters and last restart from `diagnostics`, and the packages commands that install apps.
#[gatt_service(uuid = "6e540001-0000-4000-8000-4e656f54696d")]
struct NeoTimeService {
    /// Read for the counters; writing `DEBUG_REPORT_SAVED` says the companion app saved the crash.
    #[characteristic(uuid = "6e540002-0000-4000-8000-4e656f54696d", read, write, value = [0; diagnostics::SNAPSHOT_BYTES])]
    counters: [u8; diagnostics::SNAPSHOT_BYTES],
    /// One JSON command per line in, one JSON reply per line out; see docs/companion-protocol.md.
    #[characteristic(
        uuid = "6e540003-0000-4000-8000-4e656f54696d",
        write,
        write_without_response,
        notify
    )]
    packages: [u8; 20],
}

/// The standard Current Time service, which phones write the time and time zone to and read them from.
#[gatt_service(uuid = "1805")]
struct CurrentTimeService {
    #[characteristic(uuid = "2a2b", read, write)]
    current_time: [u8; 10],
    #[characteristic(uuid = "2a0f", read, write)]
    local_time_information: [u8; 2],
}

/// Fills in the serial number, the one Device Information value not known when the firmware is built.
#[inline(never)]
fn set_serial_number(server: &Server<'static>) {
    let serial_number = neotime_companion::serial_number(device_id());
    let _ = server.set(&server.device_information.serial_number, &serial_number);
}

/// The chip's 64-bit factory id, unique to each nRF52.
fn device_id() -> u64 {
    const FICR_DEVICE_ID_LOW: *const u32 = 0x1000_0060 as *const u32;
    const FICR_DEVICE_ID_HIGH: *const u32 = 0x1000_0064 as *const u32;
    let low = unsafe { FICR_DEVICE_ID_LOW.read_volatile() };
    let high = unsafe { FICR_DEVICE_ID_HIGH.read_volatile() };
    (u64::from(high) << 32) | u64::from(low)
}

/// The chip's factory address with its top two bits set, the same static random address the controller reports.
fn device_address() -> [u8; 6] {
    const FICR_DEVICE_ADDRESS_LOW: *const u32 = 0x1000_00A4 as *const u32;
    const FICR_DEVICE_ADDRESS_HIGH: *const u32 = 0x1000_00A8 as *const u32;
    let low = unsafe { FICR_DEVICE_ADDRESS_LOW.read_volatile() }.to_le_bytes();
    let high = unsafe { FICR_DEVICE_ADDRESS_HIGH.read_volatile() }.to_le_bytes();
    [low[0], low[1], low[2], low[3], high[0], high[1] | 0xC0]
}

/// What the Bluetooth task keeps for the whole run, held in a static and lent to it, since an async
/// task's arguments are stored twice in its state.
struct BluetoothParts {
    dfu: LegacyDfu<storage::Targets>,
    confirmation: storage::Confirmation,
    screen: Screen,
}

impl BluetoothParts {
    fn new(targets: storage::Targets, confirmation: storage::Confirmation, screen: Screen) -> BluetoothParts {
        BluetoothParts {
            dfu: LegacyDfu::new(targets),
            confirmation,
            screen,
        }
    }
}

static BLUETOOTH_PARTS: StaticCell<BluetoothParts> = StaticCell::new();
static SERVER: StaticCell<Server<'static>> = StaticCell::new();

/// Builds the server straight into its static, so it never passes through the stack.
#[inline(never)]
fn build_server() -> &'static Server<'static> {
    let gap = GapConfig::Peripheral(PeripheralConfig {
        name: DEVICE_NAME,
        appearance: &appearance::watch::SMARTWATCH,
    });
    Server::init_in_place(SERVER.uninit(), gap).unwrap()
}

/// The GAP service's device name, the first characteristic in the table.
const DEVICE_NAME_HANDLE: u16 = 3;

/// How long to wait before trying again when the controller refuses to start advertising.
const ADVERTISING_RETRY_DELAY: Duration = Duration::from_secs(1);

/// Gives the GAP service the name the watch advertises now.
#[inline(never)]
fn set_device_name(server: &Server<'static>, name: &'static &'static str) {
    if let Ok(device_name) = server
        .table()
        .find_characteristic_by_value_handle::<&'static str>(DEVICE_NAME_HANDLE)
    {
        let _ = device_name.set_ro(server, name);
    }
}

/// The advertising intervals InfiniTime asks for, fast or about once a second.
fn advertising_parameters(
    interval: neotime_companion::advertising::AdvertisingInterval,
) -> AdvertisementParameters {
    AdvertisementParameters {
        interval_min: Duration::from_micros(interval.shortest_in_microseconds),
        interval_max: Duration::from_micros(interval.longest_in_microseconds),
        ..Default::default()
    }
}

/// Its own task, so the Bluetooth loop's stack frame is not added to start-up's.
#[embassy_executor::task]
async fn run_bluetooth(
    controller: neotime_bluetooth::BluetoothController<'static, neotime_bluetooth::NrfRadio>,
    parts: &'static mut BluetoothParts,
    files: &'static storage::Files,
) {
    crate::tasks::name_this_task("bluetooth");
    let BluetoothParts {
        dfu,
        confirmation,
        screen,
    } = parts;
    let controller: ExternalController<_, 4> = ExternalController::new(controller);
    let resources = HOST_RESOURCES.init(HostResources::new());
    let stack = trouble_host::new(controller, resources)
        .set_random_address(Address::random(device_address()))
        .build();
    let mut runner = stack.runner();
    let mut peripheral = stack.peripheral();
    let server = build_server();
    set_serial_number(server);

    let serve = async {
        let mut advertising_shown = false;
        let mut confirmed = false;
        // Advertising is fast for a while from here, as after a phone leaves or Bluetooth comes back on.
        let mut fast_from = Instant::now();
        loop {
            if !settings::is_bluetooth_on() {
                screen.write_line(line::BLUETOOTH, "Off");
                while !settings::is_bluetooth_on() {
                    settings::BLUETOOTH_SWITCHED.wait().await;
                }
                advertising_shown = false;
                fast_from = Instant::now();
            }
            let interval = neotime_companion::advertising::advertising_interval(
                fast_from.as_millis(),
                Instant::now().as_millis(),
            );
            let is_fast = interval == neotime_companion::advertising::FAST_INTERVAL;
            let name = settings::advertised_name();
            set_device_name(server, name);
            let scan_response = neotime_companion::advertising::scan_response(name);
            let Ok(advertiser) = peripheral
                .advertise(
                    &advertising_parameters(interval),
                    Advertisement::ConnectableScannableUndirected {
                        adv_data: &neotime_companion::advertising::ADVERTISEMENT,
                        scan_data: &scan_response,
                    },
                )
                .await
            else {
                Timer::after(ADVERTISING_RETRY_DELAY).await;
                continue;
            };
            if !advertising_shown {
                screen.write_line(line::BLUETOOTH, "Advertising");
                advertising_shown = true;
            }

            // Fast advertising stops when its time is up and starts again slow.
            let fast_ends_at = fast_from
                + Duration::from_millis(neotime_companion::advertising::FAST_ADVERTISING_MILLISECONDS);
            let slow_down = async {
                match is_fast {
                    true => Timer::at(fast_ends_at).await,
                    false => core::future::pending().await,
                }
            };
            let Either::First(Ok(connection)) = select(
                advertiser.accept(),
                select(settings::BLUETOOTH_SWITCHED.wait(), slow_down),
            )
            .await
            else {
                continue;
            };
            let Ok(connection) = connection.with_attribute_server(server) else {
                continue;
            };
            screen.write_line(line::BLUETOOTH, "Connected");
            phone::set_connected(true);
            let serving = serve_connection(
                &connection,
                server,
                dfu,
                screen,
                confirmation,
                &mut confirmed,
                files,
            );
            // The phone's own services are used beside serving the watch's, and end with the connection.
            #[cfg(with_shell)]
            let serving = select(serving, phone_services::use_phone_services(connection.raw()));
            serving.await;
            phone::set_connected(false);
            screen.write_line(line::BLUETOOTH, "Advertising");
            fast_from = Instant::now();
        }
    };

    let _ = embassy_futures::join::join(runner.run(), serve).await;
}

/// Hands a message to the shell, or answers `status` at once; unknown messages are passed over.
#[cfg(with_shell)]
fn read_companion_line(line: &[u8]) {
    if clock::read_time_line(line) {
        return;
    }
    match neotime_companion::parse_line(line) {
        Ok(neotime_companion::Message::Settings(change)) => {
            let _ = settings::CHANGES.try_send(change);
        }
        Ok(neotime_companion::Message::Notification(notification)) => {
            app_events::notification_arrived(notification);
        }
        Ok(neotime_companion::Message::StatusRequest) => send_status(),
        Ok(neotime_companion::Message::NotificationDismissed { id }) => {
            app_events::notification_dismissed(id);
        }
        Ok(neotime_companion::Message::Slots(request)) => {
            let _ = SLOTS_REQUESTS.try_send(request);
        }
        Ok(neotime_companion::Message::AppState { app, step }) => app_events::app_state_changed(app, step),
        Ok(neotime_companion::Message::Unknown { .. }) => {}
        Ok(phone_message) => app_events::phone_message_arrived(phone_message),
        Err(_) => {
            settings::send_reply(&neotime_companion::error_report("could not read that message"));
        }
    }
}

/// Answers `status` and refuses `slots`; with no shell to show them, every other message is passed over.
#[cfg(not(with_shell))]
fn read_companion_line(line: &[u8]) {
    if clock::read_time_line(line) {
        return;
    }
    match neotime_companion::message_kind(line) {
        Ok("status") => send_status(),
        // The reloaders carry the recovery firmware twice in wasp-os's app slot, so it stays without this.
        Ok("slots") => {
            settings::send_reply(&neotime_companion::slots_refusal(
                "the recovery firmware cannot move the slots",
            ));
        }
        Ok(_) => {}
        Err(_) => {
            settings::send_reply(&neotime_companion::error_report("could not read that message"));
        }
    }
}

/// `slots` requests, answered by the Bluetooth task, which holds the flash.
#[cfg(with_shell)]
static SLOTS_REQUESTS: embassy_sync::channel::Channel<
    embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex,
    neotime_companion::SlotsRequest,
    1,
> = embassy_sync::channel::Channel::new();

/// Answers the companion app's `status` with the battery and today's steps.
fn send_status() {
    let status = neotime_companion::WatchStatus {
        battery_percent: battery::percent(),
        battery_millivolts: battery::millivolts(),
        steps_today: motion::steps_today(),
        is_charging: battery::is_charging(),
    };
    settings::send_reply(&neotime_companion::status_report(&status));
}

/// How long to wait for a free packet when a long answer, such as a folder listing, fills the send queue.
const NOTIFY_RETRY_DELAY: Duration = Duration::from_millis(20);
const NOTIFY_ATTEMPTS: usize = 50;
/// The bytes a notification spends on its opcode and handle.
const NOTIFICATION_HEADER_BYTES: usize = 3;
/// Packets a long answer leaves free, so what the phone sends meanwhile still has somewhere to land.
const PACKETS_KEPT_FOR_INCOMING: usize = 2;
/// The most of one write the clock, companion and packages characteristics read.
const MOST_WRITE_BYTES: usize = 64;
/// The most of one write the update service reads: legacy DFU sends 20-byte packets and shorter commands.
const DFU_WRITE_BYTES: usize = 20;

/// Sends one notification through a single out-of-line copy of trouble's sending code, whatever the characteristic.
async fn notify_bytes(
    connection: &GattConnection<'_, '_, DefaultPacketPool>,
    characteristic: &Characteristic<[u8]>,
    value: &[u8],
    store: bool,
) -> Result<(), trouble_host::Error> {
    shell::OutOfLine(characteristic.notify_raw(connection, value, store)).await
}

/// Sends a companion reply on the UART characteristic in 20-byte notifications.
async fn send_uart_reply(
    connection: &GattConnection<'_, '_, DefaultPacketPool>,
    server: &Server<'_>,
    reply: &str,
) {
    for piece in reply.as_bytes().chunks(NOTIFICATION_BYTES) {
        let _ = notify_bytes(connection, &server.uart.transmit.to_raw(), piece, false).await;
    }
}

/// Sends one notification, waiting while fewer than two packets are free or the send queue is full.
async fn notify_patiently(
    connection: &GattConnection<'_, '_, DefaultPacketPool>,
    characteristic: &Characteristic<[u8]>,
    piece: &[u8],
) {
    for _ in 0..NOTIFY_ATTEMPTS {
        if free_default_packets() < PACKETS_KEPT_FOR_INCOMING {
            Timer::after(NOTIFY_RETRY_DELAY).await;
            continue;
        }
        match notify_bytes(connection, characteristic, piece, false).await {
            Err(trouble_host::Error::OutOfMemory) => Timer::after(NOTIFY_RETRY_DELAY).await,
            _ => break,
        }
    }
}

/// Sends a BLEFS command's answers, one notification each.
async fn send_file_answers(
    connection: &GattConnection<'_, '_, DefaultPacketPool>,
    server: &Server<'_>,
    answers: alloc::vec::Vec<alloc::vec::Vec<u8>>,
) {
    for answer in answers {
        notify_patiently(connection, &server.file_transfer.transfer.to_raw(), &answer).await;
    }
}

/// Sends a packages reply in 20-byte notifications, since an app list takes more notifications than the pool has packets.
async fn send_packages_reply(
    connection: &GattConnection<'_, '_, DefaultPacketPool>,
    server: &Server<'_>,
    reply: &str,
) {
    for piece in reply.as_bytes().chunks(NOTIFICATION_BYTES) {
        notify_patiently(connection, &server.neotime.packages.to_raw(), piece).await;
    }
}

/// Asks the phone for the connection parameters the link's pace wants, if they changed, and gives
/// when to look again.
///
/// A phone that turns the request down keeps the link as it was, but one that never went out is sent again.
#[inline(never)]
fn ask_for_link_pace(
    connection: &GattConnection<'_, '_, DefaultPacketPool>,
    link_pace: &mut LinkPace,
) -> Instant {
    let now = Instant::now().as_secs();
    if let Some(wanted) = link_pace.request(now) {
        let sent = connection.raw().try_request_connection_params(
            wanted.interval_min,
            wanted.interval_max,
            wanted.latency,
            wanted.supervision_timeout,
        );
        if sent.is_err() {
            link_pace.retry_later(now);
        }
    }
    link_pace
        .next_change_seconds()
        .map_or(Instant::MAX, Instant::from_secs)
}

/// Confirms the firmware the first time a phone exchanges data with it, and says so on the status page.
async fn confirm_once(screen: &mut Screen, confirmation: &mut storage::Confirmation, confirmed: &mut bool) {
    if !*confirmed {
        let kept = confirmation.confirm().await;
        if screen.mode == status::Mode::OnTrial {
            if kept {
                screen.set_mode(status::Mode::Confirmed);
                screen.write_line(line::ACTION, "Update kept");
            } else {
                screen.write_line(line::ACTION, "Could not keep it");
            }
        }
        *confirmed = true;
    }
}

/// The first data a phone exchanges proves the radio works end to end, so that is when the firmware confirms itself.
///
/// Each slow step is polled out of line, which keeps this function's state machine small.
// Only the shell's builds move slots, the one use of `files` here.
#[cfg_attr(not(with_shell), allow(unused_variables))]
async fn serve_connection(
    connection: &GattConnection<'_, '_, DefaultPacketPool>,
    server: &Server<'_>,
    dfu: &mut LegacyDfu<storage::Targets>,
    screen: &mut Screen,
    confirmation: &mut storage::Confirmation,
    confirmed: &mut bool,
    files: &'static storage::Files,
) {
    let mut shown_progress = ShownProgress::default();
    let mut companion_lines = LineAssembler::new();
    storage_requests::REQUESTS
        .send(storage_requests::StorageRequest::StartFileTransfer)
        .await;
    // A packages command so far, on the heap since commands are short and rare.
    let mut packages_line: alloc::vec::Vec<u8> = alloc::vec::Vec::new();
    // Complete packages commands, handed to their task once the write that finished them is answered.
    let mut packages_commands: alloc::vec::Vec<alloc::vec::Vec<u8>> = alloc::vec::Vec::new();
    // A reply left from a phone that went away before it arrived is not this phone's.
    packages::REPLIES.clear();
    let mut link_pace = LinkPace::new(Instant::now().as_secs());
    loop {
        let pace_change = ask_for_link_pace(connection, &mut link_pace);
        // Pinned where they are made, so the connection's large future is never copied across the stack.
        let next_event = core::pin::pin!(connection.next());
        let next_reply = core::pin::pin!(select(settings::REPLIES.receive(), packages::REPLIES.receive()));
        let reading_change = core::pin::pin!(select3(
            battery::PERCENT_CHANGED.wait(),
            heart_rate::READING_TAKEN.wait(),
            infinitime_services::next_notification(),
        ));
        let bluetooth_switched = core::pin::pin!(select3(
            settings::BLUETOOTH_SWITCHED.wait(),
            clock::SET_FROM_PHONE_SERVICE.wait(),
            Timer::at(pace_change)
        ));
        let event = match shell::OutOfLine(select4(
            next_event,
            next_reply,
            reading_change,
            bluetooth_switched,
        ))
        .await
        {
            Either4::First(event) => event,
            Either4::Second(Either::First(reply)) => {
                send_uart_reply(connection, server, &reply).await;
                continue;
            }
            Either4::Second(Either::Second(reply)) => {
                link_pace.note_transfer(Instant::now().as_secs());
                shell::OutOfLine(send_packages_reply(connection, server, &reply)).await;
                continue;
            }
            Either4::Third(Either3::First(())) => {
                let percent = battery::percent();
                let _ = notify_bytes(
                    connection,
                    &server.battery.level.to_raw(),
                    percent.as_gatt(),
                    true,
                )
                .await;
                continue;
            }
            Either4::Third(Either3::Second(())) => {
                let measurement = heart_rate::measurement();
                let _ = notify_bytes(
                    connection,
                    &server.heart_rate.measurement.to_raw(),
                    measurement.as_gatt(),
                    true,
                )
                .await;
                continue;
            }
            Either4::Third(Either3::Third(notification)) => {
                infinitime_services::notify(connection, server, notification).await;
                continue;
            }
            // The link stays until the controller reports it gone, which ends this connection as usual.
            Either4::Fourth(Either3::First(())) => {
                if !settings::is_bluetooth_on() {
                    connection.raw().disconnect();
                }
                continue;
            }
            Either4::Fourth(Either3::Second(())) => {
                shell::OutOfLine(confirm_once(screen, confirmation, confirmed)).await;
                continue;
            }
            // The link has settled, so the top of the loop asks for the slower parameters.
            Either4::Fourth(Either3::Third(())) => continue,
        };
        match event {
            GattConnectionEvent::Disconnected { .. } => {
                infinitime_services::note_subscriptions(server, None);
                let was_updating = dfu.is_updating();
                // What reached the flash is kept, so the same image sent again carries on from there.
                dfu.pause();
                if was_updating {
                    let mut text = StatusLine::new();
                    let _ = core::fmt::write(
                        &mut text,
                        format_args!("Kept {} KB", dfu.paused_bytes().unwrap_or(0) / 1024),
                    );
                    screen.write_line(line::ACTION, "Update paused");
                    screen.write_line(line::PROGRESS, &text);
                }
                screen.release_status();
                return;
            }
            GattConnectionEvent::Gatt { event } => {
                // Only an update's 20 bytes outlive the awaits below, never a whole write.
                let mut dfu_written = heapless::Vec::<u8, DFU_WRITE_BYTES>::new();
                let mut file_answers = alloc::vec::Vec::new();
                let mut unstored = false;
                let target = match &event {
                    GattEvent::Write(write) => {
                        let handle = write.handle();
                        let target = server.dfu.classify(handle);
                        let file_request = write.with_data(|_, bytes| {
                            let written = &bytes[..bytes.len().min(MOST_WRITE_BYTES)];
                            unstored = infinitime_services::take_write(server, handle, bytes);
                            if target.is_some() {
                                let _ = dfu_written
                                    .extend_from_slice(&written[..written.len().min(DFU_WRITE_BYTES)]);
                            }
                            if handle == server.time.current_time.handle {
                                clock::set_from_current_time(written);
                            }
                            if handle == server.time.local_time_information.handle {
                                clock::set_local_time_information(written);
                            }
                            if handle == server.uart.receive.handle {
                                companion_lines.push(written, read_companion_line);
                            }
                            if handle == server.neotime.counters.handle
                                && written.first() == Some(&diagnostics::DEBUG_REPORT_SAVED)
                            {
                                restart::dismiss_crash();
                                ping_shell();
                            }
                            if handle == server.neotime.packages.handle {
                                for &byte in written {
                                    if byte == b'\n' {
                                        packages_commands.push(core::mem::take(&mut packages_line));
                                    } else if packages_line.len() < PACKAGES_LINE_BYTES {
                                        packages_line.push(byte);
                                    }
                                }
                            }
                            (handle == server.file_transfer.transfer.handle)
                                .then(|| storage_requests::FileRequest::copy_of(bytes))
                        });
                        if file_request.is_some()
                            || target.is_some()
                            || handle == server.neotime.packages.handle
                        {
                            link_pace.note_transfer(Instant::now().as_secs());
                        }
                        if let Some(file_request) = file_request {
                            shell::OutOfLine(foreground::wait_for_a_pause()).await;
                            let room = connection.raw().att_mtu() as usize - NOTIFICATION_HEADER_BYTES;
                            file_answers =
                                shell::OutOfLine(storage_requests::answer_file_transfer(file_request, room))
                                    .await;
                        }
                        // The recovery firmware has nowhere to show a notification.
                        #[cfg(with_shell)]
                        if handle == server.alert_notification.new_alert.handle {
                            write.with_data(|_, bytes| {
                                read_new_alert(
                                    &bytes[..bytes.len().min(MOST_NEW_ALERT_BYTES)],
                                    neotime_companion::parse_new_alert,
                                )
                            });
                        }
                        target
                    }
                    GattEvent::Read(read) => {
                        infinitime_services::fill_read(server, read.handle());
                        if read.handle() == server.neotime.counters.handle {
                            let _ = server.set(&server.neotime.counters, &diagnostics::snapshot());
                        }
                        if read.handle() == server.battery.level.handle {
                            let _ = server.set(&server.battery.level, &battery::percent());
                        }
                        if read.handle() == server.heart_rate.measurement.handle {
                            let _ = server.set(&server.heart_rate.measurement, &heart_rate::measurement());
                        }
                        if read.handle() == server.time.current_time.handle {
                            let _ = server.set(&server.time.current_time, &clock::current_time());
                        }
                        if read.handle() == server.time.local_time_information.handle {
                            let _ = server.set(
                                &server.time.local_time_information,
                                &clock::local_time_information(),
                            );
                        }
                        None
                    }
                    _ => None,
                };
                let reply = match event {
                    GattEvent::Write(write)
                        if unstored || write.handle() == server.alert_notification.new_alert.handle =>
                    {
                        write.accept_unprocessed()
                    }
                    event => event.accept(),
                };
                if let Ok(reply) = reply {
                    shell::OutOfLine(reply.send()).await;
                }
                infinitime_services::note_subscriptions(server, Some(connection));
                shell::OutOfLine(clock::save_time_zone_if_changed()).await;
                for command in packages_commands.drain(..) {
                    shell::OutOfLine(packages::COMMANDS.send(command)).await;
                }
                #[cfg(with_shell)]
                if let Ok(request) = SLOTS_REQUESTS.try_receive() {
                    let answer = shell::OutOfLine(storage::answer_slots(files, confirmation, request)).await;
                    let must_restart = matches!(answer, Ok((_, true)));
                    // Built on the heap after the flash work, so the connection's state never holds a reply line.
                    let answer = storage::slots_answer_line(files, answer);
                    send_uart_reply(connection, server, &answer).await;
                    if must_restart {
                        screen.write_line(line::ACTION, "Slots moved");
                        shell::OutOfLine(storage::reset(screen)).await;
                    }
                }
                shell::OutOfLine(confirm_once(screen, confirmation, confirmed)).await;
                shell::OutOfLine(send_file_answers(connection, server, file_answers)).await;

                dfu.set_allows_updates(settings::current().is_on(Setting::AllowsUpdates));
                let outcome = match target {
                    Some(DfuWrite::ControlPoint) => {
                        shell::OutOfLine(dfu.on_control_point(&dfu_written)).await
                    }
                    Some(DfuWrite::Packet) => shell::OutOfLine(dfu.on_packet(&dfu_written)).await,
                    None => continue,
                };
                for notification in &outcome.notifications {
                    let _ = notify_bytes(
                        connection,
                        &server.dfu.control_point.to_raw(),
                        notification,
                        false,
                    )
                    .await;
                }
                show_update(screen, dfu, &outcome, &mut shown_progress);
                if outcome.reset_requested {
                    screen.write_line(line::ACTION, "Restarting to install");
                    shell::OutOfLine(storage::reset(screen)).await;
                }
            }
            _ => {}
        }
    }
}

// ============================================================================
// An update's progress on the status page
// ============================================================================

/// What the page last showed of an update, so a packet that changes nothing draws nothing.
#[derive(Default, PartialEq, Eq)]
struct ShownProgress {
    activity: Option<Activity>,
    kilobytes_received: u32,
}

/// Puts the page up for an update, and shows its step and how much has arrived, or why it was refused.
fn show_update(
    screen: &mut Screen,
    dfu: &LegacyDfu<storage::Targets>,
    outcome: &Outcome,
    shown: &mut ShownProgress,
) {
    if let Some(failure) = outcome.failure() {
        screen.show_status();
        screen.write_line(line::ACTION, "Update failed");
        screen.write_line(line::PROGRESS, failure.words());
        screen.release_status();
        *shown = ShownProgress::default();
        return;
    }
    let activity = dfu.activity();
    if activity == Activity::Idle {
        return;
    }
    let now = ShownProgress {
        activity: Some(activity),
        kilobytes_received: dfu.bytes_received() / 1024,
    };
    if now == *shown {
        return;
    }

    screen.show_status();
    if shown.activity != now.activity {
        let slot = match dfu.image_kind() {
            ImageKind::Firmware => "firmware",
            ImageKind::Recovery => "recovery",
        };
        let step = match activity {
            Activity::Idle | Activity::Starting => "Starting",
            Activity::Receiving => "Receiving",
            Activity::Checking => "Checking",
            Activity::Installing => "Installing",
        };
        let mut text = StatusLine::new();
        let _ = core::fmt::write(&mut text, format_args!("{step} {slot}"));
        screen.write_line(line::ACTION, &text);
    }
    let mut progress = StatusLine::new();
    let _ = write_progress(&mut progress, dfu.bytes_received(), dfu.image_size_in_bytes());
    screen.write_line(line::PROGRESS, &progress);
    *shown = now;
}
