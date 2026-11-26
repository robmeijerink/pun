use std::path::Path;

use pun_analyse::{Project, run};
use serde_json::Value;

// A match whose subject was assigned from another match reads that match's arm values, not the `1` the subject
// pass writes into a match's arms (#38 item 3): PHPStan 2.3.5's errors on matches.php at level 6
#[test]
fn a_match_subject_assigned_from_another_match_keeps_its_arm_values() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/chained-matches");
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
    let mut project = Project::new(vec![root.join("matches.php")], Vec::new());
    project.level = 6;
    let got: Vec<_> = (run(&project).unwrap().into_iter())
        .map(|e| (e.line, e.identifier, e.message))
        .collect();
    assert_eq!(got, want);
}
