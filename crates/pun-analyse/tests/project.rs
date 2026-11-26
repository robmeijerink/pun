//! `analyse()` on a project from its phpstan.neon: which files are analysed or only scanned (paths, excludePaths,
//! fileExtensions, scanFiles, scanDirectories, Composer autoload of the root package and of vendor/composer/
//! installed.json), phpVersion, ignoreErrors and the ignore flags. Expected: PHPStan's own run on the fixture
//! (`scripts/analyse-corpus.py fixture`), restricted to the identifiers Pun reports.

use std::path::Path;

use pun_analyse::{Project, analyse, reports, run};
use pun_phpstan::config::{Options, resolve};
use serde_json::Value;

fn check(neon: &str) {
    check_in("project", neon);
}

fn check_in(fixture: &str, neon: &str) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(fixture);
    let dir = root.to_string_lossy().into_owned();
    let options = Options {
        config: Some(format!("{dir}/{neon}.neon")),
        ..Options::for_project(&dir)
    };
    let config = resolve(&options).unwrap_or_else(|e| panic!("{e}"));
    let prefix = format!("{dir}/");
    let got: Vec<_> = analyse(&root, &config)
        .unwrap()
        .into_iter()
        .map(|e| {
            let file = e.file.strip_prefix(&prefix).unwrap_or(&e.file).to_string();
            (file, e.line, e.identifier, e.message, e.tip)
        })
        .collect();
    let expected: Value =
        serde_json::from_slice(&std::fs::read(root.join(format!("expected-{neon}.json"))).unwrap())
            .unwrap();
    let mut want: Vec<_> = expected["errors"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| reports(e["identifier"].as_str().unwrap_or("")))
        .map(|e| {
            (
                e["file"].as_str().unwrap().to_string(),
                e["line"].as_i64().unwrap_or(0),
                e["identifier"].as_str().unwrap().to_string(),
                e["message"].as_str().unwrap().replace("{root}", &dir),
                e["tip"].as_str().map(str::to_string),
            )
        })
        .collect();
    want.sort_by(|a, b| (&a.0, a.1, &a.3).cmp(&(&b.0, b.1, &b.3)));
    assert_eq!(got, want);
}

#[test]
fn project_from_phpstan_neon() {
    check("phpstan");
}

#[test]
fn block_pollution_defaults_and_explicit_config() {
    use pun_phpstan::{Array, Value};

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/block-pollution");
    assert!(
        Project::new(Vec::new(), Vec::new())
            .options
            .contains(pun_analyse::ProjectOptions::POLLUTE_SCOPE_WITH_BLOCK)
    );
    assert!(
        Project::from_config(&root, &Array::new())
            .options
            .contains(pun_analyse::ProjectOptions::POLLUTE_SCOPE_WITH_BLOCK)
    );
    for (mode, pollution, maybe, loops) in [
        ("default", true, true, true),
        ("enabled", true, true, true),
        ("disabled", false, true, true),
        ("quiet", false, false, true),
        ("independent", false, true, false),
        ("loops-only", true, true, false),
        ("secondary", false, true, true),
        ("secondary-enabled", true, true, true),
    ] {
        let config = resolve(&Options {
            config: Some(
                root.join(format!("{mode}.neon"))
                    .to_string_lossy()
                    .into_owned(),
            ),
            ..Options::for_project(&root.to_string_lossy())
        })
        .unwrap();
        assert_eq!(
            config.parameters.get_str("polluteScopeWithBlock"),
            Some(&Value::Bool(pollution))
        );
        let project = Project::from_config(&root, &config.parameters);
        assert_eq!(
            project
                .options
                .contains(pun_analyse::ProjectOptions::POLLUTE_SCOPE_WITH_BLOCK),
            pollution
        );
        assert_eq!(
            project
                .options
                .contains(pun_analyse::ProjectOptions::CHECK_MAYBE_UNDEFINED_VARIABLES),
            maybe
        );
        assert_eq!(
            project
                .options
                .contains(pun_analyse::ProjectOptions::POLLUTE_SCOPE_WITH_LOOP_INITIAL_ASSIGNMENTS),
            loops
        );
        assert_eq!(
            project
                .options
                .contains(pun_analyse::ProjectOptions::POLLUTE_SCOPE_WITH_ALWAYS_ITERABLE_FOREACH),
            loops
        );
    }
}

