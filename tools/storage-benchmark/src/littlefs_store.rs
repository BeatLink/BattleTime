//! What NeoTime does today, for comparison: each image a littlefs file, read a chunk at a time by its offset.

use std::collections::HashMap;

use neotime_storage::{prepare_file_system, read_part, FileSystem, FileSystemFlash};

use crate::{
    flash::SimulatedFlash,
    stores::Store,
    workload::{kind_of, AppKeys, Key, Kind},
};

/// Where a key's bytes live: a file, and the offset and length inside it.
struct Place {
    path: String,
    offset: u32,
    length: u32,
}

pub struct LittlefsStore {
    file_system: FileSystem<SimulatedFlash>,
    /// Each app's file and image, by app number, written whole when its index is written.
    images: HashMap<u32, (String, Vec<u8>)>,
    places: HashMap<Key, Place>,
}

impl LittlefsStore {
    /// Mounts littlefs with the firmware's own settings in its 3 MB region, which starts 1 MB into the chip.
    pub fn format(flash: SimulatedFlash, apps: &[AppKeys]) -> LittlefsStore {
        let prepared = prepare_file_system(FileSystemFlash::new(flash)).unwrap_or_else(|failed| panic!("littlefs: {:?}", failed.error));
        let file_system = prepared.file_system;
        for folder in ["/steps", "/notifications"] {
            file_system.mkdir(folder).expect("littlefs mkdir");
        }
        let mut images = HashMap::new();
        let mut places = HashMap::new();
        for app in apps {
            let number = app.read_at_open[0].0 >> 20;
            let path = format!("/apps/{}.ntapp", app.name);
            // The parts read at open come straight after the header and section table, about as the sections lie.
            let mut offset = 0;
            for (key, value) in &app.read_at_open {
                places.insert(*key, Place { path: path.clone(), offset, length: value.len() as u32 });
                offset += value.len() as u32;
            }
            for ((key, _), (offset, length)) in app.read_on_demand.iter().zip(&app.offsets_in_image) {
                places.insert(*key, Place { path: path.clone(), offset: *offset, length: *length });
            }
            places.insert(app.saved_state.0, Place { path: format!("/apps/{}.state", app.name), offset: 0, length: u32::MAX });
            images.insert(number, (path, app.image.clone()));
        }
        LittlefsStore { file_system, images, places }
    }

    /// The file a value that is not part of an image is kept in.
    fn path_of(&self, key: Key) -> String {
        let item = key & 0xfff;
        match kind_of(key) {
            kind if kind == Kind::Settings as u32 => format!("/settings/{item}"),
            kind if kind == Kind::Steps as u32 => format!("/steps/{item}"),
            kind if kind == Kind::Notification as u32 => format!("/notifications/{item}"),
            _ => self.places[&key].path.clone(),
        }
    }
}

impl Store for LittlefsStore {
    async fn write_all(&mut self, values: &[(Key, Vec<u8>)]) {
        for (key, value) in values {
            let kind = kind_of(*key);
            if kind == Kind::Index as u32 {
                let (path, image) = &self.images[&(key >> 20)];
                self.file_system.write_file(path, image).expect("littlefs write image");
            } else if kind == Kind::SavedState as u32 || kind >= Kind::Settings as u32 {
                self.file_system.write_file(&self.path_of(*key), value).expect("littlefs write");
            }
            // Every other part of an app is inside its image, written with the index.
        }
    }

    async fn read(&mut self, key: Key, buffer: &mut [u8]) -> Option<usize> {
        let path = self.path_of(key);
        let (offset, length) = match self.places.get(&key) {
            Some(place) => (place.offset, place.length.min(buffer.len() as u32)),
            None => (0, buffer.len() as u32),
        };
        read_part(&self.file_system, &path, offset, &mut buffer[..length as usize])
    }
}
