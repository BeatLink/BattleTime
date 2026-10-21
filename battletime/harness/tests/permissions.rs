//! The permissions the compiler works out for every app, shell screen and face, checked against
//! what each one needs, so a change to the compiler cannot quietly drop one.

use battletime_app_testing::{every_package, package_source};

/// Every package that needs a permission, with the permissions it needs; any other needs none.
const NEEDED: &[(&str, &[&str])] = &[
    ("apps/alarm", &["wake", "background"]),
    ("apps/beacon", &["heart_rate"]),
    ("apps/call", &["phone"]),
    ("apps/dice", &["motion"]),
    ("apps/heart", &["heart_rate"]),
    ("apps/level", &["motion"]),
    ("apps/motion", &["motion"]),
    ("apps/music", &["phone"]),
    ("apps/navigation", &["phone"]),
    ("apps/phone-finder", &["phone"]),
    ("apps/pomodoro", &["wake", "background"]),
    ("apps/prompt", &["phone_messages"]),
    ("apps/remote", &["phone_messages"]),
    ("apps/settings", &["settings"]),
    ("apps/sos", &["heart_rate", "phone_messages", "background"]),
    ("apps/software", &["settings"]),
    ("apps/about", &["settings"]),
    ("apps/timer", &["wake", "background"]),
    ("shell/notifications", &["notifications"]),
    ("shell/quick-settings", &["settings"]),
];

#[test]
fn every_package_needs_exactly_its_listed_permissions() {
    let mut wrong = Vec::new();
    for package in every_package() {
        let label = package.label();
        let (_, output) =
            battletime_compiler::compile_file(&package_source(package.kind, &package.id)).unwrap();
        assert!(output.errors().next().is_none(), "{label} does not compile");
        let found: Vec<&str> = output
            .permissions
            .iter()
            .map(|permission| permission.name())
            .collect();
        let mut expected: Vec<&str> = NEEDED
            .iter()
            .find(|(name, _)| *name == label)
            .map_or(Vec::new(), |(_, needed)| needed.to_vec());
        expected.sort_unstable();
        let mut sorted = found.clone();
        sorted.sort_unstable();
        if sorted != expected {
            wrong.push(format!("{label}: needs {found:?}, listed {expected:?}"));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
