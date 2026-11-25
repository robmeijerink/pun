//! `pun composer check-platform-reqs`: Composer 2.11.2's `CheckPlatformReqsCommand`, its output and exit codes included.

use crate::Exit;
use anyhow::{Context, Result, bail};
use pun_packages::platform_reqs::{Source, check};
use pun_packages::resolver::platform::probe_cached;
use serde_json::{Map, Value};
use std::path::PathBuf;

#[derive(clap::Args)]
pub(crate) struct PlatformReqsArgs {
    /// Leave out the requirements of require-dev and of the dev packages.
    #[arg(long)]
    no_dev: bool,
    /// Check the packages of composer.lock, not those installed in the vendor dir.
    #[arg(long)]
    lock: bool,
    /// The output: text or json (another value is text, as in Composer).
    #[arg(short = 'f', long, value_name = "FORMAT", default_value = "text")]
    format: String,
    /// Run in this directory instead of the current one.
    #[arg(short = 'd', long, value_name = "DIR")]
    working_dir: Option<PathBuf>,
}

pub(crate) fn run(a: &PlatformReqsArgs) -> Result<Exit> {
    // Composer changes to `-d` and works from `getcwd()`.
    let base = match &a.working_dir {
        Some(d) => d.canonicalize().map_err(|_| {
            anyhow::anyhow!(
                "Invalid working directory specified, {} does not exist.",
                d.display()
            )
        })?,
        None => std::env::current_dir()?,
    };
    // `Factory::getComposerFile`: `COMPOSER`, else composer.json; the lock is named after it.
    let named = std::env::var("COMPOSER")
        .ok()
        .map(|f| f.trim().to_string())
        .filter(|f| !f.is_empty());
    let file = named.clone().unwrap_or_else(|| "composer.json".into());
    let path = base.join(&file);
    if !path.exists() {
        let missing = match &named {
            Some(file) => format!("Composer could not find the config file: {file}"),
            None => format!(
                "Composer could not find a composer.json file in {}",
                base.display()
            ),
        };
        bail!(
            "{missing}\nTo initialize a project, please create a composer.json file. See https://getcomposer.org/basic-usage"
        );
    }
    let text = std::fs::read(&path).with_context(|| format!("can't read {}", path.display()))?;
    // Composer adds jsonlint's description of the error (context/PLAN.md §3).
    let manifest: Map<String, Value> = match serde_json::from_slice(&text) {
        Ok(Value::Object(manifest)) => manifest,
        _ => bail!("\"./{file}\" does not contain valid JSON"),
    };
    let lock_file = match file.strip_suffix(".json") {
        Some(stem) => format!("{stem}.lock"),
        None => format!("{file}.lock"),
    };
    let vendor = pun_packages::install::dirs(&base.to_string_lossy(), &manifest).vendor;
    // The real platform: Composer checks it with `config.platform` left out.
    let probe = probe_cached(
        &crate::install::php_binary(),
        pun_config::cache_dir().as_deref(),
    )?;
    let report = check(
        &base.join(lock_file),
        &manifest,
        &PathBuf::from(vendor),
        &probe,
        a.no_dev,
        a.lock,
    )?;
    let non_dev = if a.no_dev { "non-dev " } else { "" };
    eprintln!(
        "{}",
        match report.source {
            Source::Lock => format!("Checking {non_dev}platform requirements using the lock file"),
            Source::Vendor =>
                format!("Checking {non_dev}platform requirements for packages in the vendor dir"),
            Source::NoVendor => {
                format!(
                    "No vendor dir present, checking {non_dev}platform requirements from the lock file"
                )
            }
        }
    );
    if a.format == "json" {
        println!("{}", report.json());
    } else {
        print!("{}", report.text());
    }
    Ok(match report.exit {
        0 => Exit::Success,
        1 => Exit::Failure,
        _ => Exit::RunError,
    })
}
