extern crate std;

use std::{vec, vec::Vec};

use embassy_futures::block_on;
use embedded_storage_async::nor_flash::{NorFlash as AsyncNorFlash, ReadNorFlash as AsyncReadNorFlash};

use embedded_hal::spi::ErrorType as SpiErrorType;

use super::*;

// ============================================================================
// Fake chip
// ============================================================================

struct FakeChip {
    memory: Vec<u8>,
    chip_id: ChipId,
    asleep: bool,
    writing_enabled: bool,
    busy_polls_after_each_write: u8,
    busy_polls_left: u8,
    status_polls: usize,
    programmed_addresses: Vec<u32>,
    erased_addresses: Vec<u32>,
}

impl FakeChip {
    fn new() -> FakeChip {
        FakeChip {
            memory: vec![0xFF; CAPACITY_IN_BYTES],
            chip_id: XT25F32B_CHIP_ID,
            asleep: true,
            writing_enabled: false,
            busy_polls_after_each_write: 0,
            busy_polls_left: 0,
            status_polls: 0,
            programmed_addresses: Vec::new(),
            erased_addresses: Vec::new(),
        }
    }

    fn address_from(command: &[u8]) -> u32 {
        u32::from_be_bytes([0, command[1], command[2], command[3]])
    }

    fn take_write_permission(&mut self) {
        assert!(
            self.writing_enabled,
            "the chip ignores writes without WRITE_ENABLE"
        );
        self.writing_enabled = false;
        self.busy_polls_left = self.busy_polls_after_each_write;
    }

    fn program(&mut self, address: u32, data: &[u8]) {
        let start = address as usize;
        let first_page = start / PROGRAM_PAGE_SIZE;
        let last_page = (start + data.len() - 1) / PROGRAM_PAGE_SIZE;
        assert_eq!(
            first_page, last_page,
            "a page program wraps inside its page on the real chip"
        );

        self.take_write_permission();
        for (memory_byte, new_byte) in self.memory[start..start + data.len()].iter_mut().zip(data) {
            *memory_byte &= new_byte;
        }
        self.programmed_addresses.push(address);
    }

    fn erase(&mut self, address: u32) {
        let start = address as usize;
        assert_eq!(
            start % SECTOR_SIZE,
            0,
            "sector erase address must be sector aligned"
        );

        self.take_write_permission();
        self.memory[start..start + SECTOR_SIZE].fill(0xFF);
        self.erased_addresses.push(address);
    }
}

impl SpiErrorType for FakeChip {
    type Error = core::convert::Infallible;
}

