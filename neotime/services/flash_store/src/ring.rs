//! A ring of sectors holding records, used both for records and for logs; see the crate overview.

use embedded_storage_async::nor_flash::NorFlash;
use heapless::{vec::VecView, Vec};

use crate::{
    checksum::Checksum,
    flash::{erase, half_word, read, rounded_to_word, word, PageWriter, PIECE_BYTES},
    RecordKey, StoreError, FORMAT_VERSION, LARGEST_RECORD_BYTES, SECTOR_BYTES,
};

// ============================================================================
// On-flash layout
// ============================================================================

/// A ring sector starts with its magic, sequence number, erase count (3 bytes), format version and a checksum.
pub(crate) const SECTOR_HEADER_BYTES: usize = 16;
const SECTOR_MAGIC: u32 = u32::from_le_bytes(*b"NTSR");
/// A record is its key, sequence number, length, flags and a spare byte, its data, then a checksum of both.
pub(crate) const RECORD_HEADER_BYTES: usize = 12;
const RECORD_TRAILER_BYTES: usize = 4;
/// The key is gone; reading it finds nothing.
pub(crate) const TOMBSTONE: u8 = 1;
/// The data adds to the end of the key's entry instead of replacing it.
pub(crate) const APPENDS: u8 = 2;
/// Free sectors kept so cleanup always has somewhere to copy to, even after a power cut spoiled one.
const RESERVED_FREE_SECTORS: usize = 2;
const MOST_ERASE_COUNT: u32 = 0x00ff_ffff;

pub(crate) fn record_bytes(length: usize) -> usize {
    RECORD_HEADER_BYTES + rounded_to_word(length) + RECORD_TRAILER_BYTES
}

// ============================================================================
// What RAM keeps
// ============================================================================

/// Where a key's newest copy is: the key, then its offset in the ring in words (18 bits), length (12) and flags (2).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Newest {
    pub(crate) key: RecordKey,
    place: u32,
}

const OFFSET_BITS: u32 = 18;
const LENGTH_BITS: u32 = 12;

impl Newest {
    fn new(key: RecordKey, offset: usize, length: usize, flags: u8) -> Newest {
        let place = (offset / 4) as u32
            | (length as u32) << OFFSET_BITS
            | (flags as u32) << (OFFSET_BITS + LENGTH_BITS);
        Newest { key, place }
    }

    pub(crate) fn offset(&self) -> usize {
        (self.place & ((1 << OFFSET_BITS) - 1)) as usize * 4
    }

    pub(crate) fn length(&self) -> usize {
        ((self.place >> OFFSET_BITS) & ((1 << LENGTH_BITS) - 1)) as usize
    }

    pub(crate) fn flags(&self) -> u8 {
        (self.place >> (OFFSET_BITS + LENGTH_BITS)) as u8
    }

    pub(crate) fn is_tombstone(&self) -> bool {
        self.flags() & TOMBSTONE != 0
    }
}

/// The table of each key's newest copy, sorted by key, kept by the store and lent to the ring; logs lend an empty one.
pub(crate) type KeyTable = VecView<Newest>;

pub(crate) fn find_newest(keys: &[Newest], key: RecordKey) -> Option<Newest> {
    let position = keys.binary_search_by_key(&key, |entry| entry.key).ok()?;
    Some(keys[position]).filter(|entry| !entry.is_tombstone())
}

/// One record as a scan of a sector finds it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ScannedRecord {
    pub(crate) key: RecordKey,
    pub(crate) offset: usize,
    pub(crate) length: usize,
    pub(crate) sequence: u32,
    pub(crate) flags: u8,
}

enum SectorHeader {
    Valid { sequence: u32, erase_count: u32 },
    OtherVersion,
    Invalid,
}

/// Where a record's data comes from when it is written.
pub(crate) enum RecordData<'data> {
    Memory(&'data [u8]),
    /// Pieces written one after another as one record, so they never have to be gathered in RAM.
    Pieces(&'data [&'data [u8]]),
    /// Copied from another record in the same ring, a piece at a time, keeping its sequence number.
    Copy {
        offset: usize,
        length: usize,
    },
}

