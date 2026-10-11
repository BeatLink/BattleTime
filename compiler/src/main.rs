//! The `neotime-compiler` command: compiles one app source into an image, or only checks it.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use neotime_compiler::compile_source;

// ============================================================================
// Command line
// ============================================================================

const USAGE: &str = "usage: neotime-compiler <source.tsx> [-o <out.ntapp>] [--check]";

struct Arguments {
    source: PathBuf,
    output: Option<PathBuf>,
    only_check: bool,
}

fn read_arguments() -> Result<Arguments, String> {
    let mut source = None;
    let mut output = None;
    let mut only_check = false;
    let mut arguments = std::env::args().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "-o" | "--output" => output = Some(PathBuf::from(arguments.next().ok_or(USAGE)?)),
            "--check" => only_check = true,
            "-h" | "--help" => return Err(USAGE.to_string()),
            other if other.starts_with('-') => return Err(format!("unknown option `{other}`\n{USAGE}")),
            other if source.is_none() => source = Some(PathBuf::from(other)),
            _ => return Err(USAGE.to_string()),
        }
    }
    Ok(Arguments {
        source: source.ok_or(USAGE)?,
        output,
        only_check,
    })
}

// ============================================================================
// Main
// ============================================================================

fn main() -> ExitCode {
    let arguments = match read_arguments() {
        Ok(arguments) => arguments,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    let file_name = arguments.source.display().to_string();
    let source = match std::fs::read_to_string(&arguments.source) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("{file_name}: cannot read the file: {error}");
            return ExitCode::from(2);
        }
    };

    let output = compile_source(&source);
    for diagnostic in &output.diagnostics {
        eprintln!("{}", diagnostic.render(&file_name, &source));
    }
    let Some(image) = output.image else {
        return ExitCode::FAILURE;
    };
    if arguments.only_check {
        return ExitCode::SUCCESS;
    }

    let destination = arguments
        .output
        .unwrap_or_else(|| default_output(&arguments.source));
    if let Err(error) = std::fs::write(&destination, &image) {
        eprintln!("{}: cannot write the image: {error}", destination.display());
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// The source's path with the `.ntapp` extension.
fn default_output(source: &Path) -> PathBuf {
    source.with_extension("ntapp")
}
