// Reads the header of a compiled NeoTime image: its kind, id, name and launcher group, without checking its code.
// The layout is neotime/runtime/src/image.rs; src/protocol/app-image.ts reads the same.

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageKind {
    App,
    Face,
    Library,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageHeader {
    pub kind: ImageKind,
    pub id: String,
    pub name: String,
    // The launcher group the app declares, or empty when it declares none.
    pub group: String,
    pub has_icon: bool,
    pub bytes: usize,
}

const MAGIC: &[u8; 4] = b"NTAP";
const FORMAT_VERSION: u16 = 2;
const HEADER_BYTES: usize = 20;
const SECTION_ENTRY_BYTES: usize = 12;
const TABLE_ENTRY_BYTES: usize = 4;
const STRINGS_SECTION: u16 = 1;
const LAUNCHER_SECTION: u16 = 11;

fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

pub fn read_image_header(bytes: &[u8]) -> Result<ImageHeader, String> {
    if bytes.len() < HEADER_BYTES || &bytes[..4] != MAGIC {
        return Err("That file is not a NeoTime app image.".into());
    }
    if u16_at(bytes, 4) != Some(FORMAT_VERSION) {
        return Err("That image was made for another version of NeoTime.".into());
    }
    let kind = match bytes[6] {
        0 => ImageKind::App,
        1 => ImageKind::Face,
        2 => ImageKind::Library,
        _ => return Err("That image is of a kind this app does not know.".into()),
    };
    let sections = u16_at(bytes, 10).unwrap_or(0) as usize;
    let id_string = u16_at(bytes, 16).unwrap_or(0);
    let name_string = u16_at(bytes, 18).unwrap_or(0);

    let mut strings: Option<(&[u8], u16)> = None;
    let mut group_string = None;
    for index in 0..sections {
        let entry = HEADER_BYTES + index * SECTION_ENTRY_BYTES;
        let (Some(number), Some(count), Some(offset), Some(length)) = (
            u16_at(bytes, entry),
            u16_at(bytes, entry + 2),
            u32_at(bytes, entry + 4),
            u32_at(bytes, entry + 8),
        ) else {
            break;
        };
        let (offset, length) = (offset as usize, length as usize);
        if number == STRINGS_SECTION {
            let section = bytes
                .get(offset..offset + length)
                .ok_or("That image is cut short.")?;
            strings = Some((section, count));
        } else if number == LAUNCHER_SECTION && length >= 2 {
            group_string = u16_at(bytes, offset);
        }
    }
    let (section, count) = strings.ok_or("That image has no strings, so it names no app.")?;
    let string = |which| read_string(section, count, which);
    Ok(ImageHeader {
        kind,
        id: string(id_string)?,
        name: string(name_string)?,
        group: match group_string {
            Some(which) => string(which)?,
            None => String::new(),
        },
        has_icon: bytes[7] & 1 != 0,
        bytes: bytes.len(),
    })
}

fn read_string(section: &[u8], count: u16, index: u16) -> Result<String, String> {
    if index >= count {
        return Err("That image names a string it does not have.".into());
    }
    let table = index as usize * TABLE_ENTRY_BYTES;
    let start = u16_at(section, table).unwrap_or(0) as usize;
    let length = u16_at(section, table + 2).unwrap_or(0) as usize;
    let data = &section[(count as usize * TABLE_ENTRY_BYTES).min(section.len())..];
    let text = data
        .get(start..start + length)
        .ok_or("That image is cut short.")?;
    Ok(String::from_utf8_lossy(text).into_owned())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    // A header, a strings section holding `strings` and, with `group`, a launcher section naming that string.
    pub fn image(kind: u8, strings: &[&str], group: Option<u16>) -> Vec<u8> {
        let sections = if group.is_some() { 2 } else { 1 };
        let mut bytes = Vec::new();
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        bytes.extend_from_slice(&[kind, 1, 0, 0]);
        bytes.extend_from_slice(&(sections as u16).to_le_bytes());
        bytes.extend_from_slice(&[0, 0, 0, 0]);
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());

        let mut table = Vec::new();
        let mut data = Vec::new();
        for text in strings {
            table.extend_from_slice(&(data.len() as u16).to_le_bytes());
            table.extend_from_slice(&(text.len() as u16).to_le_bytes());
            data.extend_from_slice(text.as_bytes());
        }
        let strings_offset = HEADER_BYTES + sections * SECTION_ENTRY_BYTES;
        let strings_length = table.len() + data.len();
        let mut entries = Vec::new();
        entries.extend_from_slice(&STRINGS_SECTION.to_le_bytes());
        entries.extend_from_slice(&(strings.len() as u16).to_le_bytes());
        entries.extend_from_slice(&(strings_offset as u32).to_le_bytes());
        entries.extend_from_slice(&(strings_length as u32).to_le_bytes());
        if group.is_some() {
            entries.extend_from_slice(&LAUNCHER_SECTION.to_le_bytes());
            entries.extend_from_slice(&0u16.to_le_bytes());
            entries.extend_from_slice(&((strings_offset + strings_length) as u32).to_le_bytes());
            entries.extend_from_slice(&2u32.to_le_bytes());
        }
        bytes.extend(entries);
        bytes.extend(table);
        bytes.extend(data);
        if let Some(group) = group {
            bytes.extend_from_slice(&group.to_le_bytes());
        }
        bytes
    }

    #[test]
    fn a_header_names_its_app_and_group() {
        let header = read_image_header(&image(0, &["buzz-back", "Buzz back", "tools"], Some(2))).unwrap();
        assert_eq!(header.kind, ImageKind::App);
        assert_eq!(
            (header.id.as_str(), header.name.as_str(), header.group.as_str()),
            ("buzz-back", "Buzz back", "tools")
        );
        assert!(header.has_icon);
    }

    #[test]
    fn a_face_without_a_group_has_an_empty_one() {
        let header = read_image_header(&image(1, &["digital", "Digital"], None)).unwrap();
        assert_eq!(header.kind, ImageKind::Face);
        assert_eq!(header.group, "");
    }

    #[test]
    fn bad_images_say_what_is_wrong() {
        assert!(read_image_header(b"short")
            .unwrap_err()
            .contains("not a NeoTime app image"));
        let mut old = image(0, &["a", "A"], None);
        old[4] = 1;
        assert!(read_image_header(&old).unwrap_err().contains("another version"));
        let mut strange = image(0, &["a", "A"], None);
        strange[6] = 9;
        assert!(read_image_header(&strange).unwrap_err().contains("kind"));
        assert!(read_image_header(&image(0, &["a"], None))
            .unwrap_err()
            .contains("does not have"));
        let mut cut = image(0, &["a", "A"], None);
        cut.truncate(cut.len() - 1);
        assert!(read_image_header(&cut).unwrap_err().contains("cut short"));
    }
}