// ============================================================================
// The ring
// ============================================================================

pub(crate) struct Ring<const SECTORS: usize> {
    start_address: u32,
    sector_count: usize,
    /// Records carry their newest copies forward when cleaned; logs let their oldest sector go.
    keeps_newest: bool,
    /// The used sectors, oldest first; the last is where records go.
    order: Vec<u8, SECTORS>,
    /// One bit per sector, set while it is known to be erased, so it is not erased twice.
    erased: [u8; 32],
    head_end: u16,
    next_sector_sequence: u32,
    pub(crate) next_record_sequence: u32,
    /// The flash the newest copies take, which writes may not push past half the ring.
    bytes_in_use: usize,
}

impl<const SECTORS: usize> Ring<SECTORS> {
    /// A ring with nothing read yet, which `mount_in_place` fills where it lies, so no copy of it is ever made.
    pub(crate) const fn unmounted(start_address: u32, sector_count: usize, keeps_newest: bool) -> Self {
        Ring {
            start_address,
            sector_count,
            keeps_newest,
            order: Vec::new(),
            erased: [0; 32],
            head_end: SECTOR_BYTES as u16,
            next_sector_sequence: 1,
            next_record_sequence: 1,
            bytes_in_use: 0,
        }
    }

    pub(crate) async fn mount_in_place<Flash: NorFlash>(
        &mut self,
        flash: &mut Flash,
        keys: &mut KeyTable,
    ) -> Result<(), StoreError<Flash::Error>> {
        if self.sector_count > SECTORS || !(RESERVED_FREE_SECTORS + 2..=256).contains(&self.sector_count) {
            return Err(StoreError::LayoutDoesNotFit);
        }
        *self = Ring::unmounted(self.start_address, self.sector_count, self.keeps_newest);
        keys.clear();
        // Each used sector goes into the order by its sequence as it is found, so the order ends up oldest first.
        let mut sequences = [0u32; SECTORS];
        for index in 0..self.sector_count {
            match self.read_sector_header(flash, index).await? {
                SectorHeader::Valid { sequence, .. } => {
                    sequences[index] = sequence;
                    self.next_sector_sequence = self.next_sector_sequence.max(sequence + 1);
                    let mut position = self.order.len();
                    let _ = self.order.push(index as u8);
                    while position > 0 && sequences[self.order[position - 1] as usize] > sequence {
                        self.order.swap(position - 1, position);
                        position -= 1;
                    }
                }
                SectorHeader::OtherVersion => return Err(StoreError::OtherFormatVersion),
                SectorHeader::Invalid => {}
            }
        }
        for position in 0..self.order.len() {
            let index = self.order[position];
            let mut table_full = false;
            let start_address = self.start_address;
            let end = scan_sector(flash, start_address, index as usize, |record| {
                self.next_record_sequence = self.next_record_sequence.max(record.sequence + 1);
                if self.note_record(keys, record).is_err() {
                    table_full = true;
                }
            })
            .await?;
            if table_full {
                return Err(StoreError::KeyTableFull);
            }
            self.head_end = end as u16;
        }
        if self.order.is_empty() {
            self.start_new_head(flash).await?;
        }
        // A power cut during cleanup can leave too few free sectors; finish the cleanup now.
        self.keep_free_sectors(flash, keys).await
    }

    fn sector_address(&self, index: usize) -> u32 {
        self.start_address + (index as u32) * SECTOR_BYTES
    }

    fn index_of_offset(&self, offset: usize) -> usize {
        offset / SECTOR_BYTES as usize
    }

    fn head(&self) -> usize {
        self.order[self.order.len() - 1] as usize
    }

    fn free_count(&self) -> usize {
        self.sector_count - self.order.len()
    }

    fn is_used(&self, index: usize) -> bool {
        self.order.contains(&(index as u8))
    }

