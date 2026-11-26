use std::path::Path;

use pun_analyse::{Project, run};
use serde_json::Value;

// A `static` return declared on a class with more templates than the receiver's (`Relation<A, B, C>::clone()` on
// `HasMany<A, B>`) keeps the receiver's arguments (#52): PHPStan 2.3.5's errors on statics.php at level max, sorted
#[test]
fn static_keeps_the_receivers_arguments_when_its_class_names_fewer_templates() {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/static-of-fewer-templates");
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
    let mut project = Project::new(vec![root.join("statics.php")], Vec::new());
    project.level = 10;
    let mut got: Vec<_> = (run(&project).unwrap().into_iter())
        .map(|e| (e.line, e.identifier, e.message))
        .collect();
    want.sort();
    got.sort();
    assert_eq!(got, want);
}
