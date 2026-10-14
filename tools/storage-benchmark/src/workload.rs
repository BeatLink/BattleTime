//! Real compiled apps split into keys, and the system data that lives beside them.

use std::path::Path;

use neotime_runtime::image::AppImage;

// ============================================================================
// Keys
// ============================================================================

/// A key packs the app's number (12 bits), its install version (4), what the value holds (4) and which one (12).
pub type Key = u32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Kind {
    Index = 0,
    Expressions = 1,
    Layout = 2,
    StateLayout = 3,
    Subscriptions = 4,
    InitialValues = 5,
    Function = 6,
    Handler = 7,
    Text = 8,
    Resource = 9,
    SavedState = 10,
    Settings = 11,
    Steps = 12,
    Notification = 13,
    WholeImage = 14,
}

/// The app number the watch's own data is kept under.
pub const SYSTEM: u32 = 0xfff;
pub const INSTALL_VERSION: u32 = 1;

pub fn key(app: u32, kind: Kind, item: u32) -> Key {
    assert!(app <= 0xfff && item <= 0xfff, "app {app} or item {item} out of range");
    app << 20 | INSTALL_VERSION << 16 | (kind as u32) << 12 | item
}

pub fn kind_of(key: Key) -> u32 {
    key >> 12 & 0xf
}

// ============================================================================
// One app as keys
// ============================================================================

pub struct AppKeys {
    pub name: String,
    pub image_bytes: usize,
    /// What the engine reads when the app opens: the index and the parts every redraw uses.
    pub read_at_open: Vec<(Key, Vec<u8>)>,
    /// What the engine reads only when it is needed: functions, handlers, strings and pictures.
    pub read_on_demand: Vec<(Key, Vec<u8>)>,
    /// Where each of `read_on_demand` lies in the image, as (offset, length), for reading it from a file.
    pub offsets_in_image: Vec<(u32, u32)>,
    pub image: Vec<u8>,
    pub saved_state: (Key, Vec<u8>),
}

impl AppKeys {
    /// Every value an install writes, the index last so an app exists only once all of it is there.
    pub fn install_values(&self) -> Vec<(Key, Vec<u8>)> {
        let mut values: Vec<_> = self.read_at_open.iter().skip(1).cloned().collect();
        values.extend(self.read_on_demand.iter().cloned());
        values.push(self.saved_state.clone());
        values.extend(self.read_at_open.first().cloned());
        values
    }
}

pub fn split_image(name: &str, number: u32, bytes: &[u8]) -> AppKeys {
    let image = AppImage::parse(bytes).unwrap_or_else(|error| panic!("{name}: {error:?}"));
    let section_count = u16::from_le_bytes([bytes[10], bytes[11]]) as usize;

    let mut read_at_open = vec![(key(number, Kind::Index, 0), bytes[..20 + 12 * section_count].to_vec())];
    let mut add_whole = |kind: Kind, value: Vec<u8>| {
        if !value.is_empty() {
            read_at_open.push((key(number, kind, 0), value));
        }
    };
    add_whole(
        Kind::Expressions,
        (0..image.expression_count()).flat_map(|index| image.expression_code(index).unwrap().to_vec()).collect(),
    );
    add_whole(
        Kind::Layout,
        (0..image.widget_count()).flat_map(|index| image.widget_bytes(index).unwrap().to_vec()).collect(),
    );
    add_whole(Kind::StateLayout, vec![0; 16 * image.state_count() as usize]);
    add_whole(Kind::Subscriptions, vec![0; 16 * image.subscription_count() as usize]);
    add_whole(
        Kind::InitialValues,
        (0..image.initial_values_count()).flat_map(|index| image.initial_values(index).unwrap().1.to_vec()).collect(),
    );

    let mut read_on_demand = Vec::new();
    let mut offsets_in_image = Vec::new();
    // A chunk's place in the file is where its bytes start, less any header the key's value adds in front.
    let mut add_chunk = |key: Key, value: Vec<u8>, slice: &[u8], header_bytes: usize| {
        let offset = slice.as_ptr() as usize - bytes.as_ptr() as usize - header_bytes;
        offsets_in_image.push((offset as u32, value.len() as u32));
        read_on_demand.push((key, value));
    };
    for index in 0..image.function_count() {
        let function = image.function(index).unwrap();
        let mut value = vec![function.argument_count, function.result_count, function.local_count];
        value.extend_from_slice(function.code);
        add_chunk(key(number, Kind::Function, index as u32), value, function.code, 3);
    }
    for index in 0..image.handler_count() {
        let code = image.handler_code(index).unwrap();
        add_chunk(key(number, Kind::Handler, index as u32), code.to_vec(), code, 0);
    }
    for index in 0..image.string_count() {
        let text = image.string(index).unwrap().as_bytes();
        add_chunk(key(number, Kind::Text, index as u32), text.to_vec(), text, 0);
    }
    for index in 0..image.resource_count() {
        let resource = image.resource_bytes(index).unwrap();
        add_chunk(key(number, Kind::Resource, index as u32), resource.to_vec(), resource, 0);
    }

    let persisted: Vec<_> = (0..image.state_count())
        .filter_map(|index| image.state_variable(index))
        .filter(|variable| variable.persists)
        .collect();
    // The saved form: a 6-byte header, then for each variable 8 bytes of key and type before its value.
    let saved_bytes = 6 + persisted.iter().map(|variable| 8 + variable.stored_bytes()).sum::<usize>();
    let saved_state = (key(number, Kind::SavedState, 0), vec![0x5a; saved_bytes]);

    AppKeys {
        name: name.to_string(),
        image_bytes: bytes.len(),
        read_at_open,
        read_on_demand,
        offsets_in_image,
        image: bytes.to_vec(),
        saved_state,
    }
}

