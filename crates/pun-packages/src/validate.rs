//! `composer validate` (Composer 2.11.2): [`validate`] checks one composer.json as `ConfigValidator::validate` does,
//! and [`lock_errors`] checks composer.lock against it as `ValidateCommand` does through Composer's `Locker`.

use crate::error::{Context, Result, bail};
use crate::lock::{Lock, content_hash};
use crate::verify::missing_requirements;
use pcre2::bytes::Regex;
use serde_json::{Map, Value};
use std::path::Path;
use std::sync::LazyLock;

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
    let (manifest, text) = match read_text(file, display) {
        Ok(read) => read,
        Err(e) => {
            findings.errors.push(format!("{e:#}"));
            return findings;
        }
    };
    let warnings = &mut findings.warnings;
    if let Some((key, line)) = first_duplicate_key(&text) {
        warnings.push(format!(
            "Key {key} is a duplicate in {display} at line {line}"
        ));
    }
    licenses(&manifest, warnings);
    if checks.version && isset(manifest.get("version")) {
        warnings.push("The version field is present, it is recommended to leave it out if the package is published on Packagist.".into());
    }
    if let Some(name) = manifest
        .get("name")
        .filter(|n| !empty(Some(n)))
        .and_then(Value::as_str)
        && name.bytes().any(|b| b.is_ascii_uppercase())
    {
        findings.publish_errors.push(format!(
            "Name \"{name}\" does not match the best practice (e.g. lower-cased/with-dashes). We suggest using \"{}\" instead. As such you will not be able to submit it to Packagist.",
            suggested_name(name)
        ));
    }
    if manifest.get("type").and_then(Value::as_str) == Some("composer-installer") {
        warnings.push("The package type 'composer-installer' is deprecated. Please distribute your custom installers as plugins from now on. See https://getcomposer.org/doc/articles/plugins.md for plugin documentation.".into());
    }
    links(&manifest, warnings);
    scripts(&manifest, warnings);
    let autoload = manifest.get("autoload");
    for kind in ["psr-0", "psr-4"] {
        if isset(autoload.and_then(|a| a.get(kind)).and_then(|p| p.get(""))) {
            warnings.push(format!(
                "Defining autoload.{kind} with an empty namespace prefix is a bad idea for performance"
            ));
        }
    }
    findings
}

/// PHP's `isset()` on a decoded value: present and not null.
fn isset(value: Option<&Value>) -> bool {
    value.is_some_and(|v| !v.is_null())
}

/// PHP's `empty()` on a decoded value: absent, null, false, 0, "", "0", or an empty array or object.
fn empty(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => true,
        Some(Value::Bool(b)) => !b,
        Some(Value::Number(n)) => n.as_f64() == Some(0.0),
        Some(Value::String(s)) => s.is_empty() || s == "0",
        Some(Value::Array(a)) => a.is_empty(),
        Some(Value::Object(o)) => o.is_empty(),
    }
}

/// The SPDX licenses Composer knows (`assets/spdx.json`, from `scripts/export-spdx.php`): lower-cased identifier to
/// whether it is deprecated.
fn spdx_licenses() -> &'static std::collections::HashMap<String, bool> {
    static LICENSES: LazyLock<std::collections::HashMap<String, bool>> = LazyLock::new(|| {
        let data: Value =
            serde_json::from_str(include_str!("../assets/spdx.json")).expect("valid spdx.json");
        (data["licenses"].as_object().expect("licenses").iter())
            .map(|(id, deprecated)| (id.to_ascii_lowercase(), deprecated.as_bool() == Some(true)))
            .collect()
    });
    &LICENSES
}