    fn is_erased(&self, index: usize) -> bool {
        self.erased[index / 8] & (1 << (index % 8)) != 0
    }

    fn set_erased(&mut self, index: usize, erased: bool) {
        if erased {
            self.erased[index / 8] |= 1 << (index % 8);
        } else {
            self.erased[index / 8] &= !(1 << (index % 8));
        }
    }

    async fn read_sector_header<Flash: NorFlash>(
        &self,
        flash: &mut Flash,
        index: usize,
    ) -> Result<SectorHeader, StoreError<Flash::Error>> {
        let mut bytes = [0u8; SECTOR_HEADER_BYTES];
        read(flash, self.sector_address(index), &mut bytes).await?;
        if word(&bytes, 0) != SECTOR_MAGIC || word(&bytes, 12) != Checksum::of(&bytes[..12]) {
            return Ok(SectorHeader::Invalid);
        }
        if bytes[11] != FORMAT_VERSION {
            return Ok(SectorHeader::OtherVersion);
        }
        Ok(SectorHeader::Valid {
            sequence: word(&bytes, 4),
            erase_count: word(&bytes, 8) & MOST_ERASE_COUNT,
        })
    }

    /// Records keep each key's newest copy; logs keep nothing, as they are read by scanning.
    fn note_record(&mut self, keys: &mut KeyTable, record: ScannedRecord) -> Result<(), ()> {
        if !self.keeps_newest {
            return Ok(());
        }
        let newest = Newest::new(record.key, record.offset, record.length, record.flags);
        match keys.binary_search_by_key(&record.key, |entry| entry.key) {
            Ok(position) => {
                self.bytes_in_use -= record_bytes(keys[position].length());
                keys[position] = newest;
            }
            Err(position) => {
                keys.insert(position, newest).map_err(|_| ())?;
            }
        }
        self.bytes_in_use += record_bytes(record.length);
        Ok(())
    }

    /// The free sector the next head takes: the first one after the current head.
    fn next_head_index(&self) -> Option<usize> {
        let after = self.order.last().map_or(0, |head| *head as usize + 1);
        (0..self.sector_count)
            .map(|step| (after + step) % self.sector_count)
            .find(|index| !self.is_used(*index))
    }

    /// Makes a free sector the one records go to, erasing it first unless it is known to be erased.
    async fn start_new_head<Flash: NorFlash>(
        &mut self,
        flash: &mut Flash,
    ) -> Result<(), StoreError<Flash::Error>> {
        let index = self.next_head_index().ok_or(StoreError::Full)?;
        let erase_count = match self.read_sector_header(flash, index).await? {
            SectorHeader::Valid { erase_count, .. } => erase_count + 1,
            _ => 1,
        };
        if !self.is_erased(index) {
            erase(flash, self.sector_address(index)).await?;
        }
        let sequence = self.next_sector_sequence;
        self.next_sector_sequence += 1;
        let mut header = [0u8; SECTOR_HEADER_BYTES];
        header[0..4].copy_from_slice(&SECTOR_MAGIC.to_le_bytes());
        header[4..8].copy_from_slice(&sequence.to_le_bytes());
        let erase_count_and_version = erase_count.min(MOST_ERASE_COUNT) | (FORMAT_VERSION as u32) << 24;
        header[8..12].copy_from_slice(&erase_count_and_version.to_le_bytes());
        let checksum = Checksum::of(&header[..12]);
        header[12..16].copy_from_slice(&checksum.to_le_bytes());
        let mut writer = PageWriter::new(self.sector_address(index));
        writer.push(flash, &header).await?;
        writer.finish(flash).await?;
        self.set_erased(index, false);
        let _ = self.order.push(index as u8);
        self.head_end = SECTOR_HEADER_BYTES as u16;
        Ok(())
    }

