//! Runs every example script in tests/scripts, so `cargo test` and CI play them end to end.

use std::path::PathBuf;

use battletime_app_testing::{load_script, repository_root, run_script};

// ============================================================================
// Finding the scripts
// ============================================================================

/// The examples docs/testing.md points to; more may be added beside them.
const FEWEST_EXAMPLE_SCRIPTS: usize = 5;

fn example_scripts() -> Vec<PathBuf> {
    let folder = repository_root().join("tests").join("scripts");
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&folder)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", folder.display()))
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "battletime")
        })
        .collect();
    paths.sort();
    paths
}

// ============================================================================
// Running them
// ============================================================================

#[test]
fn every_example_script_passes() {
    let output_folder = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("script-screenshots");
    let scripts = example_scripts();
    assert!(scripts.len() >= FEWEST_EXAMPLE_SCRIPTS, "only found {scripts:?}");

    let mut problems = Vec::new();
    for path in &scripts {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        match load_script(path) {
            Err(problem) => problems.push(problem),
            Ok(script) => {
                let run = run_script(&script, &output_folder, |_| {});
                if let Some(failure) = run.failure {
                    problems.push(format!("{name}: {failure}"));
                }
            }
        }
    }

    assert!(problems.is_empty(), "{}", problems.join("\n\n"));
}

#[test]
fn a_script_that_cannot_be_read_names_its_file() {
    let missing = repository_root()
        .join("tests")
        .join("scripts")
        .join("missing.battletime");

    let problem = load_script(&missing).unwrap_err();

    assert!(problem.contains("missing.battletime"), "{problem}");
}
