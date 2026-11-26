use std::path::Path;

use pun_analyse::{Project, run};
use serde_json::Value;

fn check(file: &str, oracle: &str, level: u8) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/filter-functions");
    let capture: Value =
        serde_json::from_slice(&std::fs::read(root.join(oracle)).unwrap()).unwrap();
    let want: Vec<_> = capture["files"][file]["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|error| {
            (
                error["line"].as_i64().unwrap(),
                error["identifier"].as_str().unwrap().to_owned(),
                error["message"].as_str().unwrap().to_owned(),
                error["tip"].as_str().map(str::to_owned),
            )
        })
        .collect();
    assert_ne!(
        want,
        [] as [(
            i64,
            std::string::String,
            std::string::String,
            std::option::Option<std::string::String>
        ); 0]
    );
    let mut project = Project::new(vec![root.join(file)], Vec::new());
    project.php_version = pun_analyse::PhpVersion::from_id(80400);
    project.level = level;
    let got: Vec<_> = run(&project)
        .unwrap()
        .into_iter()
        .map(|error| (error.line, error.identifier, error.message, error.tip))
        .collect();
    let missing: Vec<_> = want.iter().filter(|error| !got.contains(error)).collect();
    let extra: Vec<_> = got.iter().filter(|error| !want.contains(error)).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "{file}\nmissing: {missing:#?}\nextra: {extra:#?}"
    );
    assert_eq!(got.len(), want.len());
}

#[test]
fn filter_flags_values_and_ranges() {
    check("cases.php", "expected-phpstan.json", 0);
}

#[test]
fn unknown_states_are_implicit_but_explicit_inputs_and_failures_are_not_suppressed() {
    check("flow.php", "expected-flow.json", 9);
}

#[test]
fn untouched_upstream_filter_regressions_at_level_nine() {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rules/data/Functions/data");
    for name in ["bug-6261.php", "bug-11863.php"] {
        let mut project = Project::new(vec![root.join(name)], Vec::new());
        project.level = 9;
        assert_eq!(run(&project).unwrap(), Vec::new(), "{name}");
    }
}

#[test]
fn filter_validate_boolean() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/filter-functions");
    let mut project = Project::new(vec![root.join("boolean.php")], Vec::new());
    project.php_version = pun_analyse::PhpVersion::from_id(80400);
    project.level = 9;
    let got: Vec<_> = run(&project)
        .unwrap()
        .into_iter()
        .map(|error| (error.line, error.identifier, error.message))
        .collect();
    let want: Vec<_> = [
        (16, "Dumped type: bool"),
        (17, "Dumped type: bool"),
        (18, "Dumped type: bool"),
        (19, "Dumped type: bool"),
        (20, "Dumped type: bool|null"),
        (21, "Dumped type: bool|null"),
        (22, "Dumped type: bool"),
        (23, "Dumped type: bool|null"),
        (24, "Dumped type: mixed"),
        (25, "Dumped type: 'bad'|bool"),
        (26, "Dumped type: bool"),
        (27, "Dumped type: bool"),
        (28, "Dumped type: true"),
        (29, "Dumped type: false"),
        (30, "Dumped type: false"),
        (31, "Dumped type: null"),
        (32, "Dumped type: false"),
        (33, "Dumped type: array<string, bool>"),
        (34, "Dumped type: array<bool>"),
        (35, "Dumped type: bool|null"),
        (36, "Dumped type: bool|null"),
        (37, "Dumped type: bool"),
        (38, "Dumped type: bool|null"),
        (39, "Dumped type: bool|null"),
        (40, "Dumped type: mixed"),
        (41, "Dumped type: mixed"),
        (42, "Dumped type: array<bool|null>|null"),
    ]
    .into_iter()
    .map(|(line, message)| (line, "phpstan.dumpType".to_owned(), message.to_owned()))
    .collect();
    assert_eq!(got, want);
}