/// The license warnings: none given, or a deprecated SPDX identifier.
fn licenses(manifest: &Map<String, Value>, warnings: &mut Vec<String>) {
    static PLUS: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^[AL]?GPL-[123](\.[01])?\+$").expect("valid"));
    static PLAIN: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^[AL]?GPL-[123](\.[01])?$").expect("valid"));
    let license = manifest.get("license");
    if empty(license) {
        warnings.push("No license specified, it is recommended to do so. For closed-source software you may use \"proprietary\" as license.".into());
        return;
    }
    // `(array) $manifest['license']`, without "proprietary".
    let all: Vec<&Value> = match license {
        Some(Value::Array(items)) => items.iter().collect(),
        Some(Value::Object(items)) => items.values().collect(),
        Some(other) => vec![other],
        None => vec![],
    };
    for license in all
        .into_iter()
        .filter_map(Value::as_str)
        .filter(|l| *l != "proprietary")
    {
        if spdx_licenses().get(&license.to_ascii_lowercase()) != Some(&true) {
            continue;
        }
        let matches = |re: &Regex| re.is_match(license.as_bytes()).unwrap_or(false);
        warnings.push(if matches(&PLUS) {
            format!(
                "License \"{license}\" is a deprecated SPDX license identifier, use \"{}-or-later\" instead",
                license.replace('+', "")
            )
        } else if matches(&PLAIN) {
            format!(
                "License \"{license}\" is a deprecated SPDX license identifier, use \"{license}-only\" or \"{license}-or-later\" instead"
            )
        } else {
            format!("License \"{license}\" is a deprecated SPDX license identifier, see https://spdx.org/licenses/")
        });
    }
}

/// `ConfigValidator`'s suggestion for a name with capitals: a dash between a lower-case letter and a capital, and
/// before a capital that starts a capitalised word after another capital, then all lower-case.
fn suggested_name(name: &str) -> String {
    static WORDS: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?:([a-z])([A-Z])|([A-Z])([A-Z][a-z]))").expect("valid"));
    let bytes = name.as_bytes();
    let (mut out, mut at) = (Vec::with_capacity(bytes.len() + 4), 0);
    for caps in WORDS
        .captures_iter(bytes)
        .filter_map(std::result::Result::ok)
    {
        let whole = caps.get(0).expect("the match");
        out.extend_from_slice(&bytes[at..whole.start()]);
        let group = |i| caps.get(i).map_or(&[][..], |m| m.as_bytes());
        out.extend_from_slice(group(1));
        out.extend_from_slice(group(3));
        out.push(b'-');
        out.extend_from_slice(group(2));
        out.extend_from_slice(group(4));
        at = whole.end();
    }
    out.extend_from_slice(&bytes[at..]);
    String::from_utf8_lossy(&out).to_ascii_lowercase()
}

/// The requirement warnings: a package in both `require` and `require-dev`, one that `provide` or `replace` already
/// satisfies, and a constraint pointing at a commit.
fn links(manifest: &Map<String, Value>, warnings: &mut Vec<String>) {
    let object = |key: &str| manifest.get(key).and_then(Value::as_object);
    if let (Some(require), Some(require_dev)) = (object("require"), object("require-dev")) {
        let both: Vec<&str> = (require.keys())
            .filter(|k| require_dev.contains_key(*k))
            .map(String::as_str)
            .collect();
        if !both.is_empty() {
            let verb = if both.len() > 1 { "are" } else { "is" };
            warnings.push(format!(
                "{} {verb} required both in require and require-dev, this can lead to unexpected behavior",
                both.join(", ")
            ));
        }
    }
    for link in ["provide", "replace"] {
        let Some(provided) = object(link) else {
            continue;
        };
        for require in ["require", "require-dev"] {
            let Some(required) = object(require) else {
                continue;
            };
            for name in provided.keys().filter(|n| isset(required.get(*n))) {
                warnings.push(format!(
                    "The package {name} in {require} is also listed in {link} which satisfies the requirement. Remove it from {link} if you wish to install it."
                ));
            }
        }
    }
    // `array_merge($require, $requireDev)`: require-dev's constraint for a package in both, at require's place.
    let mut merged: Map<String, Value> = object("require").cloned().unwrap_or_default();
    for (name, constraint) in object("require-dev").into_iter().flatten() {
        merged.insert(name.clone(), constraint.clone());
    }
    for (name, constraint) in &merged {
        if constraint.as_str().is_some_and(|c| c.contains('#')) {
            warnings.push(format!(
                "The package \"{name}\" is pointing to a commit-ref, this is bad practice and can cause unforeseen issues."
            ));
        }
    }
}

