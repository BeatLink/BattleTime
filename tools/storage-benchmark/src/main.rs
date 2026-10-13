//! Compares ekv and sequential-storage as the one store for NeoTime's apps split into keys, steps, state and settings.
//!
//! Each scenario formats a simulated 3 MB flash, installs every app, writes a year of step logs, runs some days of
//! everyday writes, then opens every app and reads each of its chunks twice. See docs/architecture-assessment.md.

mod custom_store;
mod flash;
mod littlefs_store;
mod power_cut;
mod stores;
mod workload;

use std::path::PathBuf;

use embassy_futures::block_on;
use sequential_storage::cache::{
    key_pointers::{ArrayKeyPointers, HeapKeyPointers},
    page_pointers::HeapPagePointers,
    page_states::HeapPageStates,
    Cache, Uncached,
};

use flash::{FlashCounters, SharedCounters, SimulatedFlash, Timing, SECTOR_BYTES};
use stores::{EkvStore, SequentialStore, Store};
use workload::{AppKeys, Key, Kind, Layout, Shuffle};

// ============================================================================
// Options
// ============================================================================

struct Options {
    apps_folder: PathBuf,
    flash_sectors: usize,
    days_of_use: u32,
    batch_minutes: u32,
    timing: Timing,
    layout: Layout,
    run_ekv: bool,
    run_sequential: bool,
    run_littlefs: bool,
    run_custom: bool,
}

fn read_options() -> Options {
    let mut options = Options {
        apps_folder: PathBuf::new(),
        flash_sectors: 768,
        days_of_use: 30,
        batch_minutes: 60,
        timing: Timing::ASSUMED,
        layout: Layout::Split,
        run_ekv: true,
        run_sequential: true,
        run_littlefs: false,
        run_custom: false,
    };
    let mut arguments = std::env::args().skip(1);
    while let Some(argument) = arguments.next() {
        let mut number = || -> f64 {
            let text = arguments.next().unwrap_or_else(|| panic!("{argument} needs a number"));
            text.parse().unwrap_or_else(|_| panic!("{argument}: {text} is not a number"))
        };
        match argument.as_str() {
            "--flash-kib" => options.flash_sectors = number() as usize / 4,
            "--days" => options.days_of_use = number() as u32,
            "--batch-minutes" => options.batch_minutes = number() as u32,
            "--erase-ms" => options.timing.sector_erase = number() * 1000.0,
            "--program-us" => options.timing.page_program = number(),
            "--whole-images" => options.layout = Layout::WholeImages,
            "--saved-state-only" => options.layout = Layout::SavedStateOnly,
            "--grouped" => options.layout = Layout::Grouped,
            "--only-ekv" => options.run_sequential = false,
            "--only-sequential" => options.run_ekv = false,
            "--only-custom" => {
                (options.run_ekv, options.run_sequential, options.run_custom) = (false, false, true);
            }
            "--power-cut-test" => {
                power_cut::run();
                std::process::exit(0);
            }
            "--only-littlefs" => {
                (options.run_ekv, options.run_sequential, options.run_littlefs) = (false, false, true);
            }
            "--help" => {
                println!("usage: neotime-storage-benchmark APPS_FOLDER [--flash-kib 3072] [--days 30] [--batch-minutes 60] [--erase-ms 50] [--program-us 600] [--whole-images | --saved-state-only | --grouped] [--only-ekv | --only-sequential | --only-littlefs | --only-custom] [--power-cut-test]");
                std::process::exit(0);
            }
            folder => options.apps_folder = PathBuf::from(folder),
        }
    }
    if options.apps_folder.as_os_str().is_empty() {
        panic!("give the folder of compiled .ntapp images; tools/storage_benchmark.sh builds it");
    }
    options
}

// ============================================================================
// What one scenario measured
// ============================================================================

#[derive(Default)]
struct Results {
    install: FlashCounters,
    year_of_steps: FlashCounters,
    days_of_use: FlashCounters,
    /// How long each write during the days of use took.
    write_microseconds: Vec<f64>,
    open_microseconds: Vec<f64>,
    cold_reads: Vec<FlashCounters>,
    warm_reads: Vec<FlashCounters>,
    ram_bytes: usize,
    /// For the custom store, the most erases of one sector in its apps, groups and step log.
    most_erases: Option<(u32, u32, u32)>,
}

/// Runs one write and keeps how long it took, since a read that arrives meanwhile waits that long.
async fn timed_write(store: &mut impl Store, counters: &SharedCounters, values: &[(Key, Vec<u8>)], results: &mut Results) {
    let before = counters.get();
    store.write_all(values).await;
    results.write_microseconds.push(counters.get().since(&before).busy_microseconds);
}

// ============================================================================
// The scenario
// ============================================================================