#[test]
fn foreach_pollution_defaults_and_explicit_config() {
    use pun_phpstan::{Array, Value};

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/foreach-pollution");
    assert!(
        Project::new(Vec::new(), Vec::new())
            .options
            .contains(pun_analyse::ProjectOptions::POLLUTE_SCOPE_WITH_ALWAYS_ITERABLE_FOREACH)
    );
    assert!(
        Project::from_config(&root, &Array::new())
            .options
            .contains(pun_analyse::ProjectOptions::POLLUTE_SCOPE_WITH_ALWAYS_ITERABLE_FOREACH)
    );
    for (mode, pollution, maybe, loop_initial) in [
        ("default", true, true, true),
        ("enabled", true, true, true),
        ("disabled", false, true, true),
        ("quiet", false, false, true),
        ("independent", false, true, false),
    ] {
        let config = resolve(&Options {
            config: Some(
                root.join(format!("{mode}.neon"))
                    .to_string_lossy()
                    .into_owned(),
            ),
            ..Options::for_project(&root.to_string_lossy())
        })
        .unwrap();
        assert_eq!(
            config
                .parameters
                .get_str("polluteScopeWithAlwaysIterableForeach"),
            Some(&Value::Bool(pollution))
        );
        let project = Project::from_config(&root, &config.parameters);
        assert_eq!(
            project
                .options
                .contains(pun_analyse::ProjectOptions::POLLUTE_SCOPE_WITH_ALWAYS_ITERABLE_FOREACH),
            pollution
        );
        assert_eq!(
            project
                .options
                .contains(pun_analyse::ProjectOptions::CHECK_MAYBE_UNDEFINED_VARIABLES),
            maybe
        );
        assert_eq!(
            project
                .options
                .contains(pun_analyse::ProjectOptions::POLLUTE_SCOPE_WITH_LOOP_INITIAL_ASSIGNMENTS),
            loop_initial
        );
    }
}

#[test]
fn loop_initial_pollution_defaults_and_explicit_config() {
    use pun_phpstan::{Array, Value};

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/loop-pollution");
    assert!(
        Project::new(Vec::new(), Vec::new())
            .options
            .contains(pun_analyse::ProjectOptions::POLLUTE_SCOPE_WITH_LOOP_INITIAL_ASSIGNMENTS)
    );
    assert!(
        Project::from_config(&root, &Array::new())
            .options
            .contains(pun_analyse::ProjectOptions::POLLUTE_SCOPE_WITH_LOOP_INITIAL_ASSIGNMENTS)
    );
    for (mode, pollution, maybe) in [
        ("default", true, true),
        ("enabled", true, true),
        ("disabled", false, true),
        ("quiet", false, false),
    ] {
        let config = resolve(&Options {
            config: Some(
                root.join(format!("{mode}.neon"))
                    .to_string_lossy()
                    .into_owned(),
            ),
            ..Options::for_project(&root.to_string_lossy())
        })
        .unwrap();
        assert_eq!(
            config
                .parameters
                .get_str("polluteScopeWithLoopInitialAssignments"),
            Some(&Value::Bool(pollution))
        );
        let project = Project::from_config(&root, &config.parameters);
        assert_eq!(
            project
                .options
                .contains(pun_analyse::ProjectOptions::POLLUTE_SCOPE_WITH_LOOP_INITIAL_ASSIGNMENTS),
            pollution
        );
        assert_eq!(
            project
                .options
                .contains(pun_analyse::ProjectOptions::CHECK_MAYBE_UNDEFINED_VARIABLES),
            maybe
        );
    }
}

