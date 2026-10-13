//! A simulated external flash that counts every operation and adds up how long the real chip would take.

use std::{cell::Cell, rc::Rc};

use embedded_storage_async::nor_flash::{
    ErrorType, MultiwriteNorFlash, NorFlash, NorFlashError, NorFlashErrorKind, ReadNorFlash,
};

// ============================================================================
// Size and timing
// ============================================================================

/// One erase sector of the XT25F32B, and the page size both stores use.
pub const SECTOR_BYTES: usize = 4096;
/// The chip programs at most this many bytes per program command.
const PROGRAM_PAGE_BYTES: usize = 256;
/// The nRF52832's EasyDMA moves at most this many bytes per SPI transfer.
const DMA_CHUNK_BYTES: usize = 255;

/// How long each part of an operation takes, in microseconds.
#[derive(Clone, Copy, Debug)]
pub struct Timing {
    /// Chip select, the command and its 3 address bytes, and the driver starting a transfer.
    pub command: f64,
    /// One byte over the bus at 8 MHz.
    pub byte: f64,
    /// The interrupt and wake-up between two DMA chunks of one transfer.
    pub dma_chunk: f64,
    /// The chip programming one page after it has the bytes.
    pub page_program: f64,
    /// The chip erasing one 4 KB sector.
    pub sector_erase: f64,
}

impl Timing {
    /// Assumed typical figures for a 4 MB SPI NOR flash on an 8 MHz bus; replace them with the datasheet's.
    pub const ASSUMED: Timing = Timing {
        command: 10.0,
        byte: 1.0,
        dma_chunk: 3.0,
        page_program: 600.0,
        sector_erase: 50_000.0,
    };

    fn read(&self, byte_count: usize) -> f64 {
        self.command + byte_count as f64 * self.byte + byte_count.div_ceil(DMA_CHUNK_BYTES) as f64 * self.dma_chunk
    }
}

// ============================================================================
// Counters
// ============================================================================

#[derive(Clone, Copy, Debug, Default)]
pub struct FlashCounters {
    pub reads: u64,
    pub read_bytes: u64,
    pub writes: u64,
    pub written_bytes: u64,
    pub erases: u64,
    pub busy_microseconds: f64,
    /// Writes and erases so far, which a power cut is armed against.
    pub changes: u64,
    /// Set once a power cut has happened; every write and erase after it is lost.
    pub power_lost: bool,
}

impl FlashCounters {
    pub fn since(&self, earlier: &FlashCounters) -> FlashCounters {
        FlashCounters {
            reads: self.reads - earlier.reads,
            read_bytes: self.read_bytes - earlier.read_bytes,
            writes: self.writes - earlier.writes,
            written_bytes: self.written_bytes - earlier.written_bytes,
            erases: self.erases - earlier.erases,
            busy_microseconds: self.busy_microseconds - earlier.busy_microseconds,
            changes: self.changes - earlier.changes,
            power_lost: self.power_lost,
        }
    }
}

/// The counters, shared so the benchmark can read them while a store owns the flash.
pub type SharedCounters = Rc<Cell<FlashCounters>>;

// ============================================================================
// The flash
// ============================================================================

pub struct SimulatedFlash {
    bytes: Vec<u8>,
    timing: Timing,
    counters: SharedCounters,
    /// How often each sector has been erased.
    erase_counts: Vec<u32>,
    /// The write or erase, counted from 1, during which the power goes; it is left half done.
    power_cut_at: Option<u64>,
    /// Picks how much of the interrupted write or erase lands.
    noise: u64,
}

impl SimulatedFlash {
    pub fn new(sector_count: usize, timing: Timing) -> (SimulatedFlash, SharedCounters) {
        SimulatedFlash::from_bytes(vec![0xff; sector_count * SECTOR_BYTES], timing)
    }