async fn run_scenario(
    store: &mut impl Store,
    counters: &SharedCounters,
    apps: &[AppKeys],
    options: &Options,
    batch_minutes: Option<u32>,
) -> Results {
    let mut results = Results::default();
    let mut buffer = vec![0u8; 64 * 1024];

    let start = counters.get();
    store.write_all(&workload::settings_values(options.layout)).await;
    for app in apps {
        store.write_all(&app.install_values()).await;
    }
    results.install = counters.get().since(&start);

    let start = counters.get();
    for day in 0..365 {
        store.write_all(&[workload::step_day(day, 240)]).await;
    }
    results.year_of_steps = counters.get().since(&start);

    // Each day: a step slot every 6 minutes, 30 saved states and 20 notifications, spread over the day.
    let start = counters.get();
    let mut shuffle = Shuffle::new(2026);
    let mut notification_number = 0;
    for day in 365..365 + options.days_of_use {
        let mut events: Vec<(u32, (Key, Vec<u8>))> =
            (0..240).map(|slot| (slot * 6, workload::step_day(day, slot as usize + 1))).collect();
        for _ in 0..30 {
            let app = &apps[(shuffle.next() % apps.len() as u64) as usize];
            events.push(((shuffle.next() % 1440) as u32, app.saved_state.clone()));
        }
        for _ in 0..20 {
            notification_number += 1;
            events.push(((shuffle.next() % 1440) as u32, workload::notification(notification_number, options.layout)));
        }
        events.sort_by_key(|(minute, _)| *minute);

        let mut waiting: Vec<(Key, Vec<u8>)> = Vec::new();
        let mut last_flush = 0;
        for (minute, value) in events {
            match batch_minutes {
                None => timed_write(store, counters, &[value], &mut results).await,
                Some(batch) => {
                    if minute / batch != last_flush / batch && !waiting.is_empty() {
                        timed_write(store, counters, &waiting, &mut results).await;
                        waiting.clear();
                    }
                    last_flush = minute;
                    waiting.push(value);
                }
            }
        }
        if !waiting.is_empty() {
            timed_write(store, counters, &waiting, &mut results).await;
        }
    }
    results.days_of_use = counters.get().since(&start);

    // Open every app, then read each of its chunks once cold and once again, in a shuffled order.
    let mut order: Vec<&AppKeys> = apps.iter().collect();
    shuffle.shuffle(&mut order);
    for app in order {
        let before_open = counters.get();
        for (key, value) in app.read_at_open.iter().chain([&app.saved_state]) {
            let length = store.read(*key, &mut buffer).await.expect("value read at open");
            assert_eq!(length, value.len(), "{}: key {key:x}", app.name);
        }
        results.open_microseconds.push(counters.get().since(&before_open).busy_microseconds);

        let mut chunks: Vec<&(Key, Vec<u8>)> = app.read_on_demand.iter().collect();
        shuffle.shuffle(&mut chunks);
        for pass in [&mut results.cold_reads, &mut results.warm_reads] {
            for (key, value) in &chunks {
                let before = counters.get();
                let length = store.read(*key, &mut buffer).await.expect("chunk read");
                assert_eq!(&buffer[..length], value.as_slice(), "{}: key {key:x}", app.name);
                pass.push(counters.get().since(&before));
            }
        }
    }
    results
}

// ============================================================================
// Running every store
// ============================================================================

fn new_flash(options: &Options) -> (SimulatedFlash, SharedCounters) {
    SimulatedFlash::new(options.flash_sectors, options.timing)
}

fn run_sequential<C: sequential_storage::cache::CacheImpl<Key>>(
    options: &Options,
    apps: &[AppKeys],
    cache: C,
    data_buffer_bytes: usize,
) -> Results {
    let (flash, counters) = new_flash(options);
    let cache_bytes = std::mem::size_of_val(&cache);
    let mut store = SequentialStore::new(flash, (options.flash_sectors * SECTOR_BYTES) as u32, cache, data_buffer_bytes);
    let mut results = block_on(run_scenario(&mut store, &counters, apps, options, None));
    results.ram_bytes = cache_bytes + data_buffer_bytes;
    results
}

fn run_ekv(options: &Options, apps: &[AppKeys], batch_minutes: Option<u32>) -> Results {
    let (flash, counters) = new_flash(options);
    let mut store = block_on(EkvStore::format(flash));
    let mut results = block_on(run_scenario(&mut store, &counters, apps, options, batch_minutes));
    // The database's own state, less the flash it owns; the stack it borrows during operations is not counted.
    results.ram_bytes = std::mem::size_of::<EkvStore>() - std::mem::size_of::<SimulatedFlash>();
    results
}

