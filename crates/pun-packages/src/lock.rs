use crate::error::{Context, Result, bail};
use crate::php::json_compact;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::Path;

const SETTINGS: [&str; 9] = [
    "minimum-stability",
    "stability-flags",
    "prefer-stable",
    "prefer-lowest",
    "platform",
    "platform-dev",
    "platform-overrides",
    "plugin-api-version",
    "aliases",
];

#[derive(Debug, Clone, PartialEq)]
pub struct Lock {
    pub content_hash: String,
    pub settings: Map<String, Value>,
    pub packages: Vec<LockedPackage>,
    pub packages_dev: Vec<LockedPackage>,
}

/// One package of a lock, in composer.lock's shape. The keys keep the order they were read in, so a lock writes
/// back byte for byte; the accessors name the keys Pun reads.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LockedPackage(pub Map<String, Value>);

impl std::ops::Deref for LockedPackage {
    type Target = Map<String, Value>;

    fn deref(&self) -> &Map<String, Value> {
        &self.0
    }
}

impl LockedPackage {
    fn str(&self, key: &str) -> Option<&str> {
        self.0.get(key).and_then(Value::as_str)
    }

    pub fn pretty_name(&self) -> &str {
        self.str("name").unwrap_or_default()
    }

    pub fn name(&self) -> String {
        self.pretty_name().to_ascii_lowercase()
    }

    pub fn version(&self) -> &str {
        self.str("version").unwrap_or_default()
    }

    pub fn kind(&self) -> Option<&str> {
        self.str("type")
    }

    pub fn dist(&self) -> Option<&Map<String, Value>> {
        self.0.get("dist").and_then(Value::as_object)
    }

    /// The commit or version the package installs at: the dist's, else the source's, as Composer reads it.
    pub fn reference(&self) -> Option<&str> {
        let get = |k: &str| {
            self.0
                .get(k)?
                .get("reference")?
                .as_str()
                .filter(|r| !r.is_empty())
        };
        if self.0.contains_key("dist") {
            get("dist").or_else(|| get("source"))
        } else {
            get("source")
        }
    }

    pub fn bins(&self) -> impl Iterator<Item = &str> {
        (self.0.get("bin").and_then(Value::as_array))
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
    }

    pub fn link_targets(&self, kind: &str) -> impl Iterator<Item = String> {
        (self.0.get(kind).and_then(Value::as_object))
            .into_iter()
            .flatten()
            .map(|(target, _)| target.to_ascii_lowercase())
    }
}

pub fn content_hash(manifest: &Map<String, Value>) -> String {
    const RELEVANT: [&str; 11] = [
        "name",
        "version",
        "require",
        "require-dev",
        "conflict",
        "replace",
        "provide",
        "minimum-stability",
        "prefer-stable",
        "repositories",
        "extra",
    ];
    let mut relevant: Vec<(String, Value)> = (manifest.iter())
        .filter(|(k, _)| RELEVANT.contains(&k.as_str()))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let mut config = Map::new();
    for key in ["platform", "policy"] {
        // `isset()`: a null value is left out.
        if let Some(v) = (manifest.get("config").and_then(|c| c.get(key))).filter(|v| !v.is_null())
        {
            config.insert(key.to_string(), v.clone());
        }
    }
    if !config.is_empty() {
        relevant.push(("config".into(), Value::Object(config)));
    }
    relevant.sort_by(|a, b| a.0.cmp(&b.0));
    let json = json_compact(&Value::Object(relevant.into_iter().collect()));
    crate::php::md5(json.as_bytes())
}

impl Lock {
    pub fn from_composer_lock(text: &str) -> Result<Lock> {
        let json: Value = serde_json::from_str(text).context("composer.lock is not valid JSON")?;
        let Value::Object(mut root) = json else {
            bail!("composer.lock must hold an object");
        };
        let content_hash = match root.get("content-hash") {
            Some(Value::String(h)) => h.clone(),
            _ => bail!("composer.lock has no content-hash"),
        };
        let mut packages = |key: &str| -> Result<Vec<LockedPackage>> {
            match root.shift_remove(key) {
                None => Ok(vec![]),
                Some(Value::Array(items)) => (items.into_iter())
                    .map(|p| match p {
                        Value::Object(m) if m.get("name").is_some_and(Value::is_string) => {
                            Ok(LockedPackage(m))
                        }
                        _ => bail!(
                            "composer.lock: every entry of {key} must be a package with a name"
                        ),
                    })
                    .collect(),
                Some(_) => bail!("composer.lock: {key} must be a list"),
            }
        };
        let (packages, packages_dev) = (packages("packages")?, packages("packages-dev")?);
        let settings = (SETTINGS.iter())
            .filter_map(|k| Some((k.to_string(), root.get(*k)?.clone())))
            .collect();
        Ok(Lock {
            content_hash,
            settings,
            packages,
            packages_dev,
        })
    }

