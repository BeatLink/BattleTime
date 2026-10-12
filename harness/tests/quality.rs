//! Checks every app and face against NeoTime's rules for packages, in the spirit of wasp-os's
//! `test_qa`: who it says it is, that it is documented, and that its image is sound and fits in RAM.

use neotime_app_testing::{
    compile_package, every_package, package_source, repository_root, Package, PackageKind,
};
use neotime_runtime::{AppImage, AppKind, Property, PropertySource, WidgetTree};

// ============================================================================
// Rules
// ============================================================================

/// The longest id the watch takes for an app's folder, `MOST_ID_BYTES` in `services/packages`.
const MOST_ID_BYTES: usize = 32;

/// The RAM docs/app-runtime.md gives the open app, which its expressions are copied into.
const OPEN_APP_BYTES: usize = 12 * 1024;

/// The runtime's measured working memory in that RAM: interpreter, bindings, state and section reader.
const RUNTIME_WORKING_BYTES: usize = 1_792 + 2_056 + 2_440 + 256;

/// The expression bytecode that fits beside the runtime's working memory.
const MOST_EXPRESSION_BYTES: usize = OPEN_APP_BYTES - RUNTIME_WORKING_BYTES;

/// How many widgets of one screen fit in the 20 KB UI heap at 240 B a node, from docs/memory-map.md.
const MOST_WIDGETS_ON_A_SCREEN: usize = 55;

/// Whether an id is lowercase words of letters and digits joined by single hyphens, like `four-in-a-row`.
fn is_lowercase_hyphenated(id: &str) -> bool {
    let words_are_plain = id.split('-').all(|word| {
        !word.is_empty()
            && word
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
    });
    words_are_plain && id.len() <= MOST_ID_BYTES
}

/// The page that lists a package, and its text.
fn listing_page(kind: PackageKind) -> (&'static str, String) {
    let page = match kind {
        PackageKind::App => "docs/apps.md",
        PackageKind::Face => "docs/faces.md",
    };
    let text = std::fs::read_to_string(repository_root().join(page)).unwrap_or_default();
    (page, text)
}

/// Whether the first column of some table row on the page names the id, as `` `id` ``.
fn is_listed(page_text: &str, id: &str) -> bool {
    let quoted = format!("`{id}`");
    page_text
        .lines()
        .filter_map(|line| line.strip_prefix('|'))
        .filter_map(|row| row.split('|').next())
        .any(|first_column| first_column.contains(&quoted))
}

/// How many widgets a screen has, counting the root and everything inside it.
fn widgets_in(tree: &WidgetTree, root: u16) -> usize {
    1 + tree
        .children(root)
        .map(|child| widgets_in(tree, child))
        .sum::<usize>()
}

fn screen_name<'image>(image: &AppImage<'image>, tree: &WidgetTree<'image>, root: u16) -> &'image str {
    let name = tree
        .widget(root)
        .and_then(|widget| widget.property(Property::ScreenName));
    match name {
        Some(PropertySource::Constant(string)) => u16::try_from(string)
            .ok()
            .and_then(|string| image.string(string))
            .unwrap_or("main"),
        _ => "main",
    }
}

// ============================================================================
// Checking one package
// ============================================================================