impl SpiDevice for FakeChip {
    fn transaction(&mut self, operations: &mut [Operation<'_, u8>]) -> Result<(), Self::Error> {
        let command = match &operations[0] {
            Operation::Write(command) => command.to_vec(),
            _ => panic!("every transaction starts by writing a command"),
        };

        if self.asleep {
            if command == [RELEASE_POWER_DOWN] {
                self.asleep = false;
            }
            return Ok(());
        }
        assert!(
            self.busy_polls_left == 0 || command[0] == READ_STATUS,
            "the chip ignores every command but a status read while it is busy"
        );

        match (command[0], &mut operations[1..]) {
            (READ_CHIP_ID, [Operation::Read(id_bytes)]) => {
                id_bytes.copy_from_slice(&[
                    self.chip_id.manufacturer,
                    self.chip_id.memory_type,
                    self.chip_id.capacity,
                ]);
            }
            (READ_STATUS, [Operation::Read(status)]) => {
                self.status_polls += 1;
                if self.busy_polls_left > 0 {
                    self.busy_polls_left -= 1;
                    status[0] = WRITE_IN_PROGRESS_BIT;
                } else {
                    status[0] = 0;
                }
            }
            (READ_DATA, [Operation::Read(bytes)]) => {
                let start = Self::address_from(&command) as usize;
                bytes.copy_from_slice(&self.memory[start..start + bytes.len()]);
            }
            (PAGE_PROGRAM, [Operation::Write(data)]) => {
                let data = data.to_vec();
                self.program(Self::address_from(&command), &data);
            }
            (SECTOR_ERASE, []) => self.erase(Self::address_from(&command)),
            (WRITE_ENABLE, []) => self.writing_enabled = true,
            (DEEP_POWER_DOWN, []) => self.asleep = true,
            (RELEASE_POWER_DOWN, []) => {}
            (unknown, _) => panic!("unexpected command {unknown:#04x}"),
        }
        Ok(())
    }
}

impl embedded_hal_async::spi::SpiDevice for FakeChip {
    async fn transaction(&mut self, operations: &mut [Operation<'_, u8>]) -> Result<(), Self::Error> {
        <FakeChip as SpiDevice>::transaction(self, operations)
    }
}

#[derive(Default)]
struct CountingDelay {
    microseconds_waited: u32,
}

impl embedded_hal_async::delay::DelayNs for CountingDelay {
    async fn delay_ns(&mut self, nanoseconds: u32) {
        self.microseconds_waited += nanoseconds / 1000;
    }
}

impl DelayNs for CountingDelay {
    fn delay_ns(&mut self, nanoseconds: u32) {
        self.microseconds_waited += nanoseconds / 1000;
    }
}

fn started_flash() -> ExternalFlash<FakeChip, CountingDelay> {
    ExternalFlash::new(FakeChip::new(), CountingDelay::default()).unwrap()
}

// ============================================================================
// Starting
// ============================================================================

#[test]
fn starting_wakes_the_chip_and_checks_its_id() {
    let flash = started_flash();

    let parts = flash.release();

    assert!(!parts.spi.asleep);
    assert_eq!(parts.delay.microseconds_waited, WAKE_UP_TIME_IN_MICROSECONDS);
}

#[test]
fn accepts_a_four_megabyte_chip_from_another_maker() {
    let mut chip = FakeChip::new();
    chip.chip_id = ChipId {
        manufacturer: 0xEF,
        memory_type: 0x40,
        capacity: 0x16,
    };

    let result = ExternalFlash::new(chip, CountingDelay::default());

    assert!(result.is_ok());
}

#[test]
fn refuses_a_chip_of_another_size() {
    let mut chip = FakeChip::new();
    let smaller_chip_id = ChipId {
        manufacturer: 0x0B,
        memory_type: 0x40,
        capacity: 0x15,
    };
    chip.chip_id = smaller_chip_id;

    let result = ExternalFlash::new(chip, CountingDelay::default());

    assert!(matches!(result, Err(Error::UnknownChip(id)) if id == smaller_chip_id));
}

#[test]
fn refuses_a_chip_that_does_not_answer() {
    let mut chip = FakeChip::new();
    chip.chip_id = ChipId {
        manufacturer: 0xFF,
        memory_type: 0xFF,
        capacity: 0xFF,
    };

    let result = ExternalFlash::new(chip, CountingDelay::default());

    assert!(matches!(result, Err(Error::UnknownChip(_))));
}

#[test]
fn sleeping_puts_the_chip_in_deep_power_down() {
    let mut flash = started_flash();

    flash.sleep().unwrap();

    assert!(flash.release().spi.asleep);
}

// ============================================================================
// Reading and writing
// ============================================================================

#[test]
fn reads_back_what_was_written() {
    let mut flash = started_flash();
    let written = [1, 2, 3, 4, 5];

    flash.write(0x1234, &written).unwrap();
    let mut read_back = [0; 5];
    flash.read(0x1234, &mut read_back).unwrap();

    assert_eq!(read_back, written);
}

#[test]
fn splits_a_write_where_it_crosses_a_program_page() {
    let mut flash = started_flash();
    let written: Vec<u8> = (0..600).map(|index| index as u8).collect();

    flash.write(0x10F0, &written).unwrap();
    let mut read_back = vec![0; 600];
    flash.read(0x10F0, &mut read_back).unwrap();

    assert_eq!(read_back, written);
    assert_eq!(
        flash.release().spi.programmed_addresses,
        [0x10F0, 0x1100, 0x1200, 0x1300]
    );
}

#[test]
fn waits_between_polls_while_the_chip_is_busy() {
    let mut chip = FakeChip::new();
    chip.busy_polls_after_each_write = 3;
    let mut flash = ExternalFlash::new(chip, CountingDelay::default()).unwrap();

    flash.write(0, &[0]).unwrap();
    let parts = flash.release();

    assert_eq!(parts.spi.status_polls, 4);
    assert_eq!(
        parts.delay.microseconds_waited,
        WAKE_UP_TIME_IN_MICROSECONDS + 3 * BUSY_POLL_INTERVAL_IN_MICROSECONDS
    );
}

#[test]
fn rejects_a_read_beyond_the_end() {
    let mut flash = started_flash();
    let mut bytes = [0; 2];

    let result = flash.read(CAPACITY_IN_BYTES as u32 - 1, &mut bytes);

    assert_eq!(result, Err(Error::BadRange(NorFlashErrorKind::OutOfBounds)));
}

// ============================================================================
// Erasing
// ============================================================================

#[test]
fn erasing_clears_each_sector_in_the_range() {
    let mut flash = started_flash();
    flash.write(0x2000, &[0; 3 * SECTOR_SIZE]).unwrap();

    flash.erase(0x3000, 0x5000).unwrap();
    let mut read_back = vec![0; 3 * SECTOR_SIZE];
    flash.read(0x2000, &mut read_back).unwrap();

    assert!(read_back[..SECTOR_SIZE].iter().all(|byte| *byte == 0));
    assert!(read_back[SECTOR_SIZE..].iter().all(|byte| *byte == 0xFF));
    assert_eq!(flash.release().spi.erased_addresses, [0x3000, 0x4000]);
}

#[test]
fn a_background_erase_returns_at_once_and_reports_when_the_chip_is_done() {
    let mut chip = FakeChip::new();
    chip.busy_polls_after_each_write = 2;
    let mut flash = ExternalFlash::new(chip, CountingDelay::default()).unwrap();

    flash.start_erasing_sector(0x7000).unwrap();
    let answers = [
        flash.erase_has_finished().unwrap(),
        flash.erase_has_finished().unwrap(),
        flash.erase_has_finished().unwrap(),
        flash.erase_has_finished().unwrap(),
    ];

    assert_eq!(answers, [false, false, true, true]);
    let parts = flash.release();
    assert_eq!(parts.spi.erased_addresses, [0x7000]);
    assert_eq!(parts.spi.status_polls, 3);
    assert_eq!(parts.delay.microseconds_waited, WAKE_UP_TIME_IN_MICROSECONDS);
}

#[test]
fn reading_or_writing_during_a_background_erase_waits_for_it_first() {
    let mut chip = FakeChip::new();
    chip.busy_polls_after_each_write = 3;
    let mut flash = ExternalFlash::new(chip, CountingDelay::default()).unwrap();
    flash.write(0x7000, &[0x12; 4]).unwrap();

    flash.start_erasing_sector(0x7000).unwrap();
    let mut read_back = [0; 4];
    flash.read(0x7000, &mut read_back).unwrap();
    flash.start_erasing_sector(0x8000).unwrap();
    flash.write(0x7000, &[0x34]).unwrap();

    assert_eq!(read_back, [0xFF; 4]);
    assert!(flash.erase_has_finished().unwrap());
    let mut written = [0];
    flash.read(0x7000, &mut written).unwrap();
    assert_eq!(written, [0x34]);
}

#[test]
fn a_background_erase_must_be_sector_aligned() {
    let mut flash = started_flash();

    let result = flash.start_erasing_sector(0x7001);

    assert_eq!(result, Err(Error::BadRange(NorFlashErrorKind::NotAligned)));
}

#[test]
fn rejects_an_erase_that_is_not_sector_aligned() {
    let mut flash = started_flash();

    let result = flash.erase(0x100, 0x1000);

    assert_eq!(result, Err(Error::BadRange(NorFlashErrorKind::NotAligned)));
}

// ============================================================================
// Async driver
// ============================================================================

fn started_async_flash(chip: FakeChip) -> AsyncExternalFlash<FakeChip, CountingDelay> {
    block_on(AsyncExternalFlash::new(chip, CountingDelay::default())).unwrap()
}

#[test]
fn async_starting_wakes_the_chip_and_waits_for_it() {
    let flash = started_async_flash(FakeChip::new());

    let parts = flash.release();

    assert!(!parts.spi.asleep);
    assert_eq!(parts.delay.microseconds_waited, WAKE_UP_TIME_IN_MICROSECONDS);
}

#[test]
fn async_driver_refuses_a_chip_of_another_size() {
    let mut chip = FakeChip::new();
    chip.chip_id = ChipId {
        manufacturer: 0x0B,
        memory_type: 0x40,
        capacity: 0x15,
    };

    let result = block_on(AsyncExternalFlash::new(chip, CountingDelay::default()));

    assert!(matches!(result, Err(Error::UnknownChip(_))));
}

#[test]
fn async_driver_splits_writes_at_program_pages_and_reads_them_back() {
    let mut flash = started_async_flash(FakeChip::new());
    let written: Vec<u8> = (0..600).map(|index| index as u8).collect();

    block_on(flash.write(0x10F0, &written)).unwrap();
    let mut read_back = vec![0; 600];
    block_on(flash.read(0x10F0, &mut read_back)).unwrap();

    assert_eq!(read_back, written);
    assert_eq!(
        flash.release().spi.programmed_addresses,
        [0x10F0, 0x1100, 0x1200, 0x1300]
    );
}

#[test]
fn async_driver_waits_between_polls_while_the_chip_is_busy() {
    let mut chip = FakeChip::new();
    chip.busy_polls_after_each_write = 3;
    let mut flash = started_async_flash(chip);

    block_on(flash.erase(0x2000, 0x3000)).unwrap();
    let parts = flash.release();

    assert_eq!(parts.spi.erased_addresses, [0x2000]);
    assert_eq!(parts.spi.status_polls, 4);
    assert_eq!(
        parts.delay.microseconds_waited,
        WAKE_UP_TIME_IN_MICROSECONDS + 3 * BUSY_POLL_INTERVAL_IN_MICROSECONDS
    );
}

#[test]
fn async_driver_rejects_an_erase_that_is_not_sector_aligned() {
    let mut flash = started_async_flash(FakeChip::new());

    let result = block_on(flash.erase(0x100, 0x1000));

    assert_eq!(result, Err(Error::BadRange(NorFlashErrorKind::NotAligned)));
}

#[test]
fn async_driver_rejects_a_write_beyond_the_end() {
    let mut flash = started_async_flash(FakeChip::new());

    let result = block_on(flash.write(CAPACITY_IN_BYTES as u32 - 1, &[0, 0]));

    assert_eq!(result, Err(Error::BadRange(NorFlashErrorKind::OutOfBounds)));
}
