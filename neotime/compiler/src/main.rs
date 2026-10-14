//! The `neotime-compiler` command: compiles one app source into an image, or only checks it.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use neotime_compiler::compile_file;

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

/// `neotime-compiler library -o shared.ntlib`: writes the shared library image of every icon.
fn write_library() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(2).collect();
    let destination = match arguments.as_slice() {
        [flag, path] if flag == "-o" || flag == "--output" => PathBuf::from(path),
        [] => PathBuf::from("shared.ntlib"),
        _ => {
            eprintln!("usage: neotime-compiler library [-o <shared.ntlib>]");
            return ExitCode::from(2);
        }
    };
    let image = match neotime_compiler::library::build_library() {
        Ok(image) => image,
        Err(problem) => {
            eprintln!("cannot build the shared library: {problem}");
            return ExitCode::FAILURE;
        }
    };
    if let Err(error) = std::fs::write(&destination, &image) {
        eprintln!("{}: cannot write the library: {error}", destination.display());
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn main() -> ExitCode {
    if std::env::args().nth(1).as_deref() == Some("library") {
        return write_library();
    }
    let arguments = match read_arguments() {
        Ok(arguments) => arguments,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    let file_name = arguments.source.display().to_string();
    let (source, output) = match compile_file(&arguments.source) {
        Ok(compiled) => compiled,
        Err(problem) => {
            eprintln!("{file_name}: {problem}");
            return ExitCode::from(2);
        }
    };
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