/// Images are stored in parts of this size, so each fits in one 4 KB page of sequential-storage.
const IMAGE_PART_BYTES: usize = 3072;

/// How the apps are kept in the store.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// The index, plus a key for each function, handler, string and picture.
    Split,
    /// The index, plus the whole image in 3 KB parts, all read when the app opens.
    WholeImages,
    /// Only each app's saved state; the images live elsewhere.
    SavedStateOnly,
    /// Each app as one value with its index inside, plus one value for its state; the watch's settings and
    /// its kept notifications each one value, edited in RAM and written back whole.
    Grouped,
}

fn whole_image(name: &str, number: u32, bytes: &[u8]) -> AppKeys {
    let mut app = split_image(name, number, bytes);
    let index = app.read_at_open.swap_remove(0);
    app.read_at_open = vec![index];
    for (part, chunk) in bytes.chunks(IMAGE_PART_BYTES).enumerate() {
        app.read_at_open.push((key(number, Kind::WholeImage, part as u32), chunk.to_vec()));
    }
    app.read_on_demand.clear();
    app.offsets_in_image.clear();
    app
}

fn grouped(name: &str, number: u32, bytes: &[u8]) -> AppKeys {
    let mut app = split_image(name, number, bytes);
    app.read_at_open = vec![(key(number, Kind::WholeImage, 0), bytes.to_vec())];
    app.read_on_demand.clear();
    app.offsets_in_image.clear();
    app
}

fn saved_state_only(name: &str, number: u32, bytes: &[u8]) -> AppKeys {
    let mut app = split_image(name, number, bytes);
    app.read_at_open.clear();
    app.read_on_demand.clear();
    app.offsets_in_image.clear();
    app
}

pub fn load_apps(folder: &Path, layout: Layout) -> Vec<AppKeys> {
    let mut paths: Vec<_> = std::fs::read_dir(folder)
        .unwrap_or_else(|error| panic!("{}: {error}", folder.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "ntapp"))
        .collect();
    paths.sort();
    paths
        .iter()
        .enumerate()
        .map(|(number, path)| {
            let name = path.file_stem().unwrap().to_string_lossy();
            let bytes = std::fs::read(path).unwrap();
            match layout {
                Layout::Split => split_image(&name, number as u32 + 1, &bytes),
                Layout::WholeImages => whole_image(&name, number as u32 + 1, &bytes),
                Layout::SavedStateOnly => saved_state_only(&name, number as u32 + 1, &bytes),
                Layout::Grouped => grouped(&name, number as u32 + 1, &bytes),
            }
        })
        .collect()
}

// ============================================================================
// The watch's own data
// ============================================================================

/// The watch-wide settings (41 bytes) and the theme (16 bytes), as two values or grouped as one.
pub fn settings_values(layout: Layout) -> Vec<(Key, Vec<u8>)> {
    if layout == Layout::Grouped {
        return vec![(key(SYSTEM, Kind::Settings, 0), vec![1; 41 + 16 + 8])];
    }
    vec![(key(SYSTEM, Kind::Settings, 0), vec![1; 41]), (key(SYSTEM, Kind::Settings, 1), vec![2; 16])]
}

/// One day's step log: 240 six-minute slots of 2 bytes, filled up to `slots_logged`.
pub fn step_day(day: u32, slots_logged: usize) -> (Key, Vec<u8>) {
    (key(SYSTEM, Kind::Steps, day % 4096), vec![0x33; 2 * slots_logged.max(1)])
}

/// A kept notification of about 200 bytes in one of the 5 places the watch keeps, or grouped, the whole ring of 5.
pub fn notification(number: u32, layout: Layout) -> (Key, Vec<u8>) {
    if layout == Layout::Grouped {
        return (key(SYSTEM, Kind::Notification, 0), vec![0x44; 5 * 200 + 8]);
    }
    (key(SYSTEM, Kind::Notification, number % 5), vec![0x44; 200])
}

// ============================================================================
// A repeatable shuffle, so every store reads in the same order
// ============================================================================

pub struct Shuffle(u64);

impl Shuffle {
    pub fn new(seed: u64) -> Shuffle {
        Shuffle(seed | 1)
    }

    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for index in (1..items.len()).rev() {
            items.swap(index, (self.next() % (index as u64 + 1)) as usize);
        }
    }
}
