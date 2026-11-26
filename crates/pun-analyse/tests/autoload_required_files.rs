use std::path::Path;

use pun_analyse::{Project, run};
use serde_json::Value;

// A package's `files` autoload that `require`s the real function files, directly or in a loop over a literal list as
// pest-plugin-laravel's Autoload.php does, defines those functions; a commented-out or unrequired file does not
// (#62): PHPStan 2.3.5's errors on src/Greeter.php at level 6 after `composer install`
#[test]
fn files_required_by_an_autoloaded_file_define_their_functions() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/autoload-required-files");
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
    let mut params = pun_phpstan::Array::new();
    params.set("level".into(), pun_phpstan::Value::Int(6));
    let mut project = Project::from_config(&root, &params);
    project.analysed = vec![root.join("src/Greeter.php")];
    let got: Vec<_> = (run(&project).unwrap().into_iter())
        .map(|e| (e.line, e.identifier, e.message))
        .collect();
    assert_eq!(got, want);
}
