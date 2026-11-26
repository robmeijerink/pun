//! phpstan/phpstan-mockery 2.0.0 (with mockery/mockery 1.6.15). tests/fixtures/mockery holds upstream's tests
//! (`tests/Mockery`, in `src/` and `src/data/`) and Probe.php, which dumps the type of each form the extension
//! types. Its expected-phpstan.json is PHPStan 2.3.5's output at level 9 with phpstan-mockery's extension.neon
//! included, and expected-phpstan-without.json the same run without it. Pun reads Mockery through the trimmed
//! `vendor/mockery`, and PHPUnit's TestCase through phpunit.php. The lines that still differ are in
//! tests/fixtures/mockery/gaps.txt, and the list must be exact.

use std::collections::BTreeSet;
use std::path::Path;

use pun_analyse::{Project, run};
use pun_phpstan::{Array, Key, Value as Parameter};
use serde_json::Value;

type Found = Vec<(String, i64, String, String)>;

fn analyse(extension: bool) -> Found {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mockery");
    let path = |p: &str| Parameter::str(root.join(p).to_string_lossy().into_owned());
    let mut parameters = Array::new();
    parameters.set(Key::from("level"), Parameter::Int(9));
    parameters.set(
        Key::from("fileExtensions"),
        Parameter::Array(Array::list([Parameter::str("php")])),
    );
    parameters.set(
        Key::from("paths"),
        Parameter::Array(Array::list([path("src")])),
    );
    parameters.set(
        Key::from("scanFiles"),
        Parameter::Array(Array::list([path("phpunit.php")])),
    );
    parameters.set(
        Key::from("scanDirectories"),
        Parameter::Array(Array::list([path("vendor")])),
    );
    if extension {
        // extension.neon's parameters
        let mut mockery = Array::new();
        mockery.set(
            Key::from("convertUnionToIntersectionType"),
            Parameter::Bool(true),
        );
        parameters.set(Key::from("mockery"), Parameter::Array(mockery));
    }
    let project = Project::from_config(&root, &parameters);
    let prefix = format!("{}/", root.display());
    let mut got: Found = run(&project)
        .unwrap()
        .into_iter()
        .map(|e| {
            let file = e.file.strip_prefix(&prefix).unwrap_or(&e.file).to_string();
            (file, e.line, e.identifier, e.message)
        })
        .collect();
    got.sort();
    got
}

fn oracle(json: &str) -> Found {
    let oracle: Value = serde_json::from_str(json).unwrap();
    // `phpstanApi.phpstanNamespace` (the fixture's `PHPStan\Mockery` namespace) is out of scope in Pun: mapping.toml
    // marks PhpStanNamespaceIn3rdPartyPackageRule unsupported.
    let mut want: Found = (oracle["errors"].as_array().unwrap().iter())
        .filter(|e| e["identifier"] != "phpstanApi.phpstanNamespace")
        .map(|e| {
            (
                e["file"].as_str().unwrap().to_string(),
                e["line"].as_i64().unwrap(),
                e["identifier"].as_str().unwrap().to_string(),
                e["message"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    want.sort();
    want
}

/// Pun's and PHPStan's errors for `neon`, each without the gaps.txt lines, and the gap lines that matched.
fn compare(neon: &str, json: &str) -> (Found, Found, Vec<String>) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mockery");
    let gaps: BTreeSet<String> = (std::fs::read_to_string(root.join("gaps.txt"))
        .unwrap()
        .lines())
    .filter_map(|l| l.strip_prefix(&format!("{neon} ")))
    .map(str::to_string)
    .collect();
    let key = |e: &(String, i64, String, String)| format!("{}:{} {}", e.0, e.1, e.2);
    let (got, want) = (analyse(neon == "phpstan"), oracle(json));
    let matched = (gaps.iter())
        .filter(|g| {
            let on = |f: &Found| {
                f.iter()
                    .filter(|e| key(e) == **g)
                    .cloned()
                    .collect::<Vec<_>>()
            };
            on(&got) == on(&want)
        })
        .cloned()
        .collect();
    let without = |f: Found| f.into_iter().filter(|e| !gaps.contains(&key(e))).collect();
    (without(got), without(want), matched)
}

#[test]
fn mock_types_with_the_extension() {
    let (got, want, matched) = compare(
        "phpstan",
        include_str!("fixtures/mockery/expected-phpstan.json"),
    );
    assert_eq!(got, want);
    assert!(matched.is_empty(), "gaps that match now: {matched:?}");
}

#[test]
fn mock_types_without_the_extension() {
    let (got, want, matched) = compare(
        "phpstan-without",
        include_str!("fixtures/mockery/expected-phpstan-without.json"),
    );
    assert_eq!(got, want);
    assert!(matched.is_empty(), "gaps that match now: {matched:?}");
}
