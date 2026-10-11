//! The launcher, written as an app image from the list of installed apps when the watch starts.

use alloc::vec::Vec;

use neotime_runtime::{
    instruction::Instruction,
    writer::{CodeWriter, ImageWriter},
    AppImage, AppKind, EffectKind, Property, PropertySource, WidgetKind,
};

const ROW_HEIGHT: i32 = 48;

/// The shared library version the launcher is written for.
const SHARED_LIBRARY_VERSION: u16 = 1;

/// A scrolling list with a button for each app; a tap calls `open_app` with the app's id.
///
/// Images that do not parse are left out.
pub fn launcher_image(apps: &[&[u8]]) -> Vec<u8> {
    use Instruction::*;

    let mut writer = ImageWriter::new("launcher", "Launcher", AppKind::App, SHARED_LIBRARY_VERSION);
    let screen_name = writer.string("main");
    let mut buttons = Vec::new();
    for bytes in apps {
        let Ok(image) = AppImage::parse(bytes) else {
            continue;
        };
        let (Some(id), Some(name)) = (image.string(image.id_string), image.string(image.name_string)) else {
            continue;
        };
        let id = writer.string(id);
        let name = writer.string(name);
        let mut code = CodeWriter::new();
        code.emit(PushText(id)).emit(Effect(EffectKind::OpenApp));
        let Ok(code) = code.finish() else {
            continue;
        };
        buttons.push((name, writer.handler(code)));
    }

    let constant = PropertySource::Constant;
    writer.widget(
        WidgetKind::List,
        buttons.len() as u8,
        &[(Property::ScreenName, constant(screen_name as i32))],
    );
    for (name, handler) in buttons {
        writer.widget(
            WidgetKind::Button,
            0,
            &[
                (Property::Text, constant(name as i32)),
                (Property::Height, constant(ROW_HEIGHT)),
                (Property::OnClick, PropertySource::Handler(handler)),
            ],
        );
    }
    writer.build()
}
