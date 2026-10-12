//! The shared library's functions: written in `sdk/neotime/src/lib.ts`, compiled into the library
//! image, and called from apps with `CallLibrary` instead of being copied into each.
//!
//! A function's number is its place in `LIBRARY_FUNCTIONS`, which only ever grows at its end, so an
//! installed app keeps calling the right function after the library is updated.

use std::sync::OnceLock;

use neotime_runtime::writer::ImageWriter;
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::{SourceType, Span};

use crate::compiler::Compiler;
use crate::scope::Reference;
use crate::types::Type;

// ============================================================================
// The functions
// ============================================================================

/// The library's functions, by their number in it.
pub const LIBRARY_FUNCTIONS: [&str; 8] = [
    "display_hour",
    "minute_of_day",
    "is_night",
    "steps_since",
    "temperature_in_unit",
    "digit_of",
    "sunday_week_of",
    "days_in_year",
];

const SDK_LIB: &str = include_str!("../../sdk/neotime/src/lib.ts");

/// How an app calls one library function.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibrarySignature {
    pub name: &'static str,
    pub number: u16,
    pub parameters: Vec<Type>,
    pub result: Type,
}

// ============================================================================
// Compiling them
// ============================================================================

/// Compiles every library function into `writer`, the library image's, numbered in order.
pub fn compile_library_functions(
    writer: ImageWriter,
) -> Result<(ImageWriter, Vec<LibrarySignature>), String> {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, SDK_LIB, SourceType::ts()).parse();
    if let Some(error) = parsed.diagnostics.errors().next() {
        return Err(format!("the library's source does not parse: {}", error.message));
    }
    let mut compiler = Compiler::new(Default::default());
    compiler.writer = writer;
    let module = compiler
        .load_module(&parsed.program)
        .map_err(|error| format!("the library's source does not load: {}", error.message))?;

    let mut signatures = Vec::new();
    for (place, name) in LIBRARY_FUNCTIONS.iter().enumerate() {
        let Some(Reference::Function(function)) = compiler.modules[module].names.get(*name).cloned() else {
            return Err(format!("the library has no function `{name}`"));
        };
        let parameter_types = function
            .parameters
            .items
            .iter()
            .map(|parameter| compiler.parameter_type(&function, parameter))
            .collect::<Option<Vec<Type>>>()
            .ok_or_else(|| format!("`{name}` must give every parameter the type of one value"))?;
        let compiled = compiler
            .compile_library_function(&function, &parameter_types, Span::default())
            .map_err(|error| format!("`{name}` does not compile: {}", error.message))?;
        if compiled.number as usize != place {
            return Err(format!(
                "`{name}` must be a function of its own, calling none of the library's others"
            ));
        }
        let result = compiled
            .result
            .ok_or_else(|| format!("`{name}` must give back a value"))?
            .value_type;
        signatures.push(LibrarySignature {
            name,
            number: place as u16,
            parameters: parameter_types,
            result,
        });
    }
    Ok((compiler.writer, signatures))
}

/// The signature of every library function, worked out once by compiling the library.
pub fn library_signatures() -> &'static [LibrarySignature] {
    static SIGNATURES: OnceLock<Vec<LibrarySignature>> = OnceLock::new();
    SIGNATURES.get_or_init(|| {
        let writer = ImageWriter::new("", "", neotime_runtime::AppKind::Library, 0);
        compile_library_functions(writer).map_or_else(|_| Vec::new(), |(_, signatures)| signatures)
    })
}

/// How an app calls the library function of this name, if there is one.
pub fn library_signature(name: &str) -> Option<&'static LibrarySignature> {
    library_signatures()
        .iter()
        .find(|signature| signature.name == name)
}

#[cfg(test)]
#[path = "shared_functions_tests.rs"]
mod shared_functions_tests;
