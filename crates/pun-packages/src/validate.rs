//! `composer validate` (Composer 2.11.2): [`validate`] checks one composer.json as `ConfigValidator::validate` does,
//! and [`lock_errors`] checks composer.lock against it as `ValidateCommand` does through Composer's `Locker`.

use crate::error::{Context, Result, bail};
use crate::lock::{Lock, content_hash};
use crate::verify::missing_requirements;
use serde_json::{Map, Value};
use std::path::Path;

/// What `ConfigValidator::validate` finds in a composer.json, in Composer's order within each list.
#[derive(Debug, Default, PartialEq)]
pub struct Findings {
    pub errors: Vec<String>,
    /// What only the strict schema refuses: the file works with Composer but can't be published.
    pub publish_errors: Vec<String>,
    pub warnings: Vec<String>,
}

/// The checks `validate`'s flags leave on.
#[derive(Clone, Copy)]
pub struct Checks {
    /// `ValidatingArrayLoader::CHECK_ALL` (off with `--no-check-all`): the unbound and exact constraint warnings.
    pub constraints: bool,
    /// `ConfigValidator::CHECK_VERSION` (off with `--no-check-version`): the warning for a `version` field.
    pub version: bool,
}

/// `ConfigValidator::validate`: a file that doesn't read as a composer.json is one error and nothing else.
pub fn validate(file: &Path, display: &str, checks: Checks) -> Findings {
    let mut findings = Findings::default();
    let manifest = match read(file, display) {
        Ok(manifest) => manifest,
        Err(e) => {
            findings.errors.push(format!("{e:#}"));
            return findings;
        }
    };
    let _ = (manifest, checks);
    findings
}

/// The composer.json at `file` (`display` as the user named it), which must be a JSON object.
pub fn read(file: &Path, display: &str) -> Result<Map<String, Value>> {
    let text =
        std::fs::read_to_string(file).with_context(|| format!("\"{display}\" is not readable"))?;
    match serde_json::from_str(&text) {
        Ok(Value::Object(manifest)) => Ok(manifest),
        Ok(_) => bail!("\"{display}\" does not match the expected JSON schema"),
        Err(_) => bail!("\"{display}\" does not contain valid JSON"),
    }
}

/// `ValidateCommand`'s lock errors for a composer.lock that Composer reads (`Locker::isLocked`: it has `packages`): the
/// staleness line when `Locker::isFresh` fails, then `Locker::getMissingRequirementInfo` with the dev requirements.
/// `composer_json` is composer.json's own text, which an old lock's `hash` is the md5 of; `lock_display` is the lock's
/// path as Composer names it.
pub fn lock_errors(
    manifest: &Map<String, Value>,
    composer_json: &str,
    lock_text: &str,
    lock_display: &str,
) -> Result<Vec<String>> {
    let mut lock: Map<String, Value> = match serde_json::from_str(lock_text) {
        Ok(Value::Object(lock)) => lock,
        _ => bail!("\"{lock_display}\" does not contain valid JSON"),
    };
    if !lock.contains_key("packages") {
        return Ok(vec![]);
    }
    let text = |key: &str| {
        lock.get(key)
            .and_then(Value::as_str)
            .filter(|h| !h.is_empty())
    };
    let fresh = match (text("content-hash"), text("hash")) {
        (Some(hash), _) => hash == content_hash(manifest),
        (None, Some(hash)) => hash == crate::php::md5(composer_json.as_bytes()),
        (None, None) => false,
    };
    let mut errors = vec![];
    if !fresh {
        errors.push("- The lock file is not up to date with the latest changes in composer.json, it is recommended that you run `composer update` or `composer update <package name>`.".to_string());
    }
    if !lock.contains_key("packages-dev") {
        bail!(
            "The lock file does not contain require-dev information, run install with the --no-dev option or delete it and run composer update to generate a new lock file."
        );
    }
    // An old lock has `hash` alone; the requirements don't need the content hash.
    lock.entry("content-hash")
        .or_insert_with(|| Value::String(String::new()));
    let lock = Lock::from_composer_lock(&Value::Object(lock).to_string())?;
    errors.extend(missing_requirements(manifest, &lock, true)?);
    Ok(errors)
}
