use std::path::Path;

use pun_analyse::{Project, run};
use serde_json::Value;

// `array_push()` and `array_unshift()` with only unpacked, possibly empty values leave a possibly empty array possibly
// empty, so `while ($pending !== [])` around `array_shift()` and `array_unshift(...)` is not always true (#38 item
// 1): PHPStan 2.3.5's errors on appends.php at level 6
#[test]
fn unpacked_appends_of_possibly_empty_arrays_may_add_nothing() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/unpacked-appends");
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
    let mut project = Project::new(vec![root.join("appends.php")], Vec::new());
    project.level = 6;
    let got: Vec<_> = (run(&project).unwrap().into_iter())
        .map(|e| (e.line, e.identifier, e.message))
        .collect();
    assert_eq!(got, want);
}
