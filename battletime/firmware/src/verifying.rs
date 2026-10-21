//! Verifying app images once, through the verifier module, since the firmware carries no verifier of its own.
//!
//! The shell verifies an image after its install, and before it first opens one the record in
//! `/system/verified` does not hold, such as a temporary app, one installed before the verifier
//! module was on the watch, or every app once a new verifier module brings new rules.
//! It does so with the open app closed, which leaves the module its room in the heap, and shows a
//! page saying so while it works. An image that passes is recorded, so it opens at once afterwards.

use battletime_crash_record::screen::{self, Page};
use battletime_packages::{
    commands::{install_reply, module_reply, ModuleCall},
    verified::{holds, with_added, ImageMark, VERIFIED_PATH},
    AppId, PackageError, APPS_DIRECTORY,
};
use battletime_runtime::AppImage;

use super::Shell;
use crate::{apps, packages};

const VERIFIER_MODULE: &str = "verifier";
const VERIFIER_PATH: &str = "/system/modules/verifier";
const VERIFY_CALL: u32 = 0;
const VERIFIED: i32 = 0;

impl Shell {
    /// What the record of verified images belongs to: the verifier module's CRC, so only new rules check every image again.
    fn verifier_id(&self) -> Option<u32> {
        let mut header = [0; battletime_module_abi::HEADER_BYTES];
        let read = self.files.read_part_now(VERIFIER_PATH, 0, &mut header)?;
        let header = battletime_module_abi::ModuleHeader::parse(&header[..read]).ok()?;
        Some(header.crc)
    }

    /// Whether the record holds the image, so it runs without being verified now.
    pub(super) fn is_verified(&self, bytes: &[u8]) -> bool {
        let Some(verifier) = self.verifier_id() else {
            return false;
        };
        let record = self.files.read_now(VERIFIED_PATH).unwrap_or_default();
        ImageMark::of(bytes).is_some_and(|mark| holds(&record, verifier, mark))
    }

    /// Verifies the image unless the record holds it, showing that it does while it works.
    pub(super) fn verify_before_opening(
        &mut self,
        bytes: &[u8],
        installing: bool,
    ) -> Result<(), PackageError> {
        let mark = ImageMark::of(bytes).ok_or(PackageError::CodeFails)?;
        let verifier = self.verifier_id().ok_or(PackageError::CouldNotVerify)?;
        let record = self.files.read_now(VERIFIED_PATH).unwrap_or_default();
        if holds(&record, verifier, mark) {
            return Ok(());
        }
        self.show_checking(bytes, installing);
        let answer = self
            .files
            .call_module_now(VERIFIER_MODULE, VERIFY_CALL, bytes)
            .ok_or(PackageError::CouldNotVerify)?
            .map_err(|_| PackageError::CouldNotVerify)?;
        if answer != VERIFIED {
            return Err(PackageError::CodeFails);
        }
        self.files
            .write_soon(VERIFIED_PATH, &with_added(&record, verifier, mark));
        Ok(())
    }

    /// Paints the checking page straight away, since verifying holds the shell until it is done.
    fn show_checking(&mut self, bytes: &[u8], installing: bool) {
        if !self.lit {
            return;
        }
        let image = AppImage::parse(bytes).ok();
        let app_name = image
            .as_ref()
            .and_then(|image| image.string(image.name_string))
            .unwrap_or("");
        let page = Page::Checking { app_name, installing };
        self.display.lock(|display| {
            crate::status_font::draw_with_glyphs(|| {
                let _ = screen::draw(&mut *display.borrow_mut(), &page);
            });
        });
    }

    /// Runs a module call the phone asked for with every app closed, so the module has its room, answers, then shows the face.
    pub(super) fn run_module_command(&mut self, module_call: ModuleCall) {
        self.close_everything();
        let outcome = match self
            .files
            .call_module_now(&module_call.name, module_call.call, &[])
        {
            Some(Ok(answer)) => Ok(answer),
            Some(Err(failure)) => Err(failure.describe()),
            None => Err("the store is busy"),
        };
        packages::send_reply(module_reply(outcome));
        self.reload_apps();
    }

    /// Verifies a newly installed app with every app closed, answers the phone, then shows the face as after any install.
    ///
    /// An app whose code fails, or that could not be verified, stays installed but turned off.
    pub(super) fn verify_install(&mut self, id: AppId, need_bytes: u32) {
        self.close_everything();
        let outcome = match apps::read_image(self.files, APPS_DIRECTORY, &id) {
            Some(image) => {
                let verified = self.verify_before_opening(image.get(), true);
                image.release();
                verified.map(|()| need_bytes)
            }
            None => Err(PackageError::NotInstalled),
        };
        if outcome.is_err() {
            self.files.set_app_enabled_soon(&id, false);
        }
        packages::send_reply(install_reply(outcome));
        self.reload_apps();
    }
}
