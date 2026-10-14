//! The file area: each file a list of sectors in any order, committed by a header written last.

use embedded_storage_async::nor_flash::NorFlash;
use heapless::{String, Vec};

use crate::{
    checksum::{path_hash, Checksum},
    flash::{erase, half_word, read, word, write},
    Extent, FileExtents, StoreError, FORMAT_VERSION, LONGEST_PATH_BYTES, MOST_SECTORS_PER_FILE, SECTOR_BYTES,
};

// ============================================================================
// On-flash layout
// ============================================================================

const FILE_MAGIC: u32 = u32::from_le_bytes(*b"NTFL");
/// Magic, sequence, length, content checksum, sector count, format version, path length and a spare byte, then
/// 64 sector numbers, the path, a checksum of all that, and a retired word that starts erased and is written to
/// zero when a newer version replaces the file or it is deleted.
pub(crate) const FILE_HEADER_BYTES: usize = 224;
const SECTORS_AT: usize = 20;
const PATH_AT: usize = SECTORS_AT + 2 * MOST_SECTORS_PER_FILE;
const HEADER_CHECKSUM_AT: usize = PATH_AT + LONGEST_PATH_BYTES;
const RETIRED_AT: usize = HEADER_CHECKSUM_AT + 4;
/// The file area may be at most this many sectors, one bit each in RAM.
pub(crate) const MOST_FILE_SECTORS: usize = 1024;

pub(crate) struct FileHeader {
    pub(crate) sequence: u32,
    pub(crate) length: usize,
    content_checksum: u32,
    sectors: Vec<u16, MOST_SECTORS_PER_FILE>,
    pub(crate) path: String<LONGEST_PATH_BYTES>,
    retired: bool,
}

// Without a heap the header cannot be boxed; it is short-lived, on the stack while one header is read.
#[allow(clippy::large_enum_variant)]
enum HeaderAt {
    Valid(FileHeader),
    OtherVersion,
    Invalid,
}

fn parse_header(bytes: &[u8; FILE_HEADER_BYTES]) -> HeaderAt {
    if word(bytes, 0) != FILE_MAGIC
        || word(bytes, HEADER_CHECKSUM_AT) != Checksum::of(&bytes[..HEADER_CHECKSUM_AT])
    {
        return HeaderAt::Invalid;
    }
    if bytes[17] != FORMAT_VERSION {
        return HeaderAt::OtherVersion;
    }
    let count = bytes[16] as usize;
    let path_length = bytes[18] as usize;
    if count == 0 || count > MOST_SECTORS_PER_FILE || path_length > LONGEST_PATH_BYTES {
        return HeaderAt::Invalid;
    }
    let Ok(path_text) = core::str::from_utf8(&bytes[PATH_AT..PATH_AT + path_length]) else {
        return HeaderAt::Invalid;
    };
    let mut path = String::new();
    let _ = path.push_str(path_text);
    HeaderAt::Valid(FileHeader {
        sequence: word(bytes, 4),
        length: word(bytes, 8) as usize,
        content_checksum: word(bytes, 12),
        sectors: (0..count)
            .map(|index| half_word(bytes, SECTORS_AT + 2 * index))
            .collect(),
        path,
        retired: word(bytes, RETIRED_AT) != u32::MAX,
    })
}

/// 4 bytes of RAM per file: 16 bits of its path's hash and its first sector.
#[derive(Clone, Copy, Debug)]
struct FileEntry {
    path_hash: u16,
    first_sector: u16,
}

/// A file being written a part at a time; it replaces any older version only when finished.
pub(crate) struct Upload {
    path: String<LONGEST_PATH_BYTES>,
    sectors: Vec<u16, MOST_SECTORS_PER_FILE>,
    length: usize,
    checksum: Checksum,
}

impl Upload {
    pub(crate) fn path(&self) -> &String<LONGEST_PATH_BYTES> {
        &self.path
    }

    pub(crate) fn length(&self) -> usize {
        self.length
    }
}

// ============================================================================
// The file area
// ============================================================================

pub(crate) struct FileArea<const MOST_FILES: usize> {
    start_address: u32,
    sector_count: usize,
    /// One bit per sector, set while a live file or an upload uses it.
    used: [u8; MOST_FILE_SECTORS / 8],
    files: Vec<FileEntry, MOST_FILES>,
    pub(crate) next_sequence: u32,
    /// Where the search for free sectors starts, so writes spread over the area.
    cursor: usize,
}