#[test]
fn php_version_range_and_ignore_flags() {
    check("phpstan-min");
}

#[test]
fn autoloaded_classes_found_on_demand() {
    check("phpstan-autoload");
}

#[test]
fn symlinks_are_followed() {
    check_in("symlinks", "phpstan");
}

#[test]
fn declaration_rules_by_php_version() {
    check("phpstan-versions-70400");
    check("phpstan-versions-80000");
    check("phpstan-versions-80400");
}

#[test]
fn feature_toggles() {
    check("phpstan-bleeding");
}

#[test]
fn unreadable_file_stops_the_run() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/project");
    let e = run(&Project::new(vec![dir.clone()], Vec::new())).unwrap_err();
    assert_eq!(
        e.to_string(),
        format!("Could not read file: {}", dir.display())
    );
    assert!(std::error::Error::source(&e).is_some());
    let gone = dir.join("gone.php");
    assert_eq!(run(&Project::new(vec![gone], Vec::new())).unwrap(), []);
}

#[test]
fn level_is_zero_when_unset() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/project");
    let options = Options {
        config: Some(
            dir.join("phpstan-nolevel.neon")
                .to_string_lossy()
                .into_owned(),
        ),
        ..Options::for_project(&dir.to_string_lossy())
    };
    let config = resolve(&options).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(Project::from_config(&dir, &config.parameters).level, 0);
}

#[test]
fn stub_files() {
    check("phpstan-stubs");
}

#[test]
fn stub_phpdoc_name_scope() {
    check("phpstan-stub-scope");
}

#[test]
fn reported_identifiers_and_general_errors() {
    use pun_phpstan::{Array, Key, Value};
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/project/reported/a.php");
    let mut project = Project::new(vec![file], Vec::new());
    project.reported = Some(std::sync::Arc::new(|id: &str| id != "function.notFound"));
    // PHPStan analysed a directory, so it reports the unmatched entry (only files on the command line would skip it).
    project.only_files = false;
    let mut entry = Array::new();
    entry.set(Key::from("message"), Value::str("#^Nothing$#"));
    let paths = Array::list([Value::str("/a/*"), Value::str("/b/*")]);
    entry.set(Key::from("paths"), Value::Array(paths));
    project.ignore_errors =
        pun_phpstan::ignore::IgnoredErrors::new(&Array::list([Value::Array(entry)]), true);
    let analysis = pun_analyse::analyse_project(&project).unwrap();
    let got: Vec<_> = (analysis.errors.iter())
        .map(|e| (e.line, e.identifier.as_str(), e.message.as_str()))
        .collect();
    assert_eq!(
        got,
        [
            (
                16,
                "ignore.unmatchedIdentifier",
                "No error with identifier function.notFound is reported on line 16."
            ),
            (
                17,
                "class.notFound",
                "Instantiated class ReportedMissing not found."
            ),
        ]
    );
    assert_eq!(
        analysis.general,
        [
            "Ignored error pattern #^Nothing$# in paths: /a/*, /b/* was not matched in reported errors."
        ]
    );
}

#[test]
fn extension_identifiers_are_reported() {
    // An identifier an extension's config entry names (disallowed-calls' `errorIdentifier`) is kept though no rule
    // claims it: the trait's error is reported and the comment ignores the other, as PHPStan does (in its order).
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/project/reported/a.php");
    let mut project = Project::new(vec![file], Vec::new());
    project.reported = Some(std::sync::Arc::new(|id: &str| id != "function.notFound"));
    project.extension_identifiers = ["function.notFound".to_string()].into();
    let analysis = pun_analyse::analyse_project(&project).unwrap();
    let got: Vec<_> = (analysis.errors.iter())
        .map(|e| (e.line, e.identifier.as_str(), e.message.as_str()))
        .collect();
    assert_eq!(
        got,
        [
            (
                17,
                "class.notFound",
                "Instantiated class ReportedMissing not found."
            ),
            (
                7,
                "function.notFound",
                "Function missing_in_trait not found."
            ),
        ]
    );
}
