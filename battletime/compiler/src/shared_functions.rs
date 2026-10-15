//! The shared library's functions: written in `sdk/battletime/src/lib.ts`, compiled into the library
//! image, and called from apps with `CallLibrary` instead of being copied into each.
//!
//! A function's number is its place in `LIBRARY_FUNCTIONS`, which only ever grows at its end, so an
//! installed app keeps calling the right function after the library is updated.

use std::sync::OnceLock;

use battletime_runtime::writer::ImageWriter;
use oxc_allocator::Allocator;
use oxc_ast::ast::{Declaration, Program, Statement, TSType};
use oxc_parser::Parser;
use oxc_span::{SourceType, Span};

use crate::compiler::Compiler;
use crate::scope::Reference;
use crate::types::Type;

// ============================================================================
// The functions
// ============================================================================

/// The library's functions, by their number in it.
pub const LIBRARY_FUNCTIONS: [&str; 13] = [
    "display_hour",
    "minute_of_day",
    "is_night",
    "steps_since",
    "temperature_in_unit",
    "digit_of",
    "sunday_week_of",
    "days_in_year",
    "weather_icon_at",
    "weather_condition_name",
    "temperature_color",
    "format_time_of_day",
    "day_mask_name",
];

const SDK_LIB: &str = include_str!("../../sdk/battletime/src/lib.ts");

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
        let result_type = declared_result_type(&parsed.program, name)
            .and_then(|annotation| compiler.declared_type(annotation, module).ok().flatten())
            .filter(|declared| !declared.is_array)
            .map(|declared| declared.value_type);
        let compiled = compiler
            .compile_library_function(&function, &parameter_types, result_type, Span::default())
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

/// The return type written on an exported library function, such as `Color`, so its literals compile to it.
fn declared_result_type<'a>(program: &'a Program<'a>, name: &str) -> Option<&'a TSType<'a>> {
    program.body.iter().find_map(|statement| match statement {
        Statement::ExportDeclaration(export) => match &export.declaration {
            Declaration::FunctionDeclaration(function)
                if function.id.as_ref().is_some_and(|id| id.name == name) =>
            {
                function
                    .return_type
                    .as_ref()
                    .map(|annotation| &annotation.type_annotation)
            }
            _ => None,
        },
        _ => None,
    })
}

/// The signature of every library function, worked out once by compiling the library.
pub fn library_signatures() -> &'static [LibrarySignature] {
    static SIGNATURES: OnceLock<Vec<LibrarySignature>> = OnceLock::new();
    SIGNATURES.get_or_init(|| {
        let writer = ImageWriter::new("", "", battletime_runtime::AppKind::Library, 0);
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