    /// The flash each sector's newest copies take, so cleanup can pick the sector that gives most space back.
    fn live_bytes_per_sector(&self, keys: &[Newest]) -> [u16; 256] {
        let mut live = [0u16; 256];
        for entry in keys {
            live[self.index_of_offset(entry.offset())] += record_bytes(entry.length()) as u16;
        }
        live
    }

    /// Logs drop their oldest sector; records clean the sector with the most to gain for its age.
    fn choose_victim(&self, keys: &[Newest]) -> usize {
        if !self.keeps_newest {
            return 0;
        }
        let live = self.live_bytes_per_sector(keys);
        let capacity = (SECTOR_BYTES as usize - SECTOR_HEADER_BYTES) as u64;
        let candidates = self.order.len() - 1;
        (0..candidates)
            .max_by_key(|position| {
                let free = capacity.saturating_sub(live[self.order[*position] as usize] as u64);
                let age = (candidates - position) as u64;
                (free * age, age)
            })
            .unwrap_or(0)
    }

    /// Frees one sector, first copying forward the newest copies it holds, then erases it.
    ///
    /// A tombstone is dropped instead of copied once no older copy of its key is left anywhere.
    async fn clean<Flash: NorFlash>(
        &mut self,
        flash: &mut Flash,
        keys: &mut KeyTable,
        position: usize,
    ) -> Result<(), StoreError<Flash::Error>> {
        let victim = self.order[position] as usize;
        if self.keeps_newest {
            let first = victim * SECTOR_BYTES as usize;
            let mut cursor = 0;
            while cursor < keys.len() {
                let entry = keys[cursor];
                let in_victim = entry.offset() >= first && entry.offset() < first + SECTOR_BYTES as usize;
                if !in_victim {
                    cursor += 1;
                    continue;
                }
                if entry.is_tombstone() && !self.has_older_copy(flash, entry.key, victim).await? {
                    self.bytes_in_use -= record_bytes(0);
                    keys.remove(cursor);
                    continue;
                }
                if !self.fits_in_head(entry.length()) {
                    self.start_new_head(flash).await?;
                }
                let data = RecordData::Copy {
                    offset: entry.offset() + RECORD_HEADER_BYTES,
                    length: entry.length(),
                };
                self.write_record(flash, keys, entry.key, entry.flags(), data)
                    .await?;
                cursor += 1;
            }
        }
        self.order.remove(position);
        erase(flash, self.sector_address(victim)).await?;
        self.set_erased(victim, true);
        Ok(())
    }

    /// Whether any used sector but `excluding` still holds a record of the key, which a tombstone must outlive.
    async fn has_older_copy<Flash: NorFlash>(
        &self,
        flash: &mut Flash,
        key: RecordKey,
        excluding: usize,
    ) -> Result<bool, StoreError<Flash::Error>> {
        for index in self
            .order
            .iter()
            .map(|index| *index as usize)
            .filter(|index| *index != excluding)
        {
            let mut found = false;
            scan_sector(flash, self.start_address, index, |record| {
                found |= record.key == key
            })
            .await?;
            if found {
                return Ok(true);
            }
        }
        Ok(false)
    }

    async fn keep_free_sectors<Flash: NorFlash>(
        &mut self,
        flash: &mut Flash,
        keys: &mut KeyTable,
    ) -> Result<(), StoreError<Flash::Error>> {
        let mut cleaned = 0;
        while self.free_count() < RESERVED_FREE_SECTORS && self.order.len() > 1 {
            if cleaned >= self.sector_count {
                return Err(StoreError::Full);
            }
            let victim = self.choose_victim(keys);
            self.clean(flash, keys, victim).await?;
            cleaned += 1;
        }
        Ok(())
    }

