//! Tests for the shared library functions: their numbers, the library image that holds them, and apps
//! that call them.

use neotime_runtime::instruction::{decode, Instruction};
use neotime_runtime::{AppImage, AppState, Interpreter, SystemValue, SystemValues, Value};

use super::{library_signature, library_signatures, LIBRARY_FUNCTIONS};
use crate::compile_source;
use crate::types::Type;

struct FakeSystem {
    now: Value,
    uses_24_hour_clock: bool,
    steps_today: Value,
}

impl SystemValues for FakeSystem {
    fn read(&self, value: SystemValue) -> Value {
        match value {
            SystemValue::Now => self.now,
            SystemValue::Uses24HourClock => self.uses_24_hour_clock as Value,
            SystemValue::UsesMetricUnits => 1,
            SystemValue::StepsToday => self.steps_today,
            SystemValue::WeatherSunrise | SystemValue::WeatherSunset => neotime_runtime::NONE,
            _ => 0,
        }
    }

    fn random_number(&self) -> u32 {
        7
    }
}

const HOUR: Value = 3_600_000;

#[test]
fn every_library_function_has_its_place_as_its_number() {
    let signatures = library_signatures();

    assert_eq!(signatures.len(), LIBRARY_FUNCTIONS.len());
    for (place, signature) in signatures.iter().enumerate() {
        assert_eq!(signature.number as usize, place);
        assert_eq!(signature.name, LIBRARY_FUNCTIONS[place]);
    }
    assert_eq!(
        library_signature("digit_of").unwrap().parameters,
        [Type::NUMBER, Type::NUMBER]
    );
    assert_eq!(library_signature("is_night").unwrap().result, Type::BOOL);
    assert!(library_signature("no_such_function").is_none());
}

#[test]
fn the_library_image_holds_the_functions_and_passes_the_install_checks() {
    let bytes = crate::library::build_library().unwrap();
    let library = AppImage::parse(&bytes).unwrap();

    assert_eq!(library.function_count() as usize, LIBRARY_FUNCTIONS.len());
    assert_eq!(neotime_runtime::verify(&library), Ok(()));
}

/// A label showing `expression`, compiled into an app.
fn app_showing(expression: &str) -> Vec<u8> {
    let source = format!(
        r#"
import {{ define_app, now, format_number }} from "neotime";
import {{ display_hour, digit_of, is_night, steps_since }} from "neotime/lib";
import {{ Label, Screen }} from "neotime/widgets";

function Main() {{
  return <Screen><Label>{{`${{{expression}}}`}}</Label></Screen>;
}}

export default define_app({{ id: "test", name: "Test", screens: {{ main: Main }} }});
"#
    );
    let output = compile_source(&source);
    let errors: Vec<String> = output
        .errors()
        .map(|error| error.render("test.tsx", &source))
        .collect();
    assert!(errors.is_empty(), "{}", errors.join("\n"));
    output.image.expect("an image")
}

/// What an app's only expression shows when run with the library.
fn shown(image_bytes: &[u8], system: &FakeSystem) -> String {
    let library_bytes = crate::library::build_library().unwrap();
    let library = AppImage::parse(&library_bytes).unwrap();
    let image = AppImage::parse(image_bytes).unwrap();
    let state = AppState::new(&image).unwrap();
    let mut interpreter = Interpreter::new();
    let value = interpreter
        .evaluate_expression_with_library(&image, Some(&library), 0, &state, system)
        .unwrap();
    interpreter.text(&image, value).unwrap().to_string()
}

#[test]
fn an_app_calls_a_library_function_instead_of_carrying_it() {
    let image_bytes = app_showing("display_hour(now.value)");
    let image = AppImage::parse(&image_bytes).unwrap();
    let code = image.expression_code(0).unwrap();
    let mut calls = 0;
    let mut offset = 0;
    while offset < code.len() {
        let (instruction, next) = decode(code, offset).unwrap();
        if let Instruction::CallLibrary {
            function: 0,
            arguments: 1,
            results: 1,
        } = instruction
        {
            calls += 1;
        }
        offset = next;
    }
    let afternoon = FakeSystem {
        now: 15 * HOUR,
        uses_24_hour_clock: false,
        steps_today: 0,
    };

    assert_eq!(calls, 1);
    assert_eq!(image.function_count(), 0);
    assert_eq!(shown(&image_bytes, &afternoon), "3");
    let on_a_24_hour_clock = FakeSystem {
        uses_24_hour_clock: true,
        ..afternoon
    };
    assert_eq!(shown(&image_bytes, &on_a_24_hour_clock), "15");
}

#[test]
fn library_functions_take_several_arguments_and_give_back_booleans() {
    let system = FakeSystem {
        now: 22 * HOUR,
        uses_24_hour_clock: true,
        steps_today: 500,
    };

    assert_eq!(shown(&app_showing("digit_of(4321, 100)"), &system), "3");
    assert_eq!(shown(&app_showing("is_night(now.value) ? 1 : 0"), &system), "1");
    assert_eq!(shown(&app_showing("steps_since(200)"), &system), "300");
}
