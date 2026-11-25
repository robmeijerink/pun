//! `composer check-platform-reqs` (Composer 2.11.2's `CheckPlatformReqsCommand`): each platform requirement of the
//! installed (or locked) packages and of the root, checked against the real platform, `config.platform` aside.

use crate::constraint::{Constraint, composer_string};
use crate::error::{Result, bail};
use crate::resolver::package::{Link, Origin, Package, alias_package, load};
use crate::resolver::platform::from_probe;
use crate::version::is_platform;
use serde_json::{Map, Value, json};
use std::fmt::Write as _;
use std::path::Path;

/// Where the packages came from, which the first line names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// `--lock`.
    Lock,
    /// The packages installed in the vendor dir.
    Vendor,
    /// The lock, as the vendor dir has nothing installed.
    NoVendor,
}

/// A requirement that failed: the link and the package it is from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failed {
    pub source: String,
    /// `Link::getDescription`: "requires", or "requires (for development)" for the root's require-dev.
    pub description: String,
    pub target: String,
    /// The constraint as composer/semver writes it (`Link::__toString`).
    pub constraint: String,
    /// The constraint as written in composer.json.
    pub pretty: String,
}

/// One line of the result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub name: String,
    pub version: String,
    pub failed: Option<Failed>,
    /// "success", "failed" or "missing".
    pub status: &'static str,
    /// The package that provides or replaces the platform package, when not the platform itself.
    pub provider: Option<String>,
}

pub struct Report {
    pub source: Source,
    pub rows: Vec<Row>,
    /// 0, else 1 for a requirement the platform doesn't meet, else 2 for one it lacks.
    pub exit: u8,
}

