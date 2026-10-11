//! Compiles the ported apps and faces: the stopwatch end to end through the runtime, and every
//! other one to report which compile and the first error of each that does not.

use std::path::{Path, PathBuf};

use neotime_compiler::compile_source;
use neotime_runtime::{
    verify, AppImage, AppState, Interpreter, Property, PropertySource, SystemValue, SystemValues, Value,
    WidgetKind, WidgetTree,
};

// ============================================================================
// Helpers
// ============================================================================

struct FakeSystem {
    now: Value,
}

impl SystemValues for FakeSystem {
    fn read(&self, value: SystemValue) -> Value {
        match value {
            SystemValue::Now => self.now,
            SystemValue::Uses24HourClock => 1,
            _ => 0,
        }
    }

    fn random_number(&self) -> u32 {
        1
    }
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// Each widget's kind and properties, in the order the image stores them.
fn widgets(image: &AppImage) -> Vec<(WidgetKind, Vec<(Property, PropertySource)>)> {
    let tree = WidgetTree::new(*image);
    (0..tree.widget_count())
        .map(|index| {
            let widget = tree.widget(index).expect("a widget");
            let properties = widget
                .properties()
                .map(|entry| entry.expect("a property"))
                .map(|entry| (entry.property, entry.source))
                .collect();
            (widget.kind, properties)
        })
        .collect()
}

fn find(properties: &[(Property, PropertySource)], wanted: Property) -> Option<PropertySource> {
    properties
        .iter()
        .find(|(property, _)| *property == wanted)
        .map(|(_, source)| *source)
}

// ============================================================================
// The stopwatch, end to end
// ============================================================================

#[test]
fn the_stopwatch_runs_on_the_runtime() {
    let path = repository_root().join("apps/stopwatch/app.tsx");
    let source = std::fs::read_to_string(&path).expect("the stopwatch source");
    let output = compile_source(&source);
    let errors: Vec<String> = output
        .errors()
        .map(|error| error.render("app.tsx", &source))
        .collect();
    assert!(errors.is_empty(), "{}", errors.join("\n"));
    let bytes = output.image.expect("an image");

    let image = AppImage::parse(&bytes).expect("the image parses");
    verify(&image).expect("the image passes the install checks");
    let mut state = AppState::new(&image).expect("the state lays out");
    let mut interpreter = Interpreter::new();

    // The time is the first label; the start and pause button is the last button.
    let all = widgets(&image);
    let time_label = all
        .iter()
        .find(|(kind, _)| *kind == WidgetKind::Label)
        .and_then(|(_, properties)| find(properties, Property::Text))
        .expect("the time label");
    let start_or_pause = all
        .iter()
        .filter(|(kind, _)| *kind == WidgetKind::Button)
        .filter_map(|(_, properties)| find(properties, Property::OnClick))
        .next_back()
        .expect("the start button");
    let (PropertySource::Expression(time_expression), PropertySource::Handler(start_handler)) =
        (time_label, start_or_pause)
    else {
        panic!("the time is an expression and the button a handler");
    };

    let shown_at = |interpreter: &mut Interpreter, state: &AppState, now: Value| -> String {
        let system = FakeSystem { now };
        let value = interpreter
            .evaluate_expression(&image, time_expression, state, &system)
            .expect("the time expression runs");
        interpreter.text(&image, value).expect("text").to_string()
    };

    let start = 1_700_000_000_000;
    assert_eq!(shown_at(&mut interpreter, &state, start), "00:00");

    let outcome = interpreter
        .run_handler(
            &image,
            start_handler,
            &mut state,
            &[120, 200],
            &FakeSystem { now: start },
        )
        .expect("start runs");
    assert!(outcome.changed_variables() != 0);
    assert_eq!(shown_at(&mut interpreter, &state, start + 65_000), "01:05");

    interpreter
        .run_handler(
            &image,
            start_handler,
            &mut state,
            &[120, 200],
            &FakeSystem { now: start + 70_000 },
        )
        .expect("pause runs");
    assert_eq!(shown_at(&mut interpreter, &state, start + 500_000), "01:10");
}

// ============================================================================
// Every app and face
// ============================================================================

/// The apps and faces that are expected to compile, so a change that breaks one is caught.
const EXPECTED_TO_COMPILE: [&str; 36] = [
    "apps/alarm",
    "apps/calculator",
    "apps/dice",
    "apps/flashlight",
    "apps/haiku",
    "apps/heart",
    "apps/hello",
    "apps/level",
    "apps/metronome",
    "apps/motion",
    "apps/music",
    "apps/navigation",
    "apps/paddle",
    "apps/phone-finder",
    "apps/pomodoro",
    "apps/puzzle15",
    "apps/snake",
    "apps/sports",
    "apps/stopwatch",
    "apps/timer",
    "faces/analog",
    "faces/casio",
    "faces/digital",
    "faces/dual-clock",
    "faces/fibonacci-clock",
    "faces/infineat",
    "faces/lite-analog",
    "faces/lite-digital",
    "faces/lite-rings",
    "faces/lite-weather",
    "faces/pinetime-style",
    "faces/pride-flag",
    "faces/resistor-clock",
    "faces/terminal",
    "faces/week-clock",
    "faces/word-clock",
];

#[test]
fn every_app_and_face_compiles_or_reports_why() {
    let root = repository_root();
    let mut sources: Vec<PathBuf> = Vec::new();
    for (folder, file) in [("apps", "app.tsx"), ("faces", "face.tsx")] {
        let Ok(entries) = std::fs::read_dir(root.join(folder)) else {
            continue;
        };
        for entry in entries.flatten() {
            let source = entry.path().join(file);
            if source.is_file() {
                sources.push(source);
            }
        }
    }
    sources.sort();
    assert!(!sources.is_empty(), "no app sources found");

    let mut report = Vec::new();
    let mut unexpected_failures = Vec::new();
    for path in &sources {
        let name = path
            .parent()
            .and_then(|folder| folder.strip_prefix(&root).ok())
            .map(|folder| folder.display().to_string())
            .unwrap_or_default();
        let source = std::fs::read_to_string(path).expect("the source");
        let output = compile_source(&source);
        let first_error = output.errors().next().cloned();
        match first_error {
            None => {
                let image = output.image.expect("an image");
                let parsed = AppImage::parse(&image).expect("the image parses");
                verify(&parsed).expect("the image passes the install checks");
                report.push(format!("compiles  {name} ({} bytes)", image.len()));
            }
            Some(error) => {
                let file = path.strip_prefix(&root).unwrap_or(path).display().to_string();
                report.push(format!("fails     {}", error.render(&file, &source)));
                if EXPECTED_TO_COMPILE.contains(&name.as_str()) {
                    unexpected_failures.push(name);
                }
            }
        };
    }
    println!("{}", report.join("\n"));
    assert!(
        unexpected_failures.is_empty(),
        "these stopped compiling: {unexpected_failures:?}"
    );
}