    /// A flash holding these bytes, as after a restart.
    pub fn from_bytes(bytes: Vec<u8>, timing: Timing) -> (SimulatedFlash, SharedCounters) {
        let counters = SharedCounters::default();
        let flash = SimulatedFlash {
            erase_counts: vec![0; bytes.len() / SECTOR_BYTES],
            bytes,
            timing,
            counters: counters.clone(),
            power_cut_at: None,
            noise: 0x9e37_79b9_7f4a_7c15,
        };
        (flash, counters)
    }

    pub fn cut_power_at(&mut self, change: u64, seed: u64) {
        self.power_cut_at = Some(change);
        self.noise = seed | 1;
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn most_erases_of_one_sector(&self, sectors: std::ops::Range<usize>) -> u32 {
        self.erase_counts[sectors].iter().copied().max().unwrap_or(0)
    }

    fn next_noise(&mut self) -> u64 {
        self.noise ^= self.noise << 13;
        self.noise ^= self.noise >> 7;
        self.noise ^= self.noise << 17;
        self.noise
    }

    /// Counts a write or erase, and says whether it lands whole, lands half done as the power goes, or is lost.
    fn power_for_change(&mut self) -> Power {
        let mut counters = self.counters.get();
        if counters.power_lost {
            return Power::Off;
        }
        counters.changes += 1;
        let cut_now = self.power_cut_at == Some(counters.changes);
        counters.power_lost = cut_now;
        self.counters.set(counters);
        if cut_now {
            Power::GoingOff
        } else {
            Power::On
        }
    }

    fn count(&self, change: impl FnOnce(&mut FlashCounters)) {
        let mut counters = self.counters.get();
        change(&mut counters);
        self.counters.set(counters);
    }

    fn read_at(&mut self, address: usize, buffer: &mut [u8]) {
        buffer.copy_from_slice(&self.bytes[address..address + buffer.len()]);
        let time = self.timing.read(buffer.len());
        self.count(|counters| {
            counters.reads += 1;
            counters.read_bytes += buffer.len() as u64;
            counters.busy_microseconds += time;
        });
    }

    /// Programs bytes as NOR flash does: bits only go from 1 to 0, one 256-byte page per command.
    fn write_at(&mut self, address: usize, data: &[u8]) {
        let mut done = 0;
        while done < data.len() {
            let start = address + done;
            let room_in_page = PROGRAM_PAGE_BYTES - start % PROGRAM_PAGE_BYTES;
            let length = room_in_page.min(data.len() - done);
            match self.power_for_change() {
                Power::Off => return,
                Power::On => {
                    for (stored, new) in self.bytes[start..start + length].iter_mut().zip(&data[done..done + length]) {
                        *stored &= new;
                    }
                }
                // Some of the page lands, and the byte where the power went is only partly programmed.
                Power::GoingOff => {
                    let landed = (self.next_noise() % (length as u64 + 1)) as usize;
                    for index in 0..landed {
                        self.bytes[start + index] &= data[done + index];
                    }
                    if landed < length {
                        let partly = data[done + landed] | self.next_noise() as u8;
                        self.bytes[start + landed] &= partly;
                    }
                    return;
                }
            }
            let time = self.timing.command + length as f64 * self.timing.byte + self.timing.page_program;
            self.count(|counters| {
                counters.writes += 1;
                counters.written_bytes += length as u64;
                counters.busy_microseconds += time;
            });
            done += length;
        }
    }

    fn erase_sector(&mut self, sector: usize) {
        match self.power_for_change() {
            Power::Off => return,
            Power::On => self.bytes[sector * SECTOR_BYTES..(sector + 1) * SECTOR_BYTES].fill(0xff),
            // An interrupted erase leaves each byte erased, as it was, or anything at all.
            Power::GoingOff => {
                for index in sector * SECTOR_BYTES..(sector + 1) * SECTOR_BYTES {
                    let noise = self.next_noise();
                    self.bytes[index] = match noise % 3 {
                        0 => 0xff,
                        1 => self.bytes[index],
                        _ => (noise >> 8) as u8,
                    };
                }
                return;
            }
        }
        self.erase_counts[sector] += 1;
        let time = self.timing.command + self.timing.sector_erase;
        self.count(|counters| {
            counters.erases += 1;
            counters.busy_microseconds += time;
        });
    }
}

enum Power {
    On,
    GoingOff,
    Off,
}

// ============================================================================
// As sequential-storage sees it
// ============================================================================

#[derive(Debug)]
pub struct FlashError(NorFlashErrorKind);

impl NorFlashError for FlashError {
    fn kind(&self) -> NorFlashErrorKind {
        self.0
    }
}

impl ErrorType for SimulatedFlash {
    type Error = FlashError;
}

impl ReadNorFlash for SimulatedFlash {
    const READ_SIZE: usize = 1;

