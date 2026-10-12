//! Where the apps and faces live, compiling them from source, and the shared library they use.

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use neotime_app_host::OpenError;

// ============================================================================
// Packages
// ============================================================================

/// Whether a package is an app, in `apps/<id>/app.tsx`, or a watch face, in `faces/<id>/face.tsx`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PackageKind {
    App,
    Face,
}

impl PackageKind {
    pub fn folder(self) -> &'static str {
        match self {
            PackageKind::App => "apps",
            PackageKind::Face => "faces",
        }
    }

    pub fn source_file(self) -> &'static str {
        match self {
            PackageKind::App => "app.tsx",
            PackageKind::Face => "face.tsx",
        }
    }
}

/// One app or face found in the repository.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Package {
    pub kind: PackageKind,
    pub id: String,
}

impl Package {
    /// The package as its folder names it, such as `apps/stopwatch`.
    pub fn label(&self) -> String {
        format!("{}/{}", self.kind.folder(), self.id)
    }
}

pub fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default()
}

pub fn package_source(kind: PackageKind, id: &str) -> PathBuf {
    repository_root()
        .join(kind.folder())
        .join(id)
        .join(kind.source_file())
}

/// Every app and face whose folder holds its source file, apps first, each sorted by id.
pub fn every_package() -> Vec<Package> {
    let mut packages = Vec::new();
    for kind in [PackageKind::App, PackageKind::Face] {
        let Ok(entries) = std::fs::read_dir(repository_root().join(kind.folder())) else {
            continue;
        };
        for entry in entries.flatten() {
            let id = entry.file_name().to_string_lossy().into_owned();
            if package_source(kind, &id).is_file() {
                packages.push(Package { kind, id });
            }
        }
    }
    packages.sort();
    packages
}

// ============================================================================
// Failures
// ============================================================================

/// Why a test watch could not open an app.
#[derive(Clone, PartialEq, Eq)]
pub enum OpenFailure {
    CannotRead(String),
    /// The compiler's errors, rendered with the source lines they point at.
    DoesNotCompile(String),
    NoSharedLibrary(String),
    CannotOpen(OpenError),
}

impl fmt::Display for OpenFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        match self {
            OpenFailure::CannotRead(problem) => write!(formatter, "cannot read the source: {problem}"),
            OpenFailure::DoesNotCompile(diagnostics) => write!(formatter, "does not compile:\n{diagnostics}"),
            OpenFailure::NoSharedLibrary(problem) => {
                write!(formatter, "cannot build the shared library: {problem}")
            }
            OpenFailure::CannotOpen(error) => {
                write!(formatter, "the app host cannot open the image: {error:?}")
            }
        }
    }
}

// Debug shows the same text, so `unwrap` prints the compiler's diagnostics readably.
impl fmt::Debug for OpenFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(self, formatter)
    }
}

// ============================================================================
// Compiling
// ============================================================================

/// Compiles an app or face from its source into an image.
pub fn compile_package(kind: PackageKind, id: &str) -> Result<Vec<u8>, OpenFailure> {
    let path = package_source(kind, id);
    let (source, output) = neotime_compiler::compile_file(&path).map_err(OpenFailure::CannotRead)?;
    let file_name = format!("{}/{}/{}", kind.folder(), id, kind.source_file());
    let errors: Vec<String> = output
        .errors()
        .map(|error| error.render(&file_name, &source))
        .collect();
    match output.image {
        Some(image) if errors.is_empty() => Ok(image),
        _ => Err(OpenFailure::DoesNotCompile(errors.join("\n"))),
    }
}

/// Compiles app source text that has no images of its own, such as an app written inside a test.
pub fn compile_source_text(source: &str) -> Result<Vec<u8>, OpenFailure> {
    let output = neotime_compiler::compile_source(source);
    let errors: Vec<String> = output
        .errors()
        .map(|error| error.render("app.tsx", source))
        .collect();
    match output.image {
        Some(image) if errors.is_empty() => Ok(image),
        _ => Err(OpenFailure::DoesNotCompile(errors.join("\n"))),
    }
}

/// The shared library, built once for every test in the process.
pub fn shared_library() -> Result<&'static [u8], OpenFailure> {
    static LIBRARY: OnceLock<Result<Vec<u8>, String>> = OnceLock::new();
    match LIBRARY.get_or_init(neotime_compiler::library::build_library) {
        Ok(bytes) => Ok(bytes),
        Err(problem) => Err(OpenFailure::NoSharedLibrary(problem.clone())),
    }
}