/// The custom store in the 3 MB file system region; only whole images, so the grouped layout.
fn run_custom(options: &Options, apps: &[AppKeys]) -> Results {
    let geometry = custom_store::Geometry::WATCH;
    let (flash, counters) = SimulatedFlash::new(geometry.sector_count(), options.timing);
    let mut store = block_on(custom_store::CustomStore::mount(flash, geometry));
    let mut results = block_on(run_scenario(&mut store, &counters, apps, options, None));
    results.ram_bytes = store.ram_bytes();
    let flash = store.flash();
    results.most_erases = Some((
        flash.most_erases_of_one_sector(geometry.file_range()),
        flash.most_erases_of_one_sector(geometry.record_range()),
        flash.most_erases_of_one_sector(geometry.log_range()),
    ));
    results
}

/// littlefs needs the whole 4 MB chip, since the firmware's file system region starts 1 MB in.
fn run_littlefs(options: &Options, apps: &[AppKeys]) -> Results {
    let (flash, counters) = SimulatedFlash::new(1024, options.timing);
    let mut store = littlefs_store::LittlefsStore::format(flash, apps);
    let mut results = block_on(run_scenario(&mut store, &counters, apps, options, None));
    // From docs/memory-map.md: 944 B while mounted and 408 B for the open image file.
    results.ram_bytes = 944 + 408;
    results
}

// ============================================================================
// Report
// ============================================================================

fn percentile(values: &mut [f64], fraction: f64) -> f64 {
    values.sort_by(f64::total_cmp);
    values[((values.len() - 1) as f64 * fraction).round() as usize]
}

fn milliseconds(microseconds: f64) -> String {
    format!("{:.1} ms", microseconds / 1000.0)
}

fn describe_reads(reads: &[FlashCounters]) -> String {
    if reads.is_empty() {
        return "none: every chunk came with the app".to_string();
    }
    let mut times: Vec<f64> = reads.iter().map(|read| read.busy_microseconds).collect();
    let flash_reads = reads.iter().map(|read| read.reads).sum::<u64>() as f64 / reads.len() as f64;
    let bytes = reads.iter().map(|read| read.read_bytes).sum::<u64>() as f64 / reads.len() as f64;
    format!(
        "{:>6.0} {:>6.0} {:>7.0} {:>5.1} {:>6.0}",
        percentile(&mut times, 0.5),
        percentile(&mut times, 0.99),
        percentile(&mut times, 1.0),
        flash_reads,
        bytes
    )
}

fn print_workload(apps: &[AppKeys]) {
    let all: Vec<&(Key, Vec<u8>)> =
        apps.iter().flat_map(|app| app.read_at_open.iter().chain(&app.read_on_demand)).collect();
    let bytes: usize = all.iter().map(|(_, value)| value.len()).sum();
    let largest = all.iter().map(|(_, value)| value.len()).max().unwrap_or(0);
    println!("## Workload\n");
    println!("{} apps and faces, {} B of images, stored as {} keys holding {} B; the largest value is {} B.", apps.len(), apps.iter().map(|app| app.image_bytes).sum::<usize>(), all.len(), bytes, largest);
    for (kind, label) in [
        (Kind::Function, "functions"),
        (Kind::Handler, "handlers"),
        (Kind::Text, "strings"),
        (Kind::Resource, "pictures"),
    ] {
        let sizes: Vec<usize> = all
            .iter()
            .filter(|(key, _)| workload::kind_of(*key) == kind as u32)
            .map(|(_, value)| value.len())
            .collect();
        if !sizes.is_empty() {
            println!("- {label}: {} keys, {} B on average", sizes.len(), sizes.iter().sum::<usize>() / sizes.len());
        }
    }
    println!();
}

