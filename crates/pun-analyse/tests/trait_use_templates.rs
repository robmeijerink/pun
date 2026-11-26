use std::path::Path;

use pun_analyse::{Project, run};
use serde_json::Value;

// `@use HoldsRows<Apple>` binds the trait's TRow in the using class, in the trait's method bodies and inside a method
// template's bound, while an unbound use keeps the bound and a mismatched override is still reported (#50): PHPStan
// 2.3.5's errors on rows.php at level 10, sorted
#[test]
fn a_trait_use_tag_binds_the_traits_templates_in_the_using_class() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/trait-use-templates");
    let oracle: Value =
        serde_json::from_slice(&std::fs::read(root.join("expected-phpstan.json")).unwrap())
            .unwrap();
    let mut want: Vec<_> = (oracle["errors"].as_array().unwrap().iter())
        .map(|e| {
            (
                e["file"].as_str().unwrap().to_owned(),
                e["line"].as_i64().unwrap(),
                e["identifier"].as_str().unwrap().to_owned(),
                e["message"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    let mut project = Project::new(vec![root.join("rows.php")], Vec::new());
    project.level = 10;
    let prefix = format!("{}/", root.display());
    let mut got: Vec<_> = (run(&project).unwrap().into_iter())
        .map(|e| {
            let file = e.file.strip_prefix(&prefix).unwrap_or(&e.file).to_owned();
            (file, e.line, e.identifier, e.message)
        })
        .collect();
    want.sort();
    got.sort();
    assert_eq!(got, want);
}
