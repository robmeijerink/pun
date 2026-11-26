use std::path::Path;
use std::time::{Duration, Instant};

use pun_analyse::{Project, run};
use serde_json::Value;

// Array shapes nested 15 aliases deep, once accepted and once reported: PHPStan 2.3.5's errors on repro.php at
// level 10. Describing a union described its members again for every sort, join and generalization, so the run
// time tripled with each level (#43); PHPStan keeps one description per union and finishes in about a second.
#[test]
fn nested_array_shape_aliases_are_described_in_linear_time() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/nested-shape-aliases");
    let oracle: Value =
        serde_json::from_slice(&std::fs::read(root.join("expected-phpstan.json")).unwrap())
            .unwrap();
    let want: Vec<_> = (oracle["errors"].as_array().unwrap().iter())
        .map(|e| {
            (
                e["line"].as_i64().unwrap(),
                e["identifier"].as_str().unwrap().to_owned(),
                e["message"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    let mut project = Project::new(vec![root.join("repro.php")], Vec::new());
    project.level = 10;
    let started = Instant::now();
    let got: Vec<_> = (run(&project).unwrap().into_iter())
        .map(|e| (e.line, e.identifier, e.message))
        .collect();
    let elapsed = started.elapsed();
    assert_eq!(got, want);
    assert!(elapsed < Duration::from_secs(30), "took {elapsed:?}");
}
