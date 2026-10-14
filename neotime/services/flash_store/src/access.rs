//! Files by path behind a trait, so services such as BLEFS and the package manager need not name the store's
//! sizes, and give one error type whatever the flash driver.

use embedded_storage_async::nor_flash::NorFlash;

use crate::{FlashStore, StoreError, SECTOR_BYTES};

/// Why a file call failed, without the flash driver's own error type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileError {
    /// No room: the file area, its table or the records ring is full.
    Full,
    TooLarge,
    PathTooLong,
    /// Another upload is in progress, or there is none to continue.
    Busy,
    /// The flash failed, or what it holds does not check out.
    Damaged,
}

impl<FlashError> From<StoreError<FlashError>> for FileError {
    fn from(error: StoreError<FlashError>) -> FileError {
        match error {
            StoreError::Full
            | StoreError::KeyTableFull
            | StoreError::FileTableFull
            | StoreError::FileAreaFull
            | StoreError::OverOwnerLimit => FileError::Full,
            StoreError::TooLarge => FileError::TooLarge,
            StoreError::PathTooLong => FileError::PathTooLong,
            StoreError::UploadInProgress | StoreError::NoUpload => FileError::Busy,
            StoreError::Flash(_)
            | StoreError::OtherFormatVersion
            | StoreError::LayoutDoesNotFit
            | StoreError::Corrupted => FileError::Damaged,
        }
    }
}

/// Files by path, as the services use them.
#[allow(async_fn_in_trait)]
pub trait FileAccess {
    /// Reads from `offset` into the buffer, as much as it holds; nothing when the file is missing.
    async fn read_file(
        &mut self,
        path: &str,
        offset: usize,
        buffer: &mut [u8],
    ) -> Result<Option<usize>, FileError>;
    async fn file_length(&mut self, path: &str) -> Result<Option<usize>, FileError>;
    async fn write_file(&mut self, path: &str, data: &[u8]) -> Result<(), FileError>;
    /// Starts a file written a part at a time; it replaces the old one only at `finish_file`.
    async fn begin_file(&mut self, path: &str) -> Result<(), FileError>;
    async fn write_file_part(&mut self, data: &[u8]) -> Result<(), FileError>;
    async fn finish_file(&mut self) -> Result<(), FileError>;
    fn abandon_file(&mut self);
    /// The path of the upload in progress and how many bytes it holds so far.
    fn upload_progress(&self) -> Option<(&str, usize)>;
    async fn delete_file(&mut self, path: &str) -> Result<(), FileError>;
    /// Gives `visit` every file's path that starts with `prefix`, each once.
    async fn for_each_file(&mut self, prefix: &str, visit: &mut dyn FnMut(&str)) -> Result<(), FileError>;
    /// Gives `visit` every file's path that starts with `prefix` and its length in bytes, each once.
    async fn for_each_file_with_length(
        &mut self,
        prefix: &str,
        visit: &mut dyn FnMut(&str, usize),
    ) -> Result<(), FileError>;
    /// The bytes of the file area, and how many are free, counted in whole sectors.
    fn total_bytes(&self) -> usize;
    fn free_bytes(&self) -> usize;
}

impl<Flash: NorFlash, const MOST_FILES: usize, const MOST_RECORD_KEYS: usize, const RING_SECTORS: usize>
    FileAccess for FlashStore<Flash, MOST_FILES, MOST_RECORD_KEYS, RING_SECTORS>
{
    async fn read_file(
        &mut self,
        path: &str,
        offset: usize,
        buffer: &mut [u8],
    ) -> Result<Option<usize>, FileError> {
        Ok(FlashStore::read_file(self, path, offset, buffer).await?)
    }

    async fn file_length(&mut self, path: &str) -> Result<Option<usize>, FileError> {
        Ok(FlashStore::file_length(self, path).await?)
    }

    async fn write_file(&mut self, path: &str, data: &[u8]) -> Result<(), FileError> {
        Ok(FlashStore::write_file(self, path, data).await?)
    }

    async fn begin_file(&mut self, path: &str) -> Result<(), FileError> {
        Ok(FlashStore::begin_file(self, path).await?)
    }

    async fn write_file_part(&mut self, data: &[u8]) -> Result<(), FileError> {
        Ok(FlashStore::write_file_part(self, data).await?)
    }

    async fn finish_file(&mut self) -> Result<(), FileError> {
        Ok(FlashStore::finish_file(self).await?)
    }

    fn abandon_file(&mut self) {
        FlashStore::abandon_file(self)
    }

    fn upload_progress(&self) -> Option<(&str, usize)> {
        FlashStore::upload_progress(self)
    }

    async fn delete_file(&mut self, path: &str) -> Result<(), FileError> {
        Ok(FlashStore::delete_file(self, path).await?)
    }

    async fn for_each_file(&mut self, prefix: &str, visit: &mut dyn FnMut(&str)) -> Result<(), FileError> {
        Ok(FlashStore::for_each_file(self, prefix, visit).await?)
    }

    async fn for_each_file_with_length(
        &mut self,
        prefix: &str,
        visit: &mut dyn FnMut(&str, usize),
    ) -> Result<(), FileError> {
        Ok(FlashStore::for_each_file_with_length(self, prefix, visit).await?)
    }

    fn total_bytes(&self) -> usize {
        self.file_sectors() * SECTOR_BYTES as usize
    }

    fn free_bytes(&self) -> usize {
        self.free_file_sectors() * SECTOR_BYTES as usize
    }
}
