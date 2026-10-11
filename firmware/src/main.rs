//! NeoTime's firmware: the first one, which proves the chain from bootloader to Bluetooth.
//!
//! It takes over the bootloader's watchdog and feeds it only while the side button is up, shows a
//! page on the screen, mounts the file system, advertises as "NeoTime", takes firmware updates over
//! legacy DFU, and confirms itself to the bootloader once a phone has exchanged data with it.
//!
//! The `wasp-os-rehearsal` feature builds it as an app for wasp-os instead, which touches no flash;
//! see docs/updates.md.

#![no_std]
#![no_main]

extern crate alloc;

use core::cell::RefCell;

use embassy_embedded_hal::shared_bus::blocking::spi::SpiDevice;
use embassy_executor::Spawner;
use embassy_nrf::{
    bind_interrupts,
    config::LfclkSource,
    gpio::{Input, Level, Output, OutputDrive, Pull},
    peripherals,
    spim::{self, Spim},
    wdt::{self, SleepConfig, Watchdog, WatchdogHandle},
};
use embassy_sync::blocking_mutex::{raw::NoopRawMutex, Mutex};
use embassy_time::{Delay, Duration, Timer};
use embedded_alloc::LlffHeap;
use embedded_graphics::{
    mono_font::{ascii::FONT_10X20, MonoTextStyle},
    pixelcolor::Rgb565,
    prelude::*,
    text::Text,
};
use mipidsi::interface::SpiInterface;
use neotime_boot::{FIRMWARE_ALIVE_MAGIC, FIRMWARE_ALIVE_MARKER, WATCHDOG_TIMEOUT_IN_TICKS};
use neotime_button::Button;
use neotime_dfu::{service::DfuWrite, service::LegacyDfuService, LegacyDfu};
use neotime_display::Display;
use panic_halt as _;
use static_cell::StaticCell;
use trouble_host::prelude::*;

#[cfg_attr(not(feature = "wasp-os-rehearsal"), path = "storage/watch.rs")]
#[cfg_attr(feature = "wasp-os-rehearsal", path = "storage/rehearsal.rs")]
mod storage;

// ============================================================================
// PineTime wiring
// ============================================================================

bind_interrupts!(struct Interrupts {
    TWISPI0 => spim::InterruptHandler<peripherals::TWISPI0>;
    RADIO => neotime_bluetooth::RadioInterruptHandler;
});

const FIRMWARE_VERSION: &str = "0.1.0";
const DEVICE_NAME: &str = "NeoTime";
const WATCHDOG_FEED_INTERVAL: Duration = Duration::from_millis(1000);
const DISPLAY_COMMAND_BUFFER_SIZE: usize = 512;
const HEAP_SIZE: usize = 16 * 1024;

type SpiBus = Mutex<NoopRawMutex, RefCell<Spim<'static>>>;

#[global_allocator]
static HEAP: LlffHeap = LlffHeap::empty();

static SPI_BUS: StaticCell<SpiBus> = StaticCell::new();
static HOST_RESOURCES: StaticCell<HostResources<DefaultPacketPool, 1, 2>> = StaticCell::new();

