//! spaze/phpstan-disallowed-calls 4.15.1. tests/fixtures/disallowed holds the package's `tests/src` and one config
//! per upstream rule test (`<case>.neon`, which includes the package's own `extension.neon` and
//! `config/<case>.neon`, upstream's PHP-array rule config as neon parameters). Each `expected-<case>.json` is
//! PHPStan 2.3.5's output with the package on it (scripts/export-analyse-expected.py). The errors the package
//! reports (`disallowed.*` and each entry's `errorIdentifier`) are compared, except the ones `gaps.txt` lists.
//! The `pun-*` cases are Pun's own inputs (`src/pun-*`) for paths upstream's tests miss, scored the same way.

use std::collections::HashSet;
use std::path::Path;

use pun_analyse::{Project, run};
use pun_phpstan::config::{Options, resolve};
use pun_phpstan::{Array, Key, Value as V};
use serde_json::Value;

type Found = Vec<(String, i64, String, String, Option<String>)>;

fn fixtures() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/disallowed")
}

fn analyse(case: &str) -> (Found, HashSet<String>) {
    analyse_with(case, |_| {})
}

/// The errors of `case`, its resolved parameters changed by `edit` first.
fn analyse_with(case: &str, edit: impl FnOnce(&mut Array)) -> (Found, HashSet<String>) {
    let crate_dir = env!("CARGO_MANIFEST_DIR");
    let root = fixtures();
    let mut config = resolve(&Options {
        config: Some(format!("{}/{case}.neon", root.display())),
        ..Options::for_project(crate_dir)
    })
    .unwrap();
    edit(&mut config.parameters);
    let project = Project::from_config(Path::new(crate_dir), &config.parameters);
    let ids = project.extension_identifiers.clone();
    let prefix = format!("{}/", root.display());
    let got = (run(&project).unwrap().into_iter())
        .map(|e| {
            let file = e.file.strip_prefix(&prefix).unwrap_or(&e.file).to_string();
            (file, e.line, e.identifier, e.message, e.tip)
        })
        .collect();
    (got, ids)
}

fn oracle(case: &str) -> Found {
    let json = std::fs::read(fixtures().join(format!("expected-{case}.json"))).unwrap();
    let oracle: Value = serde_json::from_slice(&json).unwrap();
    assert_eq!(oracle["general"], serde_json::json!([]), "{case}");
    (oracle["errors"].as_array().unwrap().iter())
        .map(|e| {
            (
                e["file"].as_str().unwrap().to_string(),
                e["line"].as_i64().unwrap(),
                e["identifier"].as_str().unwrap_or_default().to_string(),
                e["message"].as_str().unwrap().to_string(),
                e["tip"].as_str().map(str::to_string),
            )
        })
        .collect()
}

