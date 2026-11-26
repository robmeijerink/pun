//! tomasvotruba/type-coverage 2.5.1's rules. tests/fixtures/type-coverage holds upstream's rule fixtures
//! (`Rules/*TypeCoverageRule/{Fixture,Source}` at tag 2.5.1, with its MIT LICENSE and `config/extension.neon` in
//! `conf/`) and Pun's own cases (`Pun/`: parent-guarded members, interfaces, `@param callable`, variadics, closures,
//! traits used twice, enums, anonymous classes, property hooks and `@phpstan-ignore` comments). Each
//! expected-<neon>.json is PHPStan 2.3.5's run with the extension on `<neon>.neon`
//! (`scripts/export-analyse-expected.py tests/fixtures/type-coverage <neon>`): one per upstream test config
//! (`*_type: 80`), the alias keys and other levels, the property rule alone, and measure mode, whose errors have no
//! identifier and no file (PHPStan's `N/A`, line -1).

use std::path::Path;

use pun_analyse::{Project, run};
use pun_phpstan::config::{Options, resolve};
use serde_json::Value;

type Found = Vec<(String, i64, String, String)>;

fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/type-coverage")
}

fn project(neon: &str) -> Project {
    let root = root();
    let dir = root.to_string_lossy().into_owned();
    let config = resolve(&Options {
        config: Some(format!("{dir}/{neon}.neon")),
        ..Options::for_project(&dir)
    })
    .unwrap();
    Project::from_config(&root, &config.parameters)
}

fn found(project: &Project) -> Found {
    let prefix = format!("{}/", root().display());
    let mut got: Found = (run(project).unwrap().into_iter())
        .map(|e| {
            let file = e.file.strip_prefix(&prefix).unwrap_or(&e.file).to_string();
            (file, e.line, e.identifier, e.message)
        })
        .collect();
    got.sort();
    got
}

fn expected(neon: &str) -> Found {
    let json: Value = serde_json::from_slice(
        &std::fs::read(root().join(format!("expected-{neon}.json"))).unwrap(),
    )
    .unwrap();
    assert_eq!(json["general"], Value::Array(Vec::new()));
    let mut want: Found = (json["errors"].as_array().unwrap().iter())
        .map(|e| {
            (
                e["file"].as_str().unwrap().to_string(),
                e["line"].as_i64().unwrap(),
                e["identifier"].as_str().unwrap_or_default().to_string(),
                e["message"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    want.sort();
    want
}

#[test]
fn upstream_fixtures() {
    for neon in ["return", "param", "property", "constant"] {
        assert_eq!(found(&project(neon)), expected(neon), "{neon}.neon");
    }
}

#[test]
fn aliases_levels_and_ignores() {
    assert_eq!(found(&project("pun")), expected("pun"));
}

#[test]
fn properties() {
    assert_eq!(found(&project("properties")), expected("properties"));
}

#[test]
fn measure_mode() {
    assert_eq!(found(&project("measure")), expected("measure"));
}

/// Without the extension's parameters (no `type_coverage`) no rule runs.
#[test]
fn off_without_the_extension() {
    let mut project = project("return");
    project.type_coverage = None;
    assert_eq!(found(&project), Found::new());
}