impl<const MOST_FILES: usize> FileArea<MOST_FILES> {
    /// An area with nothing read yet, which `mount_in_place` fills where it lies.
    pub(crate) const fn unmounted(start_address: u32, sector_count: usize) -> Self {
        FileArea {
            start_address,
            sector_count,
            used: [0; MOST_FILE_SECTORS / 8],
            files: Vec::new(),
            next_sequence: 1,
            cursor: 0,
        }
    }

    pub(crate) async fn mount_in_place<Flash: NorFlash>(
        &mut self,
        flash: &mut Flash,
    ) -> Result<(), StoreError<Flash::Error>> {
        if self.sector_count > MOST_FILE_SECTORS {
            return Err(StoreError::LayoutDoesNotFit);
        }
        // Cleared field by field, since a fresh area built here would put the whole file table on the stack.
        self.used = [0; MOST_FILE_SECTORS / 8];
        self.files.clear();
        self.next_sequence = 1;
        self.cursor = 0;
        let sector_count = self.sector_count;
        for index in 0..sector_count {
            let header = match self.read_header(flash, index).await? {
                HeaderAt::Valid(header) => header,
                HeaderAt::OtherVersion => return Err(StoreError::OtherFormatVersion),
                HeaderAt::Invalid => continue,
            };
            self.next_sequence = self.next_sequence.max(header.sequence + 1);
            let sectors_fit = header
                .sectors
                .iter()
                .all(|sector| (*sector as usize) < sector_count);
            if header.retired || header.sectors[0] as usize != index || !sectors_fit {
                continue;
            }
            // Two live versions of one path mean the power went before the older was retired; retire it now.
            match self.find(flash, &header.path).await? {
                Some((position, older)) if older.sequence < header.sequence => {
                    self.retire(flash, position, &older).await?;
                }
                Some(_) => {
                    write(
                        flash,
                        self.sector_address(index) + RETIRED_AT as u32,
                        &0u32.to_le_bytes(),
                    )
                    .await?;
                    continue;
                }
                None => {}
            }
            self.add(index, &header).map_err(|_| StoreError::FileTableFull)?;
        }
        Ok(())
    }

    fn sector_address(&self, index: usize) -> u32 {
        self.start_address + (index as u32) * SECTOR_BYTES
    }

    fn is_used(&self, index: usize) -> bool {
        self.used[index / 8] & (1 << (index % 8)) != 0
    }

    fn set_used(&mut self, index: usize, used: bool) {
        if used {
            self.used[index / 8] |= 1 << (index % 8);
        } else {
            self.used[index / 8] &= !(1 << (index % 8));
        }
    }

    fn add(&mut self, first_sector: usize, header: &FileHeader) -> Result<(), ()> {
        let entry = FileEntry {
            path_hash: path_hash(&header.path) as u16,
            first_sector: first_sector as u16,
        };
        self.files.push(entry).map_err(|_| ())?;
        for sector in &header.sectors {
            self.set_used(*sector as usize, true);
        }
        Ok(())
    }

    async fn read_header<Flash: NorFlash>(
        &self,
        flash: &mut Flash,
        index: usize,
    ) -> Result<HeaderAt, StoreError<Flash::Error>> {
        let mut bytes = [0u8; FILE_HEADER_BYTES];
        read(flash, self.sector_address(index), &mut bytes).await?;
        Ok(parse_header(&bytes))
    }

    /// Finds a file by path: entries whose hash matches have their header read to compare the whole path.
    pub(crate) async fn find<Flash: NorFlash>(
        &self,
        flash: &mut Flash,
        path: &str,
    ) -> Result<Option<(usize, FileHeader)>, StoreError<Flash::Error>> {
        let hash = path_hash(path) as u16;
        for position in 0..self.files.len() {
            let entry = self.files[position];
            if entry.path_hash != hash {
                continue;
            }
            if let HeaderAt::Valid(header) = self.read_header(flash, entry.first_sector as usize).await? {
                if header.path == path {
                    return Ok(Some((position, header)));
                }
            }
        }
        Ok(None)
    }

    pub(crate) fn sector_count(&self) -> usize {
        self.sector_count
    }