// ============================================================================
// Start
// ============================================================================

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    storage::prepare_clocks();
    let mut config = embassy_nrf::config::Config::default();
    config.lfclk_source = LfclkSource::ExternalXtal;
    // Leave the debug port's settings alone, so start-up never writes the protection word.
    config.debug = embassy_nrf::config::Debug::NotConfigured;
    let chip = embassy_nrf::init(config);

    start_heap();
    mark_alive();
    let watchdog = take_over_watchdog(chip.WDT);

    let button_pin = Input::new(chip.P0_13, Pull::Down);
    let power_pin = Output::new(chip.P0_15, Level::Low, OutputDrive::Standard);
    let Ok(button) = Button::new(button_pin, power_pin);
    spawner.spawn(feed_watchdog_while_button_is_up(watchdog, button).unwrap());

    let mut spi_config = spim::Config::default();
    spi_config.frequency = spim::Frequency::M8;
    let spi_bus = SPI_BUS.init(Mutex::new(RefCell::new(Spim::new(
        chip.TWISPI0,
        Interrupts,
        chip.P0_02,
        chip.P0_04,
        chip.P0_03,
        spi_config,
    ))));

    let mut screen = start_screen(
        spi_bus,
        Output::new(chip.P0_25, Level::High, OutputDrive::Standard),
        Output::new(chip.P0_18, Level::Low, OutputDrive::Standard),
        Output::new(chip.P0_26, Level::High, OutputDrive::Standard),
    );
    let _backlight_on = Output::new(chip.P0_14, Level::Low, OutputDrive::Standard);
    screen.write_line(0, "NeoTime");
    screen.write_line(1, FIRMWARE_VERSION);
    screen.write_line(6, storage::BUILD_NAME);

    let storage_peripherals = storage::StoragePeripherals {
        flash_chip_select: chip.P0_05,
        flash_controller: chip.NVMC,
    };
    let Some((targets, confirmation)) = storage::start(spi_bus, storage_peripherals, &mut screen).await
    else {
        return;
    };

    let controller = neotime_bluetooth::start(
        neotime_bluetooth::RadioPeripherals {
            radio: chip.RADIO,
            timer: chip.TIMER0,
            random_number_generator: chip.RNG,
            start_transmit_channel: chip.PPI_CH20,
            start_receive_channel: chip.PPI_CH21,
            give_up_channel: chip.PPI_CH22,
            address_time_channel: chip.PPI_CH26,
            end_time_channel: chip.PPI_CH27,
        },
        Interrupts,
    );
    run_bluetooth(controller, targets, confirmation, screen).await;
}

fn start_heap() {
    static mut HEAP_MEMORY: [u8; HEAP_SIZE] = [0; HEAP_SIZE];
    unsafe { HEAP.init(core::ptr::addr_of_mut!(HEAP_MEMORY) as usize, HEAP_SIZE) }
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
#[embassy_executor::task]
async fn feed_watchdog_while_button_is_up(
    mut watchdog: WatchdogHandle,
    mut button: Button<Input<'static>, Output<'static>>,
) {
    loop {
        if button.is_pressed() != Ok(true) {
            watchdog.pet();
        }
        Timer::after(WATCHDOG_FEED_INTERVAL).await;
    }
}

// ============================================================================
// The screen
// ============================================================================

type DisplayInterfaceOnBus =
    SpiInterface<'static, SpiDevice<'static, NoopRawMutex, Spim<'static>, Output<'static>>, Output<'static>>;

struct Screen {
    display: Display<DisplayInterfaceOnBus, Output<'static>>,
}

impl Screen {
    const LINE_HEIGHT: i32 = 24;
    const FIRST_BASELINE: i32 = 40;

    fn write_line(&mut self, line: i32, text: &str) {
        let style = MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE);
        let baseline = Self::FIRST_BASELINE + line * Self::LINE_HEIGHT;
        let _ = Text::new(text, Point::new(12, baseline), style).draw(&mut self.display);
    }
}

fn start_screen(
    spi_bus: &'static SpiBus,
    chip_select: Output<'static>,
    data_or_command: Output<'static>,
    reset: Output<'static>,
) -> Screen {
    static COMMAND_BUFFER: StaticCell<[u8; DISPLAY_COMMAND_BUFFER_SIZE]> = StaticCell::new();
    let interface = SpiInterface::new(
        SpiDevice::new(spi_bus, chip_select),
        data_or_command,
        COMMAND_BUFFER.init([0; DISPLAY_COMMAND_BUFFER_SIZE]),
    );

    let mut display = Display::new(interface, reset, &mut Delay).unwrap_or_else(|_| panic!("display"));
    let _ = display.clear(Rgb565::BLACK);
    Screen { display }
}

// ============================================================================
// Bluetooth: advertise, serve GATT, take updates
// ============================================================================

#[gatt_server]
struct Server {
    dfu: LegacyDfuService,
}

/// The chip's factory address with its top two bits set, the same static random address the controller reports.
fn device_address() -> [u8; 6] {
    const FICR_DEVICE_ADDRESS_LOW: *const u32 = 0x1000_00A4 as *const u32;
    const FICR_DEVICE_ADDRESS_HIGH: *const u32 = 0x1000_00A8 as *const u32;
    let low = unsafe { FICR_DEVICE_ADDRESS_LOW.read_volatile() }.to_le_bytes();
    let high = unsafe { FICR_DEVICE_ADDRESS_HIGH.read_volatile() }.to_le_bytes();
    [low[0], low[1], low[2], low[3], high[0], high[1] | 0xC0]
}

