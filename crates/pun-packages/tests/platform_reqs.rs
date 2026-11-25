//! `pun_packages::platform_reqs` against `composer check-platform-reqs` 2.11.2, on the cases and the platform of
//! `pun-cli/tests/fixtures/check_platform_reqs.json` (`scripts/export-platform-reqs-cases.py`): the table or JSON each
//! case prints, its exit code, where its packages came from, and the errors that stop it.

use pun_packages::platform_reqs::{Source, check};
use serde_json::{Map, Value};
use std::path::Path;

const FIXTURE: &str = include_str!("../../pun-cli/tests/fixtures/check_platform_reqs.json");

fn write(dir: &Path, case: &Value) {
    for (path, content) in case["files"].as_object().unwrap() {
        let file = dir.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, content.as_str().unwrap()).unwrap();
    }
}

/// The case's options: `-d`, `--no-dev`, `--lock`, and whether it asks for JSON.
fn options(args: &[&str]) -> (String, bool, bool, bool) {
    let mut working_dir = ".".to_string();
    let mut json = false;
    let mut words = args.iter();
    while let Some(&word) = words.next() {
        match word {
            "-d" => working_dir = (*words.next().unwrap()).to_string(),
            "--format" | "-f" => json = *words.next().unwrap() == "json",
            w if w.starts_with("--format=") => json = w == "--format=json",
            _ => {}
        }
    }
    (
        working_dir,
        args.contains(&"--no-dev"),
        args.contains(&"--lock"),
        json,
    )
}

/// The line Composer starts with, for where the packages came from.
fn heading(source: Source, no_dev: bool) -> String {
    let non_dev = if no_dev { "non-dev " } else { "" };
    match source {
        Source::Lock => format!("Checking {non_dev}platform requirements using the lock file"),
        Source::Vendor => {
            format!("Checking {non_dev}platform requirements for packages in the vendor dir")
        }
        Source::NoVendor => format!(
            "No vendor dir present, checking {non_dev}platform requirements from the lock file"
        ),
    }
}

#[test]
fn reports_as_composer_does() {
    let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
    let mut failures = vec![];
    for case in fixture["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let args: Vec<&str> = (case["args"].as_array().unwrap().iter())
            .map(|a| a.as_str().unwrap())
            .collect();
        let (working_dir, no_dev, lock, json) = options(&args);
        let dir =
            std::env::temp_dir().join(format!("pun-platform-lib-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        write(&dir, case);
        let base = dir.join(&working_dir);
        let file = case["env"]["COMPOSER"].as_str().unwrap_or("composer.json");
        let lock_file = file
            .strip_suffix(".json")
            .map_or_else(|| format!("{file}.lock"), |stem| format!("{stem}.lock"));
        // A missing or unparsable composer.json is pun-cli's to report.
        let Some(manifest) = std::fs::read(base.join(file))
            .ok()
            .and_then(|text| serde_json::from_slice::<Map<String, Value>>(&text).ok())
        else {
            std::fs::remove_dir_all(&dir).unwrap();
            continue;
        };
        let vendor = (manifest
            .get("config")
            .and_then(|c| c.get("vendor-dir"))
            .and_then(Value::as_str))
        .unwrap_or("vendor")
        .to_string();
        let result = check(
            &base.join(&lock_file),
            &manifest,
            &base.join(vendor),
            &fixture["probe"],
            no_dev,
            lock,
        );
        std::fs::remove_dir_all(&dir).unwrap();
        let difference = match (result, case.get("message").and_then(Value::as_array)) {
            (Err(e), Some(message)) => {
                let got = format!("{e:#}");
                let lines: Vec<&str> = message.iter().filter_map(Value::as_str).collect();
                (!lines.iter().all(|l| got.contains(l)))
                    .then(|| format!("want the lines {lines:?}\ngot {got}"))
            }
            (Ok(report), None) => {
                let stdout = if json {
                    format!("{}\n", report.json())
                } else {
                    report.text()
                };
                let got = (
                    i64::from(report.exit),
                    stdout,
                    format!("{}\n", heading(report.source, no_dev)),
                );
                let want = (
                    case["exit"].as_i64().unwrap(),
                    case["stdout"].as_str().unwrap().to_string(),
                    case["stderr"].as_str().unwrap().to_string(),
                );
                (got != want).then(|| format!("want {want:#?}\ngot {got:#?}"))
            }
            (Err(e), None) => Some(format!("an error Composer doesn't give: {e:#}")),
            (Ok(_), Some(message)) => Some(format!("no error, where Composer gives {message:?}")),
        };
        if let Some(difference) = difference {
            failures.push(format!("=== {name}\n{difference}"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} cases differ:\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}