    async fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), FlashError> {
        if offset as usize + bytes.len() > self.bytes.len() {
            return Err(FlashError(NorFlashErrorKind::OutOfBounds));
        }
        self.read_at(offset as usize, bytes);
        Ok(())
    }

    fn capacity(&self) -> usize {
        self.bytes.len()
    }
}

impl NorFlash for SimulatedFlash {
    const WRITE_SIZE: usize = 4;
    const ERASE_SIZE: usize = SECTOR_BYTES;

    async fn erase(&mut self, from: u32, to: u32) -> Result<(), FlashError> {
        if from as usize % SECTOR_BYTES != 0 || to as usize % SECTOR_BYTES != 0 {
            return Err(FlashError(NorFlashErrorKind::NotAligned));
        }
        for sector in from as usize / SECTOR_BYTES..to as usize / SECTOR_BYTES {
            self.erase_sector(sector);
        }
        Ok(())
    }

    async fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), FlashError> {
        if offset as usize + bytes.len() > self.bytes.len() {
            return Err(FlashError(NorFlashErrorKind::OutOfBounds));
        }
        self.write_at(offset as usize, bytes);
        Ok(())
    }
}

impl MultiwriteNorFlash for SimulatedFlash {}

// ============================================================================
// As ekv sees it
// ============================================================================

impl ekv::flash::Flash for SimulatedFlash {
    type Error = FlashError;

    fn page_count(&self) -> usize {
        self.bytes.len() / SECTOR_BYTES
    }

    async fn erase(&mut self, page: ekv::flash::PageID) -> Result<(), FlashError> {
        self.erase_sector(page.index());
        Ok(())
    }

    async fn read(&mut self, page: ekv::flash::PageID, offset: usize, data: &mut [u8]) -> Result<(), FlashError> {
        self.read_at(page.index() * SECTOR_BYTES + offset, data);
        Ok(())
    }

    async fn write(&mut self, page: ekv::flash::PageID, offset: usize, data: &[u8]) -> Result<(), FlashError> {
        self.write_at(page.index() * SECTOR_BYTES + offset, data);
        Ok(())
    }
}

// ============================================================================
// As littlefs sees it, which only reads and writes while blocking
// ============================================================================

impl embedded_storage::nor_flash::ReadNorFlash for SimulatedFlash {
    const READ_SIZE: usize = 1;

    fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), FlashError> {
        self.read_at(offset as usize, bytes);
        Ok(())
    }

    fn capacity(&self) -> usize {
        self.bytes.len()
    }
}

impl embedded_storage::nor_flash::NorFlash for SimulatedFlash {
    const WRITE_SIZE: usize = 1;
    const ERASE_SIZE: usize = SECTOR_BYTES;

    fn erase(&mut self, from: u32, to: u32) -> Result<(), FlashError> {
        for sector in from as usize / SECTOR_BYTES..to as usize / SECTOR_BYTES {
            self.erase_sector(sector);
        }
        Ok(())
    }

    fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), FlashError> {
        self.write_at(offset as usize, bytes);
        Ok(())
    }
}