    /// Does ahead of time the erasing a write would otherwise do when the head fills, so that write only programs.
    ///
    /// Records clean a sector early, so starting the next head leaves the reserve whole; logs never drop data early.
    pub(crate) async fn prepare_room<Flash: NorFlash>(
        &mut self,
        flash: &mut Flash,
        keys: &mut KeyTable,
    ) -> Result<(), StoreError<Flash::Error>> {
        let mut cleaned = 0;
        while self.keeps_newest
            && self.free_count() <= RESERVED_FREE_SECTORS
            && self.order.len() > 1
            && cleaned < self.sector_count
        {
            let victim = self.choose_victim(keys);
            self.clean(flash, keys, victim).await?;
            cleaned += 1;
        }
        if let Some(index) = self.next_head_index().filter(|index| !self.is_erased(*index)) {
            erase(flash, self.sector_address(index)).await?;
            self.set_erased(index, true);
        }
        Ok(())
    }

    fn fits_in_head(&self, length: usize) -> bool {
        self.head_end as usize + record_bytes(length) <= SECTOR_BYTES as usize
    }

    /// The flash records may use: every sector but the reserved ones and the one being filled.
    fn usable_bytes(&self) -> usize {
        (self.sector_count - RESERVED_FREE_SECTORS - 1) * (SECTOR_BYTES as usize - SECTOR_HEADER_BYTES)
    }

    pub(crate) fn bytes_in_use(&self) -> usize {
        self.bytes_in_use
    }

    /// Writes a record at the end of the newest sector through a page writer: header, data, then the checksum.
    async fn write_record<Flash: NorFlash>(
        &mut self,
        flash: &mut Flash,
        keys: &mut KeyTable,
        key: RecordKey,
        flags: u8,
        data: RecordData<'_>,
    ) -> Result<(), StoreError<Flash::Error>> {
        let (length, sequence) = match data {
            RecordData::Memory(bytes) => {
                self.next_record_sequence += 1;
                (bytes.len(), self.next_record_sequence - 1)
            }
            RecordData::Pieces(pieces) => {
                self.next_record_sequence += 1;
                (
                    pieces.iter().map(|piece| piece.len()).sum(),
                    self.next_record_sequence - 1,
                )
            }
            RecordData::Copy { offset, length } => {
                let mut original = [0u8; RECORD_HEADER_BYTES];
                read(
                    flash,
                    self.start_address + (offset - RECORD_HEADER_BYTES) as u32,
                    &mut original,
                )
                .await?;
                (length, word(&original, 4))
            }
        };
        let offset = self.head() * SECTOR_BYTES as usize + self.head_end as usize;
        let mut header = [0u8; RECORD_HEADER_BYTES];
        header[0..4].copy_from_slice(&key.to_le_bytes());
        header[4..8].copy_from_slice(&sequence.to_le_bytes());
        header[8..10].copy_from_slice(&(length as u16).to_le_bytes());
        header[10] = flags;
        let mut checksum = Checksum::new();
        checksum.add(&header);
        let mut writer = PageWriter::new(self.start_address + offset as u32);
        writer.push(flash, &header).await?;
        match data {
            RecordData::Memory(bytes) => {
                checksum.add(bytes);
                writer.push(flash, bytes).await?;
            }
            RecordData::Pieces(pieces) => {
                for piece in pieces {
                    checksum.add(piece);
                    writer.push(flash, piece).await?;
                }
            }
            RecordData::Copy {
                offset: source,
                length,
            } => {
                let mut piece = [0u8; PIECE_BYTES];
                let mut done = 0;
                while done < length {
                    let size = PIECE_BYTES.min(length - done);
                    read(
                        flash,
                        self.start_address + (source + done) as u32,
                        &mut piece[..size],
                    )
                    .await?;
                    checksum.add(&piece[..size]);
                    writer.push(flash, &piece[..size]).await?;
                    done += size;
                }
            }
        }
        // Padding is left erased; writing 0xff programs nothing.
        writer
            .push(flash, &[0xff; 3][..rounded_to_word(length) - length])
            .await?;
        writer.push(flash, &checksum.value().to_le_bytes()).await?;
        writer.finish(flash).await?;
        self.head_end += record_bytes(length) as u16;
        let record = ScannedRecord {
            key,
            offset,
            length,
            sequence,
            flags,
        };
        self.note_record(keys, record)
            .map_err(|_| StoreError::KeyTableFull)
    }