/// Descriptions and aliases for scripts that don't exist.
fn scripts(manifest: &Map<String, Value>, warnings: &mut Vec<String>) {
    let scripts = manifest.get("scripts").and_then(Value::as_object);
    let defined = |name: &str| scripts.is_some_and(|s| s.contains_key(name));
    for (key, what) in [
        ("scripts-descriptions", "Description"),
        ("scripts-aliases", "Aliases"),
    ] {
        for name in manifest
            .get(key)
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
            .map(|e| e.0)
        {
            if !defined(name) {
                warnings.push(format!(
                    "{what} for non-existent script \"{name}\" found in \"{key}\""
                ));
            }
        }
    }
}

/// The first duplicate key jsonlint's `DETECT_KEY_CONFLICTS` finds in `text` (valid JSON): the first whose value ends
/// first, as its parser reduces each member then, with the line that value ends on. A key whose earlier value is null
/// is no conflict to it, as it checks with `isset()`.
fn first_duplicate_key(text: &str) -> Option<(String, usize)> {
    let mut scan = Scan {
        text: text.as_bytes(),
        at: 0,
        line: 1,
        end_line: 1,
    };
    scan.value().err()
}

/// A walk over valid JSON that keeps the line each value ends on.
struct Scan<'a> {
    text: &'a [u8],
    at: usize,
    line: usize,
    end_line: usize,
}

impl Scan<'_> {
    fn space(&mut self) {
        while let Some(&b) = self.text.get(self.at).filter(|b| b.is_ascii_whitespace()) {
            self.line += usize::from(b == b'\n');
            self.at += 1;
        }
    }

    /// One value; whether it is null, or the duplicate found inside it.
    fn value(&mut self) -> std::result::Result<bool, (String, usize)> {
        self.space();
        let null = match self.text.get(self.at) {
            Some(b'{') => {
                self.object()?;
                false
            }
            Some(b'[') => {
                self.at += 1;
                self.space();
                while self.text.get(self.at) != Some(&b']') {
                    self.value()?;
                    self.space();
                    self.at += usize::from(self.text.get(self.at) == Some(&b','));
                    self.space();
                }
                self.at += 1;
                false
            }
            Some(b'"') => {
                self.string();
                false
            }
            _ => {
                let start = self.at;
                while self
                    .text
                    .get(self.at)
                    .is_some_and(|b| !b"\t\n\r ,]}".contains(b))
                {
                    self.at += 1;
                }
                self.end_line = self.line;
                return Ok(&self.text[start..self.at] == b"null");
            }
        };
        self.end_line = self.line;
        Ok(null)
    }

    /// An object's members, each checked against the earlier ones once its value ends.
    fn object(&mut self) -> std::result::Result<(), (String, usize)> {
        let mut seen: std::collections::HashMap<String, bool> = std::collections::HashMap::new();
        self.at += 1;
        self.space();
        while self.text.get(self.at) != Some(&b'}') {
            let key = self.string();
            self.space();
            self.at += 1; // the colon
            let null = self.value()?;
            if seen.get(&key) == Some(&false) {
                return Err((key, self.end_line));
            }
            seen.insert(key, null);
            self.space();
            self.at += usize::from(self.text.get(self.at) == Some(&b','));
            self.space();
        }
        self.at += 1;
        Ok(())
    }

    /// A string, decoded.
    fn string(&mut self) -> String {
        let start = self.at;
        self.at += 1;
        while let Some(&b) = self.text.get(self.at) {
            self.at += 1;
            match b {
                b'\\' => self.at += 1,
                b'"' => break,
                _ => {}
            }
        }
        serde_json::from_slice(&self.text[start..self.at]).unwrap_or_default()
    }
}

/// The composer.json at `file` (`display` as the user named it), which must be a JSON object.
pub fn read(file: &Path, display: &str) -> Result<Map<String, Value>> {
    read_text(file, display).map(|(manifest, _)| manifest)
}

fn read_text(file: &Path, display: &str) -> Result<(Map<String, Value>, String)> {
    let text =
        std::fs::read_to_string(file).with_context(|| format!("\"{display}\" is not readable"))?;
    match serde_json::from_str(&text) {
        Ok(Value::Object(manifest)) => Ok((manifest, text)),
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