#[test]
fn upstream_rule_tests() {
    let gaps = std::fs::read_to_string(fixtures().join("gaps.txt")).unwrap_or_default();
    let gaps: HashSet<&str> = (gaps.lines())
        .filter(|l| !l.starts_with('#') && !l.is_empty())
        .map(|l| l.split(" # ").next().unwrap())
        .collect();
    let mut cases: Vec<String> = (std::fs::read_dir(fixtures()).unwrap())
        .filter_map(|e| {
            let name = e.unwrap().file_name().to_string_lossy().into_owned();
            let case = name.strip_suffix(".neon")?.to_string();
            (case != "extension").then_some(case)
        })
        .collect();
    cases.sort();
    let mut diffs = Vec::new();
    let mut seen = HashSet::new();
    for case in &cases {
        let (got, ids) = analyse(case);
        let ours = |e: &(String, i64, String, String, Option<String>)| {
            e.2.starts_with("disallowed.") || ids.contains(&e.2)
        };
        let mut got: Found = got.into_iter().filter(ours).collect();
        // the CLI keeps a rule's errors by the identifiers Pun reports
        for e in got.iter().filter(|e| e.2.starts_with("disallowed.")) {
            assert!(
                pun_analyse::reports(&e.2),
                "{case}: {} is not reported",
                e.2
            );
        }
        let mut want: Found = oracle(case).into_iter().filter(ours).collect();
        got.sort();
        want.sort();
        let line = |sign: char, e: &(String, i64, String, String, Option<String>)| {
            format!("{case} {sign} {}:{} {} {}", e.0, e.1, e.2, e.3)
        };
        for e in want.iter().filter(|e| !got.contains(e)) {
            diffs.push(line('-', e));
        }
        for e in got.iter().filter(|e| !want.contains(e)) {
            diffs.push(line('+', e));
        }
    }
    let unexpected: Vec<&String> = diffs
        .iter()
        .filter(|d| !gaps.contains(d.as_str()))
        .collect();
    for d in &diffs {
        seen.insert(d.as_str());
    }
    let stale: Vec<&&str> = gaps.iter().filter(|g| !seen.contains(**g)).collect();
    assert!(
        unexpected.is_empty() && stale.is_empty(),
        "differences from PHPStan:\n{}\n\ngaps that no longer differ:\n{}",
        unexpected
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        stale.iter().map(|s| **s).collect::<Vec<_>>().join("\n"),
    );
}

/// Entries upstream rejects, with its schema or with an `InvalidConfigException` when it builds the rules; pun.toml's
/// options skip the schema, so they reach the rules. Pun leaves each out: `pun-coverage` with them added reports what
/// PHPStan reports for `pun-coverage`. Each names `covB()`, so one kept would replace its entry and its message.
#[test]
fn invalid_entries_are_left_out() {
    let table = |pairs: Vec<(Key, V)>| {
        let mut a = Array::new();
        for (k, v) in pairs {
            a.set(k, v);
        }
        V::Array(a)
    };
    let s = |k: &str| Key::from(k);
    let covb = |option: &str, key: Key, param: V| {
        table(vec![
            (s("function"), V::str("PunCoverage\\covB()")),
            (s(option), table(vec![(key, param)])),
        ])
    };
    let anywhere = "allowParamsAnywhere";
    let entries = [
        // no `position`
        covb(
            anywhere,
            Key::Int(0),
            table(vec![(s("name"), V::str("x")), (s("value"), V::Int(1))]),
        ),
        covb(
            anywhere,
            Key::Int(0),
            table(vec![
                (s("classPattern"), V::str("X")),
                (s("name"), V::str("x")),
            ]),
        ),
        // a float value
        covb(anywhere, Key::Int(1), V::Float(1.5)),
        // EmptyTypeStringInConfigException
        covb(
            anywhere,
            Key::Int(0),
            table(vec![
                (s("position"), V::Int(1)),
                (s("typeString"), V::str("")),
            ]),
        ),
        // a flag that is no integer
        covb(
            "allowParamFlagsAnywhere",
            Key::Int(0),
            table(vec![
                (s("position"), V::Int(1)),
                (s("typeString"), V::str("'a'")),
            ]),
        ),
    ];
    let (got, ids) = analyse_with("pun-coverage", |p| {
        let Some(V::Array(calls)) = p.get_mut(&s("disallowedFunctionCalls")) else {
            panic!("pun-coverage has function entries");
        };
        for e in entries {
            calls.push(e);
        }
        let Some(V::Array(properties)) = p.get_mut(&s("disallowedProperties")) else {
            panic!("pun-coverage has property entries");
        };
        // a property without one `::`
        properties.push(table(vec![(s("property"), V::str("PunCoverage\\Base"))]));
    });
    let ours = |e: &(String, i64, String, String, Option<String>)| {
        e.2.starts_with("disallowed.") || ids.contains(&e.2)
    };
    let mut got: Found = got.into_iter().filter(ours).collect();
    let mut want: Found = oracle("pun-coverage").into_iter().filter(ours).collect();
    got.sort();
    want.sort();
    assert_eq!(got, want);
}
