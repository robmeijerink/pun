use std::path::Path;

use pun_analyse::{Project, ProjectOptions, run};
use serde_json::Value;

// A match subject without an expression id (`$order?->kind`, `$order->kind ?? null`) reads its variables, though the
// arms compare a synthetic variable with the subject's span (#48): PHPStan 2.3.5's errors on reads.php at level max
// with `featureToggles.unusedVariable`
#[test]
fn a_nullsafe_match_subject_reads_its_variable() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/match-subject-reads");
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
    let mut project = Project::new(vec![root.join("reads.php")], Vec::new());
    project.level = 10;
    project.options.set(ProjectOptions::UNUSED_VARIABLE, true);
    let got: Vec<_> = (run(&project).unwrap().into_iter())
        .map(|e| (e.line, e.identifier, e.message))
        .collect();
    assert_eq!(got, want);
}