    pub(crate) fn free_sectors(&self) -> usize {
        (0..self.sector_count)
            .filter(|index| !self.is_used(*index))
            .count()
    }

    /// Takes a free sector for an upload, erasing it, and marks it used so nothing else takes it.
    async fn take_sector<Flash: NorFlash>(
        &mut self,
        flash: &mut Flash,
    ) -> Result<u16, StoreError<Flash::Error>> {
        let index = (0..self.sector_count)
            .map(|step| (self.cursor + step) % self.sector_count)
            .find(|index| !self.is_used(*index))
            .ok_or(StoreError::FileAreaFull)?;
        erase(flash, self.sector_address(index)).await?;
        self.set_used(index, true);
        self.cursor = (index + 1) % self.sector_count;
        Ok(index as u16)
    }

    pub(crate) async fn begin<Flash: NorFlash>(
        &mut self,
        flash: &mut Flash,
        path: &str,
    ) -> Result<Upload, StoreError<Flash::Error>> {
        let mut stored_path = String::new();
        stored_path.push_str(path).map_err(|_| StoreError::PathTooLong)?;
        if self.files.is_full() && self.find(flash, path).await?.is_none() {
            return Err(StoreError::FileTableFull);
        }
        let mut upload = Upload {
            path: stored_path,
            sectors: Vec::new(),
            length: 0,
            checksum: Checksum::new(),
        };
        let first = self.take_sector(flash).await?;
        let _ = upload.sectors.push(first);
        Ok(upload)
    }

    /// Writes the next part of an upload, taking a new sector whenever one fills.
    pub(crate) async fn write_part<Flash: NorFlash>(
        &mut self,
        flash: &mut Flash,
        upload: &mut Upload,
        mut data: &[u8],
    ) -> Result<(), StoreError<Flash::Error>> {
        let sector_bytes = SECTOR_BYTES as usize;
        while !data.is_empty() {
            let position = FILE_HEADER_BYTES + upload.length;
            let part = position / sector_bytes;
            if part >= MOST_SECTORS_PER_FILE {
                return Err(StoreError::TooLarge);
            }
            if part == upload.sectors.len() {
                let sector = self.take_sector(flash).await?;
                let _ = upload.sectors.push(sector);
            }
            let in_sector = position % sector_bytes;
            let size = (sector_bytes - in_sector).min(data.len());
            let address = self.sector_address(upload.sectors[part] as usize) + in_sector as u32;
            write(flash, address, &data[..size]).await?;
            upload.checksum.add(&data[..size]);
            upload.length += size;
            data = &data[size..];
        }
        Ok(())
    }

    /// Writes the header, which makes the upload the file, then retires any older version.
    pub(crate) async fn finish<Flash: NorFlash>(
        &mut self,
        flash: &mut Flash,
        upload: Upload,
    ) -> Result<(), StoreError<Flash::Error>> {
        let older = self.find(flash, &upload.path).await?;
        let sequence = self.next_sequence;
        self.next_sequence += 1;
        let mut header = [0xffu8; FILE_HEADER_BYTES];
        header[0..4].copy_from_slice(&FILE_MAGIC.to_le_bytes());
        header[4..8].copy_from_slice(&sequence.to_le_bytes());
        header[8..12].copy_from_slice(&(upload.length as u32).to_le_bytes());
        header[12..16].copy_from_slice(&upload.checksum.value().to_le_bytes());
        header[16] = upload.sectors.len() as u8;
        header[17] = FORMAT_VERSION;
        header[18] = upload.path.len() as u8;
        header[19] = 0;
        header[SECTORS_AT..HEADER_CHECKSUM_AT].fill(0);
        for (part, sector) in upload.sectors.iter().enumerate() {
            header[SECTORS_AT + 2 * part..SECTORS_AT + 2 * part + 2].copy_from_slice(&sector.to_le_bytes());
        }
        header[PATH_AT..PATH_AT + upload.path.len()].copy_from_slice(upload.path.as_bytes());
        let checksum = Checksum::of(&header[..HEADER_CHECKSUM_AT]);
        header[HEADER_CHECKSUM_AT..RETIRED_AT].copy_from_slice(&checksum.to_le_bytes());
        write(
            flash,
            self.sector_address(upload.sectors[0] as usize),
            &header[..RETIRED_AT],
        )
        .await?;

        if let Some((position, older)) = older {
            self.retire(flash, position, &older).await?;
        }
        let entry = FileEntry {
            path_hash: path_hash(&upload.path) as u16,
            first_sector: upload.sectors[0],
        };
        self.files.push(entry).map_err(|_| StoreError::FileTableFull)
    }