    pub fn read(root: &Path) -> Result<Option<Lock>> {
        let path = root.join("composer.lock");
        match std::fs::read_to_string(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e).with_context(|| format!("can't read {}", path.display())),
            Ok(text) => Lock::from_composer_lock(&text)
                .with_context(|| format!("invalid {}", path.display()))
                .map(Some),
        }
    }

    pub fn to_composer_lock(&self) -> String {
        const README: [&str; 3] = [
            "This file locks the dependencies of your project to a known state",
            "Read more about it at https://getcomposer.org/doc/01-basic-usage.md#installing-dependencies",
            "This file is @generated automatically",
        ];
        let mut lock = Map::new();
        lock.insert(
            "_readme".into(),
            README.iter().map(|s| Value::from(*s)).collect(),
        );
        lock.insert("content-hash".into(), self.content_hash.clone().into());
        for (key, list) in [
            ("packages", &self.packages),
            ("packages-dev", &self.packages_dev),
        ] {
            lock.insert(
                key.into(),
                list.iter().map(|p| Value::Object(p.0.clone())).collect(),
            );
        }
        let defaults = [
            ("aliases", Value::Array(vec![])),
            ("minimum-stability", "stable".into()),
            ("stability-flags", Value::Object(Map::new())),
            ("prefer-stable", false.into()),
            ("prefer-lowest", false.into()),
            ("platform", Value::Object(Map::new())),
            ("platform-dev", Value::Object(Map::new())),
        ];
        for (key, default) in defaults {
            lock.insert(
                key.into(),
                self.settings.get(key).cloned().unwrap_or(default),
            );
        }
        for key in ["platform-overrides", "plugin-api-version"] {
            if let Some(v) = self.settings.get(key) {
                lock.insert(key.into(), v.clone());
            }
        }
        let mut text = crate::php::json_pretty(&Value::Object(lock));
        for key in ["stability-flags", "platform", "platform-dev"] {
            text = text.replace(
                &format!("\n    \"{key}\": [],"),
                &format!("\n    \"{key}\": {{}},"),
            );
        }
        text.push('\n');
        text
    }

    pub fn all(&self) -> impl Iterator<Item = &LockedPackage> {
        self.packages.iter().chain(&self.packages_dev)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const COMPOSER_LOCK: &str = r#"{
    "_readme": ["This file locks the dependencies of your project to a known state"],
    "content-hash": "0f2b0c8f6a1d4e1b9c3a7d5e8f601234",
    "packages": [
        {
            "name": "psr/log",
            "version": "3.0.2",
            "source": {"type": "git", "url": "https://github.com/php-fig/log.git", "reference": "f16e1d5863e37f8d8c2a01719f5b34baa2b714d3"},
            "dist": {"type": "zip", "url": "https://api.github.com/repos/php-fig/log/zipball/f16e1d5863e37f8d8c2a01719f5b34baa2b714d3", "reference": "f16e1d5863e37f8d8c2a01719f5b34baa2b714d3", "shasum": ""},
            "require": {"php": ">=8.0.0"},
            "type": "library",
            "extra": {"branch-alias": {"dev-master": "3.x-dev"}},
            "autoload": {"psr-4": {"Psr\\Log\\": "src"}},
            "notification-url": "https://packagist.org/downloads/",
            "license": ["MIT"],
            "authors": [{"name": "PHP-FIG", "homepage": "https://www.php-fig.org/"}],
            "description": "Common interface for logging libraries",
            "keywords": ["log", "psr", "psr-3"],
            "support": {"source": "https://github.com/php-fig/log/tree/3.0.2"},
            "time": "2024-09-11T13:17:53+00:00"
        }
    ],
    "packages-dev": [
        {"name": "acme/tool", "version": "dev-main", "dist": {"type": "path", "url": "packages/tool", "reference": null}, "type": "metapackage", "extra": {}}
    ],
    "aliases": [],
    "minimum-stability": "stable",
    "stability-flags": {"acme/tool": 20},
    "prefer-stable": true,
    "prefer-lowest": false,
    "platform": {"php": "^8.4"},
    "platform-dev": {},
    "plugin-api-version": "2.9.0"
}"#;

    /// What Composer 2.11.2's `update --no-install` wrote for a path package.
    const WRITTEN: &str = r#"{
    "_readme": [
        "This file locks the dependencies of your project to a known state",
        "Read more about it at https://getcomposer.org/doc/01-basic-usage.md#installing-dependencies",
        "This file is @generated automatically"
    ],
    "content-hash": "b813925a3a3cb24452a1367848daf51c",
    "packages": [],
    "packages-dev": [
        {
            "name": "acme/tool",
            "version": "1.0.0",
            "dist": {
                "type": "path",
                "url": "packages/tool"
            },
            "type": "library",
            "transport-options": {
                "symlink": true,
                "relative": true
            }
        }
    ],
    "aliases": [],
    "minimum-stability": "dev",
    "stability-flags": {},
    "prefer-stable": true,
    "prefer-lowest": false,
    "platform": {},
    "platform-dev": {},
    "plugin-api-version": "2.11.0"
}
"#;

    #[test]
    fn composer_lock_reads_and_writes_back_as_composer_wrote_it() {
        let lock = Lock::from_composer_lock(COMPOSER_LOCK).unwrap();
        assert_eq!(lock.content_hash, "0f2b0c8f6a1d4e1b9c3a7d5e8f601234");
        assert_eq!((lock.packages.len(), lock.packages_dev.len()), (1, 1));
        let want: Value = serde_json::from_str(COMPOSER_LOCK).unwrap();
        assert_eq!(
            Value::Object(lock.packages[0].0.clone()),
            want["packages"][0]
        );
        assert_eq!(
            Value::Object(lock.packages_dev[0].0.clone()),
            want["packages-dev"][0]
        );
        assert_eq!(lock.all().count(), 2);
        assert_eq!(
            Lock::from_composer_lock(WRITTEN)
                .unwrap()
                .to_composer_lock(),
            WRITTEN
        );
    }

    #[test]
    fn references() {
        let r = |v: Value| {
            LockedPackage(v.as_object().unwrap().clone())
                .reference()
                .map(String::from)
        };
        assert_eq!(
            r(json!({"dist": {"reference": "d"}, "source": {"reference": "s"}})).as_deref(),
            Some("d")
        );
        assert_eq!(
            r(json!({"dist": {"reference": ""}, "source": {"reference": "s"}})).as_deref(),
            Some("s")
        );
        assert_eq!(
            r(json!({"source": {"reference": "s"}})).as_deref(),
            Some("s")
        );
        assert_eq!(r(json!({"dist": {"url": "u"}})), None);
    }

    #[test]
    fn content_hash_matches_composer() {
        let manifest = json!({
            "name": "acme/app",
            "description": "left out",
            "require": {"php": "^8.4", "laravel/framework": "^13.0"},
            "autoload": {"psr-4": {"App\\": "app/"}},
            "extra": {"laravel": {"dont-discover": []}},
            "config": {"sort-packages": true, "platform": {"php": "8.4.1"}},
            "minimum-stability": "stable",
            "prefer-stable": true
        });
        assert_eq!(
            content_hash(manifest.as_object().unwrap()),
            "1f6b30e94de0732cc8351f3d4a2ecc8c"
        );
        // `isset()`: a null config.platform or config.policy is left out, an empty or false one is not
        // (`Locker::getContentHash` of Composer 2.11.2).
        let hash = |v: Value| content_hash(v.as_object().unwrap());
        assert_eq!(
            hash(json!({"name": "acme/app", "config": {"platform": null, "policy": null}})),
            "d9a7311a656b3b9666e0e3ecc3877138"
        );
        assert_eq!(
            hash(json!({"name": "acme/app", "config": {"platform": null, "policy": {"x": 1}}})),
            "d06e7312cfd6483163d443be57c573a3"
        );
        assert_eq!(
            hash(json!({"name": "acme/app", "config": {"platform": {}, "policy": false}})),
            "1fd3c97901be068ed5b049d753a16c02"
        );
    }

    #[test]
    fn bad_locks_are_errors() {
        let lerr = |t: &str| Lock::from_composer_lock(t).unwrap_err().to_string();
        assert_eq!(lerr("[]"), "composer.lock must hold an object");
        assert_eq!(lerr("{}"), "composer.lock has no content-hash");
        assert_eq!(
            lerr(r#"{"content-hash":"x","packages":{}}"#),
            "composer.lock: packages must be a list"
        );
        assert_eq!(
            lerr(r#"{"content-hash":"x","packages":[1]}"#),
            "composer.lock: every entry of packages must be a package with a name"
        );
        assert_eq!(lerr("{"), "composer.lock is not valid JSON");
        let empty = Lock::from_composer_lock(r#"{"content-hash":"x"}"#).unwrap();
        assert!(empty.packages.is_empty() && empty.packages_dev.is_empty());
    }

    #[test]
    fn reads_composer_lock_from_the_project() {
        let d = std::env::temp_dir().join(format!("pun-lock-read-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let path = d.join("composer.lock");
        assert!(Lock::read(&d).unwrap().is_none());
        std::fs::write(&path, r#"{"content-hash": "abc"}"#).unwrap();
        assert_eq!(Lock::read(&d).unwrap().unwrap().content_hash, "abc");
        std::fs::write(&path, "{}").unwrap();
        let err = Lock::read(&d).unwrap_err();
        assert_eq!(err.to_string(), format!("invalid {}", path.display()));
        assert_eq!(
            std::error::Error::source(&err).unwrap().to_string(),
            "composer.lock has no content-hash"
        );
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        let err = Lock::read(&d).unwrap_err().to_string();
        assert_eq!(err, format!("can't read {}", path.display()));
        std::fs::remove_dir_all(&d).unwrap();
    }
}