fn print_results(rows: &mut [(String, Results)], options: &Options) {
    println!("## Writing\n");
    println!("| Store | Install all: erases, time | Year of steps: erases | {} days of use: erases a day, busy a day | One write: median, p99, slowest; writes over 1 s |", options.days_of_use);
    println!("|---|---|---|---|---|");
    for (name, results) in rows.iter_mut() {
        let over_a_second = results.write_microseconds.iter().filter(|took| **took > 1_000_000.0).count();
        let writes = &mut results.write_microseconds;
        let (median, p99, slowest) = (percentile(writes, 0.5), percentile(writes, 0.99), percentile(writes, 1.0));
        println!(
            "| {name} | {}, {} | {} | {:.0}, {} | {}, {}, {}; {over_a_second} of {} |",
            results.install.erases,
            milliseconds(results.install.busy_microseconds),
            results.year_of_steps.erases,
            results.days_of_use.erases as f64 / options.days_of_use as f64,
            milliseconds(results.days_of_use.busy_microseconds / options.days_of_use as f64),
            milliseconds(median),
            milliseconds(p99),
            milliseconds(slowest),
            writes.len(),
        );
    }
    println!("\n## Reading\n");
    println!("Times are microseconds of bus and chip time. Cold is each chunk's first read after the app opens, warm its second.\n");
    println!("| Store | RAM | Open an app: median, slowest | Cold: median, p99, slowest µs, flash reads, bytes read | Warm: same |");
    println!("|---|---:|---|---|---|");
    for (name, results) in rows.iter_mut() {
        let open_median = percentile(&mut results.open_microseconds, 0.5);
        let open_slowest = percentile(&mut results.open_microseconds, 1.0);
        println!(
            "| {name} | {} B | {}, {} | `{}` | `{}` |",
            results.ram_bytes,
            milliseconds(open_median),
            milliseconds(open_slowest),
            describe_reads(&results.cold_reads),
            describe_reads(&results.warm_reads),
        );
    }
    for (name, results) in rows.iter() {
        if let Some((apps, groups, steps)) = results.most_erases {
            println!("\n{name}: most erases of one sector after {} days: apps {apps}, groups {groups}, steps {steps}.", options.days_of_use);
        }
    }
    println!(
        "\nTiming assumed: {} µs a command, {} µs a byte, {} µs a program page, {} ms a sector erase.",
        options.timing.command,
        options.timing.byte,
        options.timing.page_program,
        options.timing.sector_erase / 1000.0
    );
}

fn main() {
    let options = read_options();
    let apps = workload::load_apps(&options.apps_folder, options.layout);
    // Every key the run writes: the apps', the two settings, a step log per day and the 5 notifications.
    let key_count: usize = apps.iter().map(|app| app.read_at_open.len() + app.read_on_demand.len() + 1).sum::<usize>()
        + 2
        + 365
        + options.days_of_use as usize
        + 5;
    let pages = options.flash_sectors;
    print_workload(&apps);

    // sequential-storage's buffer holds the largest key and value, rounded up to a flash word.
    let largest_value = apps
        .iter()
        .flat_map(|app| app.read_at_open.iter().chain(&app.read_on_demand).chain([&app.saved_state]))
        .map(|(_, value)| value.len())
        .max()
        .unwrap_or(0)
        .max(480);
    let buffer_bytes = (largest_value + 4 + 8).next_multiple_of(4);
    let page_state_bytes = pages;
    let page_pointer_bytes = 8 * pages;
    let mut rows = Vec::new();
    if options.run_ekv {
        let chunk = option_env!("EKV_MAX_CHUNK_SIZE").unwrap_or("4096");
        rows.push((format!("ekv {chunk} B chunks, writes batched every {} min", options.batch_minutes), run_ekv(&options, &apps, Some(options.batch_minutes))));
        rows.push((format!("ekv {chunk} B chunks, each write alone"), run_ekv(&options, &apps, None)));
    }
    if options.run_sequential {
        rows.push((
            "sequential-storage, no cache".to_string(),
            run_sequential(&options, &apps, Cache::<Uncached, Uncached, Uncached, Key>::new_uncached(), buffer_bytes),
        ));
        let mut row = run_sequential(&options, &apps, Cache::new(HeapPageStates::new(pages), Uncached, Uncached), buffer_bytes);
        row.ram_bytes += page_state_bytes;
        rows.push(("sequential-storage, page states".to_string(), row));
        let mut row = run_sequential(
            &options,
            &apps,
            Cache::new(HeapPageStates::new(pages), HeapPagePointers::new(pages), ArrayKeyPointers::<Key, 64>::new()),
            buffer_bytes,
        );
        row.ram_bytes += page_state_bytes + page_pointer_bytes;
        rows.push(("sequential-storage, page caches + 64 keys".to_string(), row));
        let mut row = run_sequential(
            &options,
            &apps,
            Cache::new(HeapPageStates::new(pages), HeapPagePointers::new(pages), HeapKeyPointers::new(key_count)),
            buffer_bytes,
        );
        // The heap caches' contents live behind a pointer, so they are counted by their sizes.
        row.ram_bytes += page_state_bytes + page_pointer_bytes + 8 * key_count;
        rows.push(("sequential-storage, page caches + every key".to_string(), row));
    }
    if options.run_custom {
        assert!(options.layout == Layout::Grouped, "the custom store keeps whole images, so it needs --grouped");
        rows.push(("custom: files, records and logs".to_string(), run_custom(&options, &apps)));
    }
    if options.run_littlefs {
        assert!(options.layout == Layout::Split, "littlefs reads chunks from whole image files, so it needs the split layout");
        rows.push(("littlefs, a chunk read from its image file".to_string(), run_littlefs(&options, &apps)));
    }
    print_results(&mut rows, &options);
}