async fn run_bluetooth(
    controller: neotime_bluetooth::BluetoothController<'static, neotime_bluetooth::NrfRadio>,
    targets: storage::Targets,
    mut confirmation: storage::Confirmation,
    mut screen: Screen,
) {
    let controller: ExternalController<_, 4> = ExternalController::new(controller);
    let resources = HOST_RESOURCES.init(HostResources::new());
    let stack = trouble_host::new(controller, resources)
        .set_random_address(Address::random(device_address()))
        .build();
    let mut runner = stack.runner();
    let mut peripheral = stack.peripheral();
    let server = Server::new_with_config(GapConfig::Peripheral(PeripheralConfig {
        name: DEVICE_NAME,
        appearance: &appearance::watch::GENERIC_WATCH,
    }))
    .unwrap();
    let mut dfu = LegacyDfu::new(targets);

    let mut advertisement_data = [0; 31];
    let advertisement_length = AdStructure::encode_slice(
        &[
            AdStructure::Flags(LE_GENERAL_DISCOVERABLE | BR_EDR_NOT_SUPPORTED),
            AdStructure::CompleteLocalName(DEVICE_NAME.as_bytes()),
        ],
        &mut advertisement_data,
    )
    .unwrap();

    let serve = async {
        let mut advertising_shown = false;
        let mut confirmed = false;
        loop {
            let advertiser = peripheral
                .advertise(
                    &AdvertisementParameters::default(),
                    Advertisement::ConnectableScannableUndirected {
                        adv_data: &advertisement_data[..advertisement_length],
                        scan_data: &[],
                    },
                )
                .await
                .unwrap();
            if !advertising_shown {
                screen.write_line(3, "advertising");
                advertising_shown = true;
            }

            let Ok(connection) = advertiser.accept().await else {
                continue;
            };
            let Ok(connection) = connection.with_attribute_server(&server) else {
                continue;
            };
            screen.write_line(4, "connected   ");
            serve_connection(
                &connection,
                &server,
                &mut dfu,
                &mut screen,
                &mut confirmation,
                &mut confirmed,
            )
            .await;
            screen.write_line(4, "disconnected");
        }
    };

    let _ = embassy_futures::join::join(runner.run(), serve).await;
}

/// The first data a phone exchanges proves the radio works end to end, so that is when the firmware confirms itself.
async fn serve_connection(
    connection: &GattConnection<'_, '_, DefaultPacketPool>,
    server: &Server<'_>,
    dfu: &mut LegacyDfu<storage::Targets>,
    screen: &mut Screen,
    confirmation: &mut storage::Confirmation,
    confirmed: &mut bool,
) {
    loop {
        match connection.next().await {
            GattConnectionEvent::Disconnected { .. } => {
                dfu.abort();
                return;
            }
            GattConnectionEvent::Gatt { event } => {
                let mut written = heapless::Vec::<u8, 64>::new();
                let target = match &event {
                    GattEvent::Write(write) => {
                        write.with_data(|_, bytes| {
                            let _ = written.extend_from_slice(&bytes[..bytes.len().min(64)]);
                        });
                        server.dfu.classify(write.handle())
                    }
                    _ => None,
                };
                if let Ok(reply) = event.accept() {
                    reply.send().await;
                }
                if !*confirmed {
                    let status = confirmation.confirm().await;
                    screen.write_line(3, status);
                    *confirmed = true;
                }

                let outcome = match target {
                    Some(DfuWrite::ControlPoint) => dfu.on_control_point(&written).await,
                    Some(DfuWrite::Packet) => dfu.on_packet(&written).await,
                    None => continue,
                };
                for notification in &outcome.notifications {
                    let _ = server
                        .dfu
                        .control_point
                        .notify_raw(connection, notification, false)
                        .await;
                }
                if dfu.is_updating() {
                    let mut progress = heapless::String::<20>::new();
                    let _ = core::fmt::write(
                        &mut progress,
                        format_args!("update {:>3} KB  ", dfu.bytes_received() / 1024),
                    );
                    screen.write_line(5, &progress);
                }
                if outcome.reset_requested {
                    if storage::RESET_AFTER_ACTIVATE {
                        Timer::after(Duration::from_millis(200)).await;
                        cortex_m::peripheral::SCB::sys_reset();
                    }
                    screen.write_line(5, "upload verified ");
                }
            }
            _ => {}
        }
    }
}
