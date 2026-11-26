use std::path::Path;

use pun_analyse::{Project, run};
use serde_json::Value;

// An inline `@var` in a trait method, analysed in the using class, resolves the trait's `@phpstan-type` alias; a trait
// that uses the declaring trait does not see it (#49): PHPStan 2.3.5's errors on aliases.php at level max, sorted
#[test]
fn a_trait_methods_inline_var_resolves_the_traits_alias() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/trait-type-aliases");
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
    let mut project = Project::new(vec![root.join("aliases.php")], Vec::new());
    project.level = 10;
    let mut got: Vec<_> = (run(&project).unwrap().into_iter())
        .map(|e| (e.line, e.identifier, e.message))
        .collect();
    want.sort();
    got.sort();
    assert_eq!(got, want);
}
