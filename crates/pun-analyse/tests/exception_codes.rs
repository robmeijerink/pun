use std::path::Path;

use pun_analyse::{Project, run};
use serde_json::Value;

// PHPStan's ThrowableReturnTypeExtension: `getCode()` is an `int` except on PDOException and on a bare Throwable,
// Exception or RuntimeException, whose `(int|string)` is benevolent (#74): PHPStan 2.3.5's errors on codes.php at level
// max
#[test]
fn exception_codes_are_ints_except_on_pdo_and_base_exceptions() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/exception-codes");
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
    let mut project = Project::new(vec![root.join("codes.php")], Vec::new());
    project.level = 10;
    let got: Vec<_> = (run(&project).unwrap().into_iter())
        .map(|e| (e.line, e.identifier, e.message))
        .collect();
    assert_eq!(got, want);
}