    /// Appends a record made of pieces, such as a small file's path and its data.
    pub(crate) async fn append_pieces<Flash: NorFlash>(
        &mut self,
        flash: &mut Flash,
        keys: &mut KeyTable,
        key: RecordKey,
        pieces: &[&[u8]],
    ) -> Result<(), StoreError<Flash::Error>> {
        let length: usize = pieces.iter().map(|piece| piece.len()).sum();
        self.make_room(flash, keys, key, 0, length).await?;
        self.write_record(flash, keys, key, 0, RecordData::Pieces(pieces))
            .await
    }

    /// Appends a record, refusing one that would push the newest copies past half the ring.
    pub(crate) async fn append<Flash: NorFlash>(
        &mut self,
        flash: &mut Flash,
        keys: &mut KeyTable,
        key: RecordKey,
        flags: u8,
        data: &[u8],
    ) -> Result<(), StoreError<Flash::Error>> {
        self.make_room(flash, keys, key, flags, data.len()).await?;
        self.write_record(flash, keys, key, flags, RecordData::Memory(data))
            .await
    }

    /// Checks a record of `length` bytes may be written, then starts new sectors until it fits.
    async fn make_room<Flash: NorFlash>(
        &mut self,
        flash: &mut Flash,
        keys: &mut KeyTable,
        key: RecordKey,
        flags: u8,
        length: usize,
    ) -> Result<(), StoreError<Flash::Error>> {
        if length > LARGEST_RECORD_BYTES {
            return Err(StoreError::TooLarge);
        }
        if self.keeps_newest {
            let existing = keys.binary_search_by_key(&key, |entry| entry.key).ok();
            if existing.is_none() && keys.is_full() {
                return Err(StoreError::KeyTableFull);
            }
            let replaced = existing.map_or(0, |position| record_bytes(keys[position].length()));
            let after = self.bytes_in_use - replaced + record_bytes(length);
            if flags & TOMBSTONE == 0 && after > self.usable_bytes() / 2 {
                return Err(StoreError::Full);
            }
        }
        while !self.fits_in_head(length) {
            self.start_new_head(flash).await?;
            self.keep_free_sectors(flash, keys).await?;
        }
        Ok(())
    }

    /// Reads from `offset` into a key's newest copy, as much as the buffer holds; nothing when the key is missing.
    pub(crate) async fn read_newest<Flash: NorFlash>(
        &self,
        flash: &mut Flash,
        keys: &[Newest],
        key: RecordKey,
        offset: usize,
        buffer: &mut [u8],
    ) -> Result<Option<usize>, StoreError<Flash::Error>> {
        let Some(entry) = find_newest(keys, key) else {
            return Ok(None);
        };
        let length = entry.length().saturating_sub(offset).min(buffer.len());
        let address = self.start_address + (entry.offset() + RECORD_HEADER_BYTES + offset) as u32;
        read(flash, address, &mut buffer[..length]).await?;
        Ok(Some(length))
    }

    /// Reads a record's header and the start of its data in one read, filling `bytes` with both.
    pub(crate) async fn read_record_start<Flash: NorFlash>(
        &self,
        flash: &mut Flash,
        entry: Newest,
        bytes: &mut [u8],
    ) -> Result<(), StoreError<Flash::Error>> {
        let length = bytes.len().min(RECORD_HEADER_BYTES + entry.length());
        read(
            flash,
            self.start_address + entry.offset() as u32,
            &mut bytes[..length],
        )
        .await
    }

