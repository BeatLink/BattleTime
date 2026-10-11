//! The NeoTime app compiler: turns an app written as Preact components with signals, in a strict
//! TypeScript subset, into an app image the watch's runtime reads.
//!
//! The source is parsed with oxc and compiled straight from its syntax tree. docs/compiler.md
//! describes the subset, the type rules and the number mappings.

pub mod calls;
pub mod compiler;
pub mod constants;
pub mod derived;
pub mod diagnostics;
pub mod emitter;
pub mod expressions;
pub mod images;
pub mod library;
pub mod permissions;
pub mod scope;
pub mod statements;
pub mod subscriptions;
pub mod types;
pub mod widgets;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::{SourceType, Span};

use crate::compiler::Compiler;
use crate::diagnostics::{Diagnostic, Severity};
use crate::types::Range;

pub use crate::diagnostics::Diagnostic as CompileDiagnostic;

// ============================================================================
// Compiling
// ============================================================================

/// How many passes may refine the ranges of values before every array is made 8 bytes wide.
const MOST_PASSES: usize = 6;
/// How many times the passes start again after giving up on the variables still growing.
const MOST_ROUNDS: usize = 3;

/// The shared library's components, written in the app subset; they are inlined like the app's own.
const SDK_COMPONENTS: &str = include_str!("../../sdk/neotime/src/components.tsx");

/// The result of compiling one file: the image, if there were no errors, and every diagnostic.
pub struct Output {
    pub image: Option<Vec<u8>>,
    pub diagnostics: Vec<Diagnostic>,
}

impl Output {
    pub fn errors(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity == Severity::Error)
    }

    pub fn warnings(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity == Severity::Warning)
    }
}

/// Compiles an app's source text into an image; it may not use images of its own.
pub fn compile_source(source: &str) -> Output {
    compile_source_in(source, None)
}

/// Compiles an app's source file, whose own images are read relative to its folder.
pub fn compile_file(path: &Path) -> Result<(String, Output), String> {
    let source = std::fs::read_to_string(path).map_err(|error| format!("cannot read the file: {error}"))?;
    let directory = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let output = compile_source_in(&source, Some(directory));
    Ok((source, output))
}

/// Compiles source text whose own images, if any, are relative to `directory`.
pub fn compile_source_in(source: &str, directory: Option<PathBuf>) -> Output {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::tsx()).parse();
    let parse_errors: Vec<Diagnostic> = parsed
        .diagnostics
        .errors()
        .map(|error| {
            let offset = error.labels.first().map_or(0, |label| label.offset());
            Diagnostic::error(Span::new(offset, offset), error.message.to_string())
        })
        .collect();
    if !parse_errors.is_empty() {
        return Output {
            image: None,
            diagnostics: parse_errors,
        };
    }
    let program = &parsed.program;
    let components = Parser::new(&allocator, SDK_COMPONENTS, SourceType::tsx()).parse();
    let components = &components.program;

    // Each pass assumes what the pass before found each variable holds, until nothing new turns up.
    // A variable still growing when the passes run out, such as a counter, is taken to hold anything,
    // and the passes start again, so it does not widen every other array with it.
    let mut assumed: HashMap<String, Range> = HashMap::new();
    let mut converged = false;
    'rounds: for _ in 0..MOST_ROUNDS {
        let mut still_growing: Vec<String> = Vec::new();
        for _ in 0..MOST_PASSES {
            let observed = match run_pass(program, components, assumed.clone(), &directory) {
                Ok(compiler) => compiler.observed_ranges(),
                Err(error) => {
                    return Output {
                        image: None,
                        diagnostics: vec![error],
                    }
                }
            };
            still_growing = observed
                .iter()
                .filter(|(key, range)| {
                    !assumed
                        .get(*key)
                        .is_some_and(|assumed| assumed.lowest <= range.lowest && assumed.highest >= range.highest)
                })
                .map(|(key, _)| key.clone())
                .collect();
            if still_growing.is_empty() {
                converged = true;
                break 'rounds;
            }
            for (key, range) in observed {
                let entry = assumed.entry(key).or_insert(range);
                *entry = entry.union(range);
            }
        }
        for key in still_growing {
            assumed.insert(key, Range::ANY);
        }
    }
    if !converged {
        for range in assumed.values_mut() {
            *range = Range::ANY;
        }
    }
    match run_pass(program, components, assumed, &directory) {
        Ok(compiler) => {
            let image = compiler.writer.build();
            let mut diagnostics = compiler.warnings;
            match check_image(&image) {
                Ok(()) => Output {
                    image: Some(image),
                    diagnostics,
                },
                Err(problem) => {
                    diagnostics.push(Diagnostic::error(
                        Span::default(),
                        format!("internal compiler error: the image fails the runtime's checks: {problem}"),
                    ));
                    Output {
                        image: None,
                        diagnostics,
                    }
                }
            }
        }
        Err(error) => Output {
            image: None,
            diagnostics: vec![error],
        },
    }
}

fn run_pass<'a>(
    program: &'a oxc_ast::ast::Program<'a>,
    components: &'a oxc_ast::ast::Program<'a>,
    assumed: HashMap<String, Range>,
    directory: &Option<PathBuf>,
) -> Result<Compiler<'a>, Diagnostic> {
    let mut compiler = Compiler::new(assumed);
    compiler.source_directory = directory.clone();
    let module = compiler.load_module(program)?;
    let components_module = compiler.load_module(components).map_err(|error| {
        Diagnostic::error(
            Span::default(),
            format!(
                "internal compiler error in the neotime components: {}",
                error.message
            ),
        )
    })?;
    compiler.components_module = Some(components_module);
    compiler.compile_app(module, program.span)?;
    Ok(compiler)
}

/// Runs the checks the watch runs when an image is installed.
pub fn check_image(image: &[u8]) -> Result<(), String> {
    let parsed = neotime_runtime::AppImage::parse(image).map_err(|error| format!("{error:?}"))?;
    neotime_runtime::verify(&parsed).map_err(|error| format!("{error:?}"))
}

#[cfg(test)]
mod tests;
