use std::path::Path;

use pun_analyse::{Project, run};
use serde_json::Value;

// A trait method overriding a trait method that overrides a documented method inherits the original `@param` types,
// whatever the order of the class names (#60): PHPStan 2.3.5's errors on chain.php at level 6, sorted
#[test]
fn an_override_two_traits_deep_inherits_the_original_phpdoc() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/trait-override-chains");
    let oracle: Value =
        serde_json::from_slice(&std::fs::read(root.join("expected-phpstan.json")).unwrap())
            .unwrap();
    let mut want: Vec<_> = (oracle["errors"].as_array().unwrap().iter())
        .map(|e| {
            (
                e["line"].as_i64().unwrap(),
                e["identifier"].as_str().unwrap().to_owned(),
                e["message"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    let mut project = Project::new(vec![root.join("chain.php")], Vec::new());
    project.level = 6;
    let mut got: Vec<_> = (run(&project).unwrap().into_iter())
        .map(|e| (e.line, e.identifier, e.message))
        .collect();
    want.sort();
    got.sort();
    assert_eq!(got, want);
}
