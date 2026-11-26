//! `pun composer <command>`: Composer's commands that have no top-level verb.

use crate::Exit;
use crate::install::{InstallArgs, apcu, project, scan};
use crate::render::{ColorChoice, Look, Role};
use anyhow::{Result, bail};
use pun_packages::install::{Options, dump_autoload};
use pun_packages::version::IgnorePlatform;
use std::path::PathBuf;

#[derive(clap::Args)]
pub(crate) struct ComposerArgs {
    #[command(subcommand)]
    cmd: ComposerCmd,
}

#[derive(clap::Subcommand)]
enum ComposerCmd {
    /// Write the autoloader again from vendor/composer/installed.json, as `composer dump-autoload` does.
    #[command(visible_alias = "dumpautoload")]
    DumpAutoload(DumpArgs),
    /// Read or write a setting of composer.json, or of the global config.json, as `composer config` does.
    Config(crate::composer_config::ConfigArgs),
    #[command(external_subcommand)]
    Other(Vec<String>),
}

#[derive(clap::Args)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "one bool per command-line flag"
)]
struct DumpArgs {
    /// Put the PSR-0 and PSR-4 classes in the class map too.
    #[arg(short = 'o', long)]
    optimize: bool,
    /// Load classes from the class map only (implies --optimize).
    #[arg(short = 'a', long)]
    classmap_authoritative: bool,
    /// Cache found and missing classes in APCu.
    #[arg(long)]
    apcu: bool,
    /// The APCu cache prefix (implies --apcu); without it each dump picks a random one.
    #[arg(long, value_name = "PREFIX")]
    apcu_prefix: Option<String>,
    /// Use the autoload-dev rules (default: as the last install did).
    #[arg(long, conflicts_with = "no_dev")]
    dev: bool,
    /// Leave out the autoload-dev rules and the dev packages (default: as the last install did).
    #[arg(long)]
    no_dev: bool,
    /// Skip the root package's scripts.
    #[arg(long)]
    no_scripts: bool,
    /// Skip the Composer plugins.
    #[arg(long)]
    no_plugins: bool,
    /// Leave this platform requirement out of `platform_check.php`; repeatable.
    #[arg(long, value_name = "REQ")]
    ignore_platform_req: Vec<String>,
    /// Write no `platform_check.php`.
    #[arg(long)]
    ignore_platform_reqs: bool,
    /// Run in this directory instead of the current one.
    #[arg(short = 'd', long, value_name = "DIR")]
    working_dir: Option<PathBuf>,
}

pub(crate) fn run(a: ComposerArgs) -> Result<Exit> {
    match a.cmd {
        ComposerCmd::DumpAutoload(d) => dump(&d),
        ComposerCmd::Config(c) => crate::composer_config::run_config(&c),
        ComposerCmd::Other(args) => bail!(
            "`pun composer {}` is not built yet; Pun does not run the Composer CLI",
            args.first().map_or("", String::as_str)
        ),
    }
}

fn dump(d: &DumpArgs) -> Result<Exit> {
    let project = project(d.working_dir.as_ref(), false)?;
    let options = Options {
        scan: scan(d.optimize, d.classmap_authoritative),
        apcu: apcu(d.apcu, d.apcu_prefix.as_ref()),
        no_plugins: d.no_plugins,
        ignore_platform: IgnorePlatform::new(d.ignore_platform_req.clone(), d.ignore_platform_reqs),
        cache: pun_config::cache_dir(),
        ..Options::default()
    };
    let dev = if d.dev {
        Some(true)
    } else if d.no_dev {
        Some(false)
    } else {
        None
    };
    let mut events = |event: &str, dev_mode: bool| {
        let a = InstallArgs {
            no_dev: !dev_mode,
            no_scripts: d.no_scripts,
            no_plugins: d.no_plugins,
            ..InstallArgs::default()
        };
        project.dispatch(&a, event)
    };
    let report = dump_autoload(
        &project.root,
        &project.manifest,
        project.lock.as_ref(),
        &options,
        dev,
        &mut events,
    )?;
    let look = Look::stderr(ColorChoice::Auto);
    let ok = look.paint(Role::Ok, look.glyph("✔", "+"));
    let warn = look.paint(Role::Warning, look.glyph("⚠", "!"));
    let noun = if report.classes == 1 {
        "class"
    } else {
        "classes"
    };
    eprintln!(" {ok} autoloader: {} {noun}", report.classes);
    for w in &report.warnings {
        eprintln!(" {warn} {w}");
    }
    if report.missing {
        eprintln!(
            " {warn} Not all dependencies are installed: run `pun install` to install the missing ones"
        );
        return Ok(Exit::Failure);
    }
    Ok(Exit::Success)
}