    /// Puts a log entry back together from its records, oldest sector first, and gives its whole length.
    ///
    /// Only as much as the buffer holds is copied. Logs are read rarely, so this scans the ring.
    pub(crate) async fn read_entry<Flash: NorFlash>(
        &self,
        flash: &mut Flash,
        key: RecordKey,
        buffer: &mut [u8],
    ) -> Result<Option<usize>, StoreError<Flash::Error>> {
        let mut length: Option<usize> = None;
        for index in self.order.iter().map(|index| *index as usize) {
            let mut offset = SECTOR_HEADER_BYTES;
            while let RecordAt::Record { record, next_offset } =
                record_at(flash, self.start_address, index, offset).await?
            {
                offset = next_offset;
                if record.key != key {
                    continue;
                }
                let start = if record.flags & APPENDS != 0 {
                    length.unwrap_or(0)
                } else {
                    0
                };
                let end = start + record.length;
                if start < buffer.len() {
                    let copied = end.min(buffer.len()) - start;
                    let address = self.start_address + (record.offset + RECORD_HEADER_BYTES) as u32;
                    read(flash, address, &mut buffer[start..start + copied]).await?;
                }
                length = Some(end);
            }
        }
        Ok(length)
    }
}

// ============================================================================
// Scanning a sector
// ============================================================================

/// What a sector holds at an offset: a valid record, the end of its records, or a record a power cut cut short.
enum RecordAt {
    Record {
        record: ScannedRecord,
        next_offset: usize,
    },
    End,
    CutShort,
}

/// Reads and checks the record at `offset` in a sector.
async fn record_at<Flash: NorFlash>(
    flash: &mut Flash,
    ring_start_address: u32,
    index: usize,
    offset: usize,
) -> Result<RecordAt, StoreError<Flash::Error>> {
    let sector_bytes = SECTOR_BYTES as usize;
    let sector_address = ring_start_address + (index * sector_bytes) as u32;
    if offset + record_bytes(0) > sector_bytes {
        return Ok(RecordAt::End);
    }
    let mut header = [0u8; RECORD_HEADER_BYTES];
    read(flash, sector_address + offset as u32, &mut header).await?;
    if header.iter().all(|byte| *byte == 0xff) {
        return Ok(RecordAt::End);
    }
    let length = half_word(&header, 8) as usize;
    if length > LARGEST_RECORD_BYTES || offset + record_bytes(length) > sector_bytes {
        return Ok(RecordAt::CutShort);
    }
    let mut checksum = Checksum::new();
    checksum.add(&header);
    let mut piece = [0u8; PIECE_BYTES];
    let mut done = 0;
    while done < length {
        let size = PIECE_BYTES.min(length - done);
        read(
            flash,
            sector_address + (offset + RECORD_HEADER_BYTES + done) as u32,
            &mut piece[..size],
        )
        .await?;
        checksum.add(&piece[..size]);
        done += size;
    }
    let mut trailer = [0u8; RECORD_TRAILER_BYTES];
    let trailer_address = sector_address + (offset + RECORD_HEADER_BYTES + rounded_to_word(length)) as u32;
    read(flash, trailer_address, &mut trailer).await?;
    if word(&trailer, 0) != checksum.value() {
        return Ok(RecordAt::CutShort);
    }
    let record = ScannedRecord {
        key: word(&header, 0),
        offset: index * sector_bytes + offset,
        length,
        sequence: word(&header, 4),
        flags: header[10],
    };
    Ok(RecordAt::Record {
        record,
        next_offset: offset + record_bytes(length),
    })
}

/// Reads a sector's records in order, giving each to `visit`, and returns where the next one would go.
///
/// A record that fails its checksum was cut short by a power cut, so nothing after it in the sector is used, and
/// the sector's end is returned so nothing more is written there.
async fn scan_sector<Flash: NorFlash>(
    flash: &mut Flash,
    ring_start_address: u32,
    index: usize,
    mut visit: impl FnMut(ScannedRecord),
) -> Result<usize, StoreError<Flash::Error>> {
    let mut offset = SECTOR_HEADER_BYTES;
    loop {
        match record_at(flash, ring_start_address, index, offset).await? {
            RecordAt::Record { record, next_offset } => {
                visit(record);
                offset = next_offset;
            }
            RecordAt::End => return Ok(offset),
            RecordAt::CutShort => return Ok(SECTOR_BYTES as usize),
        }
    }
}
