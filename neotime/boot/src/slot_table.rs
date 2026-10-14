//! Where the update and recovery slots sit in external flash, kept in two pages beside the bootloader.
//!
//! Each change is a new 32-byte record appended after the last, with a sequence number and a CRC, so
//! nothing is erased until a page fills. A full page moves the log to the other page, which is erased
//! first; the newest record stays readable in the old page until the new one is whole, so a power cut
//! at any point leaves either the old slots or the new ones. A missing, damaged or unsound record reads
//! as `DEFAULT_SLOTS`.

use embedded_storage::nor_flash::{NorFlash, ReadNorFlash};

use crate::{
    migration::{crc32, CRC32_INITIAL},
    Region, SlotProblem, Slots, DEFAULT_SLOTS, PAGE_SIZE, SLOT_TABLE,
};

// ============================================================================
// Records
// ============================================================================

pub const RECORD_SIZE: u32 = 32;
/// "NTST" in flash byte order.
const RECORD_MAGIC: u32 = u32::from_le_bytes(*b"NTST");
const RECORDS_PER_PAGE: u32 = PAGE_SIZE / RECORD_SIZE;
const PAGE_COUNT: u32 = SLOT_TABLE.size / PAGE_SIZE;
const WORDS_PER_RECORD: usize = (RECORD_SIZE / 4) as usize;
const CRC_WORD: usize = WORDS_PER_RECORD - 1;
const ERASED_WORD: u32 = 0xFFFF_FFFF;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Record {
    sequence: u32,
    slots: Slots,
}

impl Record {
    fn encode(&self) -> [u8; RECORD_SIZE as usize] {
        let mut words = [0; WORDS_PER_RECORD];
        words[0] = RECORD_MAGIC;
        words[1] = self.sequence;
        words[2] = self.slots.update.start;
        words[3] = self.slots.update.size;
        words[4] = self.slots.recovery.start;
        words[5] = self.slots.recovery.size;
        let mut bytes = words_to_bytes(&words);
        let crc = crc32(CRC32_INITIAL, &bytes[..CRC_WORD * 4]);
        bytes[CRC_WORD * 4..].copy_from_slice(&crc.to_le_bytes());
        bytes
    }

    /// Gives none for a record that is damaged or names slots the bootloader must not trust.
    fn decode(bytes: &[u8; RECORD_SIZE as usize]) -> Option<Record> {
        let words = bytes_to_words(bytes);
        let intact =
            words[0] == RECORD_MAGIC && words[CRC_WORD] == crc32(CRC32_INITIAL, &bytes[..CRC_WORD * 4]);
        if !intact {
            return None;
        }
        let slots = Slots {
            update: Region {
                start: words[2],
                size: words[3],
            },
            recovery: Region {
                start: words[4],
                size: words[5],
            },
        };
        slots.check().ok()?;
        Some(Record {
            sequence: words[1],
            slots,
        })
    }
}

fn words_to_bytes(words: &[u32; WORDS_PER_RECORD]) -> [u8; RECORD_SIZE as usize] {
    let mut bytes = [0; RECORD_SIZE as usize];
    for (chunk, word) in bytes.as_chunks_mut::<4>().0.iter_mut().zip(words) {
        chunk.copy_from_slice(&word.to_le_bytes());
    }
    bytes
}

fn bytes_to_words(bytes: &[u8; RECORD_SIZE as usize]) -> [u32; WORDS_PER_RECORD] {
    let mut words = [0; WORDS_PER_RECORD];
    for (word, chunk) in words.iter_mut().zip(bytes.as_chunks::<4>().0) {
        *word = u32::from_le_bytes(*chunk);
    }
    words
}

// ============================================================================
// Finding the newest record
// ============================================================================

/// What a scan of both pages found: the newest sound record, and where each page has room for the next.
struct Scan {
    newest: Option<(u32, Record)>,
    first_free_record: [Option<u32>; PAGE_COUNT as usize],
}

/// `table` is the `SLOT_TABLE` region, so offsets start at 0.
fn scan<Flash: ReadNorFlash>(table: &mut Flash) -> Result<Scan, Flash::Error> {
    let mut scan = Scan {
        newest: None,
        first_free_record: [None; PAGE_COUNT as usize],
    };
    let mut bytes = [0; RECORD_SIZE as usize];
    for page in 0..PAGE_COUNT {
        for record_index in 0..RECORDS_PER_PAGE {
            table.read(page * PAGE_SIZE + record_index * RECORD_SIZE, &mut bytes)?;
            if bytes_to_words(&bytes).iter().all(|&word| word == ERASED_WORD) {
                scan.first_free_record[page as usize] = Some(record_index);
                break;
            }
            let Some(record) = Record::decode(&bytes) else {
                continue;
            };
            let is_newer = match scan.newest {
                Some((_, newest)) => record.sequence > newest.sequence,
                None => true,
            };
            if is_newer {
                scan.newest = Some((page, record));
            }
        }
    }
    Ok(scan)
}

/// The slots the newest sound record names, or `DEFAULT_SLOTS` when there is none.
pub fn read_slots<Flash: ReadNorFlash>(table: &mut Flash) -> Result<Slots, Flash::Error> {
    Ok(scan(table)?
        .newest
        .map_or(DEFAULT_SLOTS, |(_, record)| record.slots))
}

// ============================================================================
// Writing a change
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotTableError<FlashError> {
    Unsound(SlotProblem),
    Flash(FlashError),
    ReadBackDiffers,
}

/// Appends a record naming `slots`, moving to the other page when this one is full.
pub fn write_slots<Flash: NorFlash>(
    table: &mut Flash,
    slots: Slots,
) -> Result<(), SlotTableError<Flash::Error>> {
    slots.check().map_err(SlotTableError::Unsound)?;
    let scan = scan(table).map_err(SlotTableError::Flash)?;

    let (newest_page, sequence) = match scan.newest {
        Some((page, record)) => (Some(page), record.sequence.wrapping_add(1)),
        None => (None, 1),
    };
    let room_in_newest_page = newest_page.and_then(|page| {
        scan.first_free_record[page as usize]
            .map(|record_index| page * PAGE_SIZE + record_index * RECORD_SIZE)
    });
    let offset = match room_in_newest_page {
        Some(offset) => offset,
        None => {
            let other_page = newest_page.map_or(0, |page| (page + 1) % PAGE_COUNT);
            let page_start = other_page * PAGE_SIZE;
            table
                .erase(page_start, page_start + PAGE_SIZE)
                .map_err(SlotTableError::Flash)?;
            page_start
        }
    };

    let encoded = Record { sequence, slots }.encode();
    table.write(offset, &encoded).map_err(SlotTableError::Flash)?;

    let mut read_back = [0; RECORD_SIZE as usize];
    table
        .read(offset, &mut read_back)
        .map_err(SlotTableError::Flash)?;
    if read_back != encoded {
        return Err(SlotTableError::ReadBackDiffers);
    }
    Ok(())
}