/// A requirement's links: the package each is from and its description.
type Links = Vec<(usize, &'static str, Link)>;

/// What `check-platform-reqs` finds for the root package `manifest`, its lock at `lock_file` (named `lock_display` in
/// messages) and its vendor dir `vendor`, on the platform `probe` describes.
pub fn check(
    lock_file: &Path,
    lock_display: &str,
    manifest: &Map<String, Value>,
    vendor: &Path,
    probe: &Value,
    no_dev: bool,
    lock: bool,
) -> Result<Report> {
    let mut arena: Vec<Package> = vec![];
    let mut dev_names: Vec<String> = vec![];
    let (source, installed) = if lock {
        (
            Source::Lock,
            locked(lock_file, lock_display, !no_dev, &mut arena)?,
        )
    } else {
        let local = local(vendor, &mut arena, &mut dev_names)?;
        if local.is_empty() {
            // The lock leaves its dev packages out itself; the vendor dir's dev package names don't apply.
            dev_names.clear();
            (
                Source::NoVendor,
                locked(lock_file, lock_display, !no_dev, &mut arena)?,
            )
        } else {
            if !no_dev {
                dev_names.clear();
            }
            (Source::Vendor, local)
        }
    };

    // The root package: `RootPackageRepository`, after the installed or locked packages.
    let mut root_data = manifest.clone();
    root_data.entry("name").or_insert_with(|| "__root__".into());
    root_data.entry("version").or_insert_with(|| "1.0.0".into());
    let root_ids = load(root_data, Origin::Root, &mut arena)?;
    let repository: Vec<usize> = installed.iter().chain(&root_ids).copied().collect();
    let requires = requirements(
        &arena,
        &repository,
        *root_ids.last().expect("the root"),
        &dev_names,
        no_dev,
    );

    // The platform, after the installed packages and the root.
    let mut candidates_from = repository;
    for data in from_probe(probe, None)? {
        candidates_from.extend(load(data, Origin::Platform, &mut arena)?);
    }

    let mut rows = vec![];
    let mut exit = 0;
    for (target, links) in requires.iter().filter(|(t, _)| is_platform(t)) {
        exit = exit.max(requirement(
            &arena,
            &candidates_from,
            target,
            links,
            &mut rows,
        ));
    }
    Ok(Report { source, rows, exit })
}

/// Each requirement and its links, by name (`ksort`): the root's require-dev first, unless `no_dev`, then each
/// package's requirements but the dev packages', the root's last.
fn requirements(
    arena: &[Package],
    repository: &[usize],
    root: usize,
    dev_names: &[String],
    no_dev: bool,
) -> Vec<(String, Links)> {
    let mut requires: Vec<(String, Links)> = vec![];
    let mut add = |link: &Link, from: (usize, &'static str), replace: bool| {
        let entry = (from.0, from.1, link.clone());
        match requires.iter_mut().find(|(t, _)| *t == link.target) {
            Some((_, links)) if replace => *links = vec![entry],
            Some((_, links)) => links.push(entry),
            None => requires.push((link.target.clone(), vec![entry])),
        }
    };
    if !no_dev {
        for link in &arena[root].dev_requires {
            add(link, (root, "requires (for development)"), true);
        }
    }
    for &id in repository
        .iter()
        .filter(|&&id| !dev_names.contains(&arena[id].name))
    {
        for link in &arena[id].requires {
            add(link, (id, "requires"), false);
        }
    }
    requires.sort_by(|a, b| a.0.cmp(&b.0));
    requires
}

/// One platform requirement's rows, and its exit code: the first candidate (named so, or providing or replacing it)
/// that meets every link is a success; else each candidate's first failed link, or "missing" with no candidate.
fn requirement(
    arena: &[Package],
    from: &[usize],
    target: &str,
    links: &Links,
    rows: &mut Vec<Row>,
) -> u8 {
    let candidates = providers(arena, from, target);
    if candidates.is_empty() {
        let (source, description, link) = &links[0];
        rows.push(Row {
            name: target.to_string(),
            version: "n/a".into(),
            failed: Some(failed(&arena[*source], description, link)),
            status: "missing",
            provider: None,
        });
        return 2;
    }
    let mut failures = vec![];
    for &candidate in &candidates {
        let package = &arena[candidate];
        let own = package.name == target;
        let (constraint, shown) = if own {
            (
                Constraint::exact(&package.version),
                package.pretty_version.clone(),
            )
        } else {
            let link = (package.provides.iter().chain(&package.replaces))
                .find(|l| l.target == target)
                .expect("a candidate not named so provides or replaces it");
            // `self.version` was parsed from the version of the package that wrote it: an alias keeps its package's
            // link first, so that version shows.
            let shown = if link.pretty == "self.version" {
                package
                    .alias_of
                    .map_or(package, |of| &arena[of])
                    .pretty_version
                    .clone()
            } else {
                link.pretty.clone()
            };
            (link.constraint.clone(), shown)
        };
        let name = if own {
            package.pretty_name.clone()
        } else {
            target.to_string()
        };
        let provider = (!own).then(|| format!("provided by {}", package.pretty_name));
        let Some((source, description, link)) = links
            .iter()
            .find(|(_, _, l)| !l.constraint.matches(&constraint))
        else {
            rows.push(Row {
                name,
                version: shown,
                failed: None,
                status: "success",
                provider,
            });
            return 0;
        };
        failures.push(Row {
            name,
            version: shown,
            failed: Some(failed(&arena[*source], description, link)),
            status: "failed",
            provider,
        });
    }
    rows.extend(failures);
    1
}

fn failed(from: &Package, description: &str, link: &Link) -> Failed {
    Failed {
        source: from.name.clone(),
        description: description.to_string(),
        target: link.target.clone(),
        constraint: composer_string(&link.constraint),
        pretty: link.pretty.clone(),
    }
}

/// `InstalledRepository::findPackagesWithReplacersAndProviders`: each package named `target`, or providing or replacing
/// it, in the repositories' order.
fn providers(arena: &[Package], from: &[usize], target: &str) -> Vec<usize> {
    (from.iter().copied())
        .filter(|&p| {
            let package = &arena[p];
            package.name == target
                || package
                    .provides
                    .iter()
                    .chain(&package.replaces)
                    .any(|l| l.target == target)
        })
        .collect()
}

/// The local repository (vendor/composer/installed.json), each package and its branch alias, without those whose
/// install path is gone (`Factory::purgePackages`; a metapackage has none to lose); `dev_names` gets its
/// `dev-package-names`.
fn local(
    vendor: &Path,
    arena: &mut Vec<Package>,
    dev_names: &mut Vec<String>,
) -> Result<Vec<usize>> {
    let path = vendor.join("composer/installed.json");
    let Ok(text) = std::fs::read(&path) else {
        return Ok(vec![]);
    };
    // `FilesystemRepository::initialize`'s error, around the reason. Composer follows a JSON error with jsonlint's
    // description of it (context/PLAN.md §3).
    let invalid = |why: &str| {
        format!(
            "Invalid repository data in {}, packages could not be loaded: {why}",
            path.display()
        )
    };
    let Ok(data) = serde_json::from_slice::<Value>(&text) else {
        bail!(
            "{}",
            invalid(&format!(
                "[Seld\\JsonLint\\ParsingException] \"{}\" does not contain valid JSON",
                path.display()
            ))
        );
    };
    // Composer 2's `{"packages": [...]}`, or Composer 1's list.
    let packages = match data.get("packages").filter(|p| !p.is_null()) {
        Some(packages) => packages.clone(),
        None => data.clone(),
    };
    let packages: Vec<Value> = match packages {
        Value::Array(list) => list,
        Value::Object(map) => map.into_iter().map(|(_, p)| p).collect(),
        _ => bail!(
            "{}",
            invalid("[UnexpectedValueException] Could not parse package list from the repository")
        ),
    };
    dev_names.extend(
        (data
            .get("dev-package-names")
            .and_then(Value::as_array)
            .into_iter()
            .flatten())
        .filter_map(Value::as_str)
        .map(str::to_ascii_lowercase),
    );
    let mut ids = vec![];
    for package in packages.iter().filter_map(Value::as_object) {
        let loaded = load(package.clone(), Origin::Locked, arena)?;
        let base = &arena[*loaded.last().expect("the package")];
        if base.kind == "metapackage" || vendor.join(&base.pretty_name).exists() {
            ids.extend(loaded);
        }
    }
    Ok(ids)
}

/// `Locker::getLockedRepository`: the lock's packages, with `packages-dev` when `dev`, each branch alias before its
/// package, then the root's inline aliases the lock records.
fn locked(
    lock_file: &Path,
    display: &str,
    dev: bool,
    arena: &mut Vec<Package>,
) -> Result<Vec<usize>> {
    let Ok(text) = std::fs::read(lock_file) else {
        bail!("No lockfile found. Unable to read locked packages");
    };
    // Composer follows this with jsonlint's description of the error (context/PLAN.md §3). A lock that isn't an object
    // has no `packages`, which PHP warns about; without that warning it has no `packages-dev` either.
    let Ok(data) = serde_json::from_slice::<Value>(&text) else {
        bail!("\"{display}\" does not contain valid JSON");
    };
    let list = |key: &str| {
        data.get(key)
            .filter(|v| !v.is_null())
            .and_then(Value::as_array)
            .cloned()
    };
    if dev && list("packages-dev").is_none() {
        bail!(
            "The lock file does not contain require-dev information, run install with the --no-dev option or delete it and run composer update to generate a new lock file."
        );
    }
    let packages = list("packages").unwrap_or_default();
    let dev_packages = if dev {
        list("packages-dev").unwrap_or_default()
    } else {
        vec![]
    };
    let mut ids = vec![];
    for package in packages
        .iter()
        .chain(&dev_packages)
        .filter_map(Value::as_object)
    {
        ids.extend(load(package.clone(), Origin::Locked, arena)?);
    }
    for alias in data
        .get("aliases")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let field = |k: &str| {
            alias
                .get(k)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        let of = (ids.iter().copied())
            .find(|&p| arena[p].alias_of.is_none() && arena[p].name == field("package"));
        if let Some(of) = of {
            ids.push(alias_package(
                arena,
                of,
                field("alias_normalized"),
                field("alias"),
                true,
            ));
        }
    }
    Ok(ids)
}

impl Report {
    /// The table `renderTable` prints: name, version, the failed link (as `Link::__toString` and with its constraint
    /// as written), and the status, each cell padded to its column and followed by a space.
    pub fn text(&self) -> String {
        let cells: Vec<[String; 5]> = (self.rows.iter())
            .map(|row| {
                let (link, written) = row.failed.as_ref().map_or_else(Default::default, |f| {
                    (
                        format!(
                            "{} {} {} ({})",
                            f.source, f.description, f.target, f.constraint
                        ),
                        format!("{} {} {} ({})", f.source, f.description, f.target, f.pretty),
                    )
                });
                let status = match &row.provider {
                    Some(p) => format!("{} {p}", row.status),
                    None => row.status.to_string(),
                };
                [row.name.clone(), row.version.clone(), link, written, status]
            })
            .collect();
        let width = |i: usize| {
            cells
                .iter()
                .map(|c| c[i].chars().count())
                .max()
                .unwrap_or(0)
        };
        let widths = [width(0), width(1), width(2), width(3), width(4)];
        let mut out = String::new();
        for row in &cells {
            for (cell, width) in row.iter().zip(widths) {
                let _ = write!(out, "{cell:width$} ");
            }
            out.push('\n');
        }
        out
    }

    /// The JSON `--format=json` prints: one object for each row.
    pub fn json(&self) -> String {
        let rows: Vec<Value> = (self.rows.iter())
            .map(|row| {
                json!({
                    "name": row.name,
                    "version": row.version,
                    "status": row.status,
                    "failed_requirement": row.failed.as_ref().map(|f| json!({
                        "source": f.source,
                        "type": f.description,
                        "target": f.target,
                        "constraint": f.pretty,
                    })),
                    "provider": row.provider,
                })
            })
            .collect();
        crate::php::json_pretty(&Value::Array(rows))
    }
}