/// Everything one package breaks, or an empty list.
fn problems_with(package: &Package) -> Vec<String> {
    let mut problems = Vec::new();
    let id = package.id.as_str();

    if !is_lowercase_hyphenated(id) {
        problems.push(format!(
            "the folder name is not lowercase words joined by hyphens, at most {MOST_ID_BYTES} bytes"
        ));
    }
    let source = std::fs::read_to_string(package_source(package.kind, id)).unwrap_or_default();
    if !source.starts_with("//") {
        problems.push(
            "the source does not start with a comment saying what it is and where it comes from".into(),
        );
    }
    let (page, page_text) = listing_page(package.kind);
    if !is_listed(&page_text, id) {
        problems.push(format!("{page} does not list `{id}`"));
    }

    let image_bytes = match compile_package(package.kind, id) {
        Ok(image_bytes) => image_bytes,
        Err(failure) => {
            problems.push(failure.to_string());
            return problems;
        }
    };
    if let Err(problem) = neotime_compiler::check_image(&image_bytes) {
        problems.push(format!("the image fails the install checks: {problem}"));
    }
    let Ok(image) = AppImage::parse(&image_bytes) else {
        return problems;
    };
    let expression_bytes: usize = (0..image.expression_count())
        .filter_map(|expression| image.expression_code(expression))
        .map(<[u8]>::len)
        .sum();
    if expression_bytes > MOST_EXPRESSION_BYTES {
        problems.push(format!(
            "its expressions take {expression_bytes} bytes, over the {MOST_EXPRESSION_BYTES} bytes the open app has for them"
        ));
    }
    let tree = WidgetTree::new(image);
    for screen in tree.screens() {
        let widgets = widgets_in(&tree, screen);
        if widgets > MOST_WIDGETS_ON_A_SCREEN {
            let name = screen_name(&image, &tree, screen);
            problems.push(format!(
                "its screen {name:?} has {widgets} widgets, over the {MOST_WIDGETS_ON_A_SCREEN} the UI heap holds"
            ));
        }
    }
    let declared_id = image.string(image.id_string).unwrap_or_default();
    if declared_id != id {
        problems.push(format!(
            "define_app says its id is {declared_id:?}, but its folder is {id:?}"
        ));
    }
    if image
        .string(image.name_string)
        .unwrap_or_default()
        .trim()
        .is_empty()
    {
        problems.push("define_app gives no name".into());
    }
    let expected_kind = match package.kind {
        PackageKind::App => AppKind::App,
        PackageKind::Face => AppKind::Face,
    };
    if image.kind != expected_kind {
        problems.push(format!(
            "its kind is {:?}, but it lives with the {:?}s",
            image.kind, package.kind
        ));
    }
    if package.kind == PackageKind::App && !image.has_icon {
        problems.push("define_app gives no icon for the launcher".into());
    }
    problems
}

// ============================================================================
// Every package
// ============================================================================

/// Rules some packages break today, each a real gap to fix, so the rest stay checked; see docs/testing.md.
const KNOWN_GAPS: [(&str, &str); 4] = [
    ("apps/alarm", "its screen \"main\" has 67 widgets"),
    ("apps/launcher", "its screen \"main\" has 65 widgets"),
    ("apps/twos", "its expressions take"),
    ("faces/infineat", "its expressions take"),
];

fn is_known_gap(package: &Package, problem: &str) -> bool {
    KNOWN_GAPS
        .iter()
        .any(|(label, start)| *label == package.label() && problem.starts_with(start))
}

#[test]
fn every_app_and_face_follows_the_package_rules() {
    let packages = every_package();
    assert!(!packages.is_empty(), "no apps or faces found");

    let mut report = Vec::new();
    let mut gaps_seen = Vec::new();
    for package in &packages {
        let (known, unknown): (Vec<String>, Vec<String>) = problems_with(package)
            .into_iter()
            .partition(|problem| is_known_gap(package, problem));
        gaps_seen.extend(known.iter().map(|problem| (package.label(), problem.clone())));
        if !unknown.is_empty() {
            report.push(format!("{}:\n  {}", package.label(), unknown.join("\n  ")));
        }
    }

    assert!(report.is_empty(), "\n{}", report.join("\n"));
    for (label, start) in KNOWN_GAPS {
        assert!(
            gaps_seen
                .iter()
                .any(|(seen_label, problem)| seen_label == label && problem.starts_with(start)),
            "{label} no longer has the gap {start:?}; take it out of KNOWN_GAPS"
        );
    }
}

#[test]
fn ids_are_lowercase_words_joined_by_hyphens() {
    assert!(is_lowercase_hyphenated("four-in-a-row"));
    assert!(is_lowercase_hyphenated("puzzle15"));
    assert!(!is_lowercase_hyphenated("Stopwatch"));
    assert!(!is_lowercase_hyphenated("phone_finder"));
    assert!(!is_lowercase_hyphenated("double--hyphen"));
    assert!(!is_lowercase_hyphenated("-leading"));
    assert!(!is_lowercase_hyphenated(&"a".repeat(MOST_ID_BYTES + 1)));
}

#[test]
fn a_listing_counts_only_in_the_first_column() {
    let page = "| App | Notes |\n|---|---|\n| `hello`, `template` | Like `timer`. |\n";

    assert!(is_listed(page, "hello"));
    assert!(is_listed(page, "template"));
    assert!(!is_listed(page, "timer"));
}