    /// Gives an unfinished upload's sectors back; the flash keeps no trace of it, since it has no header.
    pub(crate) fn abandon(&mut self, upload: Upload) {
        for sector in upload.sectors {
            self.set_used(sector as usize, false);
        }
    }

    async fn retire<Flash: NorFlash>(
        &mut self,
        flash: &mut Flash,
        position: usize,
        header: &FileHeader,
    ) -> Result<(), StoreError<Flash::Error>> {
        let entry = self.files.remove(position);
        write(
            flash,
            self.sector_address(entry.first_sector as usize) + RETIRED_AT as u32,
            &0u32.to_le_bytes(),
        )
        .await?;
        for sector in &header.sectors {
            self.set_used(*sector as usize, false);
        }
        Ok(())
    }

    pub(crate) async fn delete<Flash: NorFlash>(
        &mut self,
        flash: &mut Flash,
        path: &str,
    ) -> Result<(), StoreError<Flash::Error>> {
        if let Some((position, header)) = self.find(flash, path).await? {
            self.retire(flash, position, &header).await?;
        }
        Ok(())
    }

    /// Fills `extents` with where `length` bytes from `offset` lie, joining sectors that follow one another.
    ///
    /// Gives how many extents and bytes that took, fewer bytes than `length` only at the file's end, or none when
    /// `extents` is too short.
    pub(crate) fn extents(
        &self,
        header: &FileHeader,
        offset: usize,
        length: usize,
        extents: &mut [Extent],
    ) -> Option<FileExtents> {
        let sector_bytes = SECTOR_BYTES as usize;
        let wanted = header.length.saturating_sub(offset).min(length);
        let (mut done, mut count) = (0, 0);
        while done < wanted {
            let position = FILE_HEADER_BYTES + offset + done;
            let (part, in_sector) = (position / sector_bytes, position % sector_bytes);
            let size = (sector_bytes - in_sector).min(wanted - done);
            let address = self.sector_address(header.sectors[part] as usize) + in_sector as u32;
            let follows_last = count > 0 && extents[count - 1].address + extents[count - 1].length == address;
            if follows_last {
                extents[count - 1].length += size as u32;
            } else {
                *extents.get_mut(count)? = Extent {
                    address,
                    length: size as u32,
                };
                count += 1;
            }
            done += size;
        }
        Some(FileExtents { count, bytes: wanted })
    }

    /// Reads from `offset` into the buffer, across sectors; the whole file read at once is checked against its checksum.
    pub(crate) async fn read<Flash: NorFlash>(
        &self,
        flash: &mut Flash,
        header: &FileHeader,
        offset: usize,
        buffer: &mut [u8],
    ) -> Result<Option<usize>, StoreError<Flash::Error>> {
        let sector_bytes = SECTOR_BYTES as usize;
        let wanted = header.length.saturating_sub(offset).min(buffer.len());
        let mut done = 0;
        while done < wanted {
            let position = FILE_HEADER_BYTES + offset + done;
            let (part, in_sector) = (position / sector_bytes, position % sector_bytes);
            let size = (sector_bytes - in_sector).min(wanted - done);
            let address = self.sector_address(header.sectors[part] as usize) + in_sector as u32;
            read(flash, address, &mut buffer[done..done + size]).await?;
            done += size;
        }
        let read_whole = offset == 0 && wanted == header.length;
        if read_whole && Checksum::of(&buffer[..wanted]) != header.content_checksum {
            return Err(StoreError::Corrupted);
        }
        Ok(Some(wanted))
    }

    /// Gives every live file's header to `visit`.
    pub(crate) async fn for_each<Flash: NorFlash>(
        &self,
        flash: &mut Flash,
        mut visit: impl FnMut(&FileHeader),
    ) -> Result<(), StoreError<Flash::Error>> {
        for entry in &self.files {
            if let HeaderAt::Valid(header) = self.read_header(flash, entry.first_sector as usize).await? {
                visit(&header);
            }
        }
        Ok(())
    }
}
