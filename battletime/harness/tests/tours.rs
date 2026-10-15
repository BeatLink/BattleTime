//! Plays each app's tour in tests/tours, which snapshots every screen of the app, and checks that the
//! app's page in docs/apps shows its main screen and every picture its tour takes.

use std::path::PathBuf;

use battletime_app_testing::{
    docs_root, every_package, load_script, repository_root, run_script, Package, PackageKind, ScriptCommand,
};

// ============================================================================
// Finding the tours and pages
// ============================================================================

/// Every app and shell screen; faces are listed together in docs/faces.md instead.
fn toured_packages() -> Vec<Package> {
    every_package()
        .into_iter()
        .filter(|package| package.kind != PackageKind::Face)
        .collect()
}

fn picture_prefix(package: &Package) -> String {
    match package.kind {
        PackageKind::Shell => format!("shell-{}", package.id),
        _ => format!("app-{}", package.id),
    }
}

fn tour_path(id: &str) -> PathBuf {
    repository_root()
        .join("tests")
        .join("tours")
        .join(format!("{id}.battletime"))
}

fn page_path(id: &str) -> PathBuf {
    docs_root().join("apps").join(format!("{id}.md"))
}

// ============================================================================
// Checks
// ============================================================================

#[test]
fn every_tour_passes_and_its_page_shows_every_picture() {
    let packages = toured_packages();
    assert!(!packages.is_empty(), "no apps found");

    let output_folder = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("tour-screenshots");
    let mut problems = Vec::new();
    for package in &packages {
        let id = &package.id;
        let script = match load_script(&tour_path(id)) {
            Ok(script) => script,
            Err(problem) => {
                problems.push(format!("{id}: no tour in tests/tours/{id}.battletime: {problem}"));
                continue;
            }
        };
        let page = match std::fs::read_to_string(page_path(id)) {
            Ok(page) => page,
            Err(problem) => {
                problems.push(format!("{id}: no page in docs/apps/{id}.md: {problem}"));
                continue;
            }
        };

        if script.steps.first().map(|step| &step.command) != Some(&ScriptCommand::OpenApp(id.clone())) {
            problems.push(format!("{id}: its tour must start with `open app {id}`"));
        }

        let prefix = picture_prefix(package);
        let mut pictures = vec![format!("{prefix}.png")];
        pictures.extend(script.steps.iter().filter_map(|step| match &step.command {
            ScriptCommand::Snapshot(name) => Some(format!("{prefix}-{name}.png")),
            _ => None,
        }));
        for picture in pictures {
            if !page.contains(&format!("harness/snapshots/screens/{picture})")) {
                problems.push(format!("{id}: docs/apps/{id}.md does not show {picture}"));
            }
        }

        if let Some(failure) = run_script(&script, &output_folder, |_| {}).failure {
            problems.push(format!("{id}: {failure}"));
        }
    }

    assert!(
        problems.is_empty(),
        "\n{}\nRun with BATTLETIME_UPDATE_SNAPSHOTS=1 to accept a changed picture.",
        problems.join("\n")
    );
}

#[test]
fn every_tour_and_page_belongs_to_an_app() {
    let ids: Vec<String> = toured_packages().into_iter().map(|package| package.id).collect();
    let mut strays = Vec::new();
    for (folder, extension) in [
        (repository_root().join("tests/tours"), "battletime"),
        (docs_root().join("apps"), "md"),
    ] {
        for entry in std::fs::read_dir(folder).unwrap().flatten() {
            let path = entry.path();
            let stem = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
            if path.extension().is_some_and(|found| found == extension) && !ids.contains(&stem) {
                strays.push(path.display().to_string());
            }
        }
    }

    assert!(strays.is_empty(), "no app has these: {strays:?}");
}
