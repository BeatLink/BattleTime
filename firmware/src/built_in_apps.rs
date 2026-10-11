// Written by tools/build_watch_apps.sh.
pub static FACE: &[u8] = include_bytes!("../apps/face.ntapp");
pub static LAUNCHER: &[u8] = include_bytes!("../apps/launcher.ntapp");
pub static LIBRARY: &[u8] = include_bytes!("../apps/shared.ntlib");
pub static APPS: [&[u8]; 9] = [
    include_bytes!("../apps/stopwatch.ntapp"),
    include_bytes!("../apps/timer.ntapp"),
    include_bytes!("../apps/flashlight.ntapp"),
    include_bytes!("../apps/dice.ntapp"),
    include_bytes!("../apps/calculator.ntapp"),
    include_bytes!("../apps/snake.ntapp"),
    include_bytes!("../apps/puzzle15.ntapp"),
    include_bytes!("../apps/steps.ntapp"),
    include_bytes!("../apps/heart.ntapp"),
];
