//! `pun composer exec`: Composer 2.11.2's `ExecCommand`. It runs a binary of the bin dir, or of composer.json's own
//! `bin`, as Composer runs a script's command (the binary is the one command of its `__exec_command` event), or lists
//! them.

use crate::Exit;
use crate::composer::{Globals, finish, select};
use crate::render::{ColorChoice, Look};
use anyhow::{Result, bail};
use pun_packages::scripts::Runner;
use serde_json::{Map, Value};
use std::path::PathBuf;

#[derive(clap::Args)]
pub(crate) struct ExecArgs {
    /// List the binaries.
    #[arg(short = 'l', long)]
    list: bool,
    /// The binary to run, then its arguments. Composer's options count up to `--`: put it before arguments that
    /// start with `-`.
    #[arg(
        value_name = "BINARY [ARGS]",
        trailing_var_arg = true,
        allow_hyphen_values = true
    )]
    words: Vec<String>,
}

/// The command line after the binary as Symfony's `ArgvInput` reads it for `exec`: Composer's options anywhere up to
/// `--`, the other words in order.
#[derive(Default)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "one bool per command-line flag"
)]
struct Line {
    list: bool,
    quiet: bool,
    verbose: bool,
    no_interaction: bool,
    ansi: Option<bool>,
    working_dir: Option<PathBuf>,
    words: Vec<String>,
}

impl Line {
    fn read(words: &[String]) -> Result<Line> {
        let mut line = Line::default();
        let mut words = words.iter().map(String::as_str).peekable();
        let mut options = true;
        while let Some(word) = words.next() {
            if !options || word == "-" || !word.starts_with('-') {
                line.words.push(word.to_string());
            } else if word == "--" {
                options = false;
            } else if let Some(long) = word.strip_prefix("--") {
                let (name, value) = match long.split_once('=') {
                    Some((name, value)) => (name, Some(value)),
                    None => (long, None),
                };
                line.option(name, value, &mut words)?;
            } else {
                // A set of shortcuts: each a flag, but `d`, whose value is the rest of the word, else the next word.
                let set = &word[1..];
                for (at, c) in set.char_indices() {
                    let name = match c {
                        'l' => "list",
                        'q' => "quiet",
                        'v' => "verbose",
                        'n' => "no-interaction",
                        'd' => "working-dir",
                        'h' => "help",
                        'V' => "version",
                        _ => bail!("The \"-{c}\" option does not exist."),
                    };
                    if name == "working-dir" {
                        let rest = &set[at + 1..];
                        line.option(name, (!rest.is_empty()).then_some(rest), &mut words)?;
                        break;
                    }
                    line.option(name, None, &mut words)?;
                }
            }
        }
        Ok(line)
    }

    /// Symfony's `addLongOption` for Composer's options: a flag takes no value; `--working-dir` takes one, the next
    /// word when it doesn't start with `-`.
    fn option<'a>(
        &mut self,
        name: &str,
        value: Option<&'a str>,
        words: &mut std::iter::Peekable<impl Iterator<Item = &'a str>>,
    ) -> Result<()> {
        if name == "working-dir" {
            let value = match value.filter(|v| !v.is_empty()) {
                Some(v) => Some(v),
                None => words.next_if(|next| !next.starts_with('-') || next.is_empty()),
            };
            let Some(value) = value else {
                bail!("The \"--working-dir\" option requires a value.");
            };
            self.working_dir = Some(PathBuf::from(value));
            return Ok(());
        }
        let known = [
            "list",
            "quiet",
            "verbose",
            "no-interaction",
            "ansi",
            "no-ansi",
            "no-plugins",
            "no-scripts",
            "no-cache",
            "help",
            "version",
            "profile",
        ];
        if !known.contains(&name) {
            bail!("The \"--{name}\" option does not exist.");
        }
        if value.is_some() {
            bail!("The \"--{name}\" option does not accept a value.");
        }
        match name {
            "list" => self.list = true,
            "quiet" => self.quiet = true,
            "verbose" => self.verbose = true,
            "no-interaction" => self.no_interaction = true,
            "ansi" => self.ansi = Some(true),
            "no-ansi" => self.ansi = Some(self.ansi.unwrap_or(false)),
            "help" | "version" | "profile" => {
                bail!(
                    "`pun composer exec` doesn't take --{name}; put it before `exec`, or after `--` for the binary"
                )
            }
            // `--no-plugins`, `--no-scripts` and `--no-cache` change nothing here.
            _ => {}
        }
        Ok(())
    }
}

pub(crate) fn run(a: &ExecArgs, globals: &Globals) -> Result<Exit> {
    let line = Line::read(&a.words)?;
    // Composer changes to `-d` and runs the binary there.
    let here = match line.working_dir.as_ref().or(globals.working_dir.as_ref()) {
        Some(d) => d.canonicalize().map_err(|_| {
            anyhow::anyhow!(
                "Invalid working directory specified, {} does not exist.",
                d.display()
            )
        })?,
        None => std::env::current_dir()?,
    };
    if !here.join("composer.json").exists() {
        bail!(
            "Composer could not find a composer.json file in {}\nTo initialize a project, please create a composer.json file. See https://getcomposer.org/basic-usage",
            here.display()
        );
    }
    let project = crate::install::project(Some(&here), false)?;
    let dirs = pun_packages::install::dirs(&project.root.to_string_lossy(), &project.manifest);
    // `--ansi` wins over `--no-ansi`; else each of stdout and stderr is coloured on a terminal.
    let (out_color, err_color) = match line.ansi {
        Some(ansi) => (ansi, ansi),
        None => (
            Look::stdout(ColorChoice::Auto).colored(),
            Look::stderr(ColorChoice::Auto).colored(),
        ),
    };
    let mut binary = line.words.first().cloned();
    // `interact`: in a terminal, without a binary or `--list`, Composer asks which binary to run.
    if binary.is_none() && !a.list && !line.list && !globals.no_interaction && !line.no_interaction
    {
        let names: Vec<String> = binaries(&dirs.bin, &project.manifest, false)
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        binary = select("Binary to run", &names)?.map(|i| names[i].clone());
    }
    let Some(binary) = binary.filter(|_| !a.list && !line.list) else {
        let found = binaries(&dirs.bin, &project.manifest, true);
        if found.is_empty() {
            bail!(
                "No binaries found in composer.json or in bin-dir ({})",
                dirs.bin
            );
        }
        if !line.quiet {
            println!("{}", paint("33", "Available binaries:", out_color));
            for (name, local) in found {
                let shown = if local {
                    format!("- {name} (local)")
                } else {
                    format!("- {name}")
                };
                println!("{}", paint("32", &shown, out_color));
            }
        }
        return Ok(Exit::Success);
    };
    let mut runner = Runner::new(
        &project.root,
        &dirs.vendor,
        &dirs.bin,
        &project.manifest,
        true,
        &|k| std::env::var(k).ok(),
    );
    runner.quiet = line.quiet;
    runner.verbose = line.verbose;
    runner.dev_mode_env = false;
    finish(
        runner.exec(&binary, line.words.get(1..).unwrap_or_default()),
        err_color,
    )
}

/// `ExecCommand::getBinaries`: the bin dir's files (PHP's `glob`, sorted, without dot files), then composer.json's
/// `bin` (marked local), each by its file name, without a `.bat` right after the binary it wraps. `for_display`
/// (`--list`) compares the local ones with ` (local)` already appended, so `bin/x.bat (local)` never matches
/// `bin/x (local).bat` and both stay.
fn binaries(
    bin_dir: &str,
    manifest: &Map<String, Value>,
    for_display: bool,
) -> Vec<(String, bool)> {
    let mut names: Vec<String> = std::fs::read_dir(bin_dir)
        .into_iter()
        .flatten()
        .filter_map(|entry| Some(entry.ok()?.file_name().to_string_lossy().into_owned()))
        .filter(|name| !name.starts_with('.'))
        .collect();
    names.sort();
    let found = (names
        .into_iter()
        .map(|name| (format!("{bin_dir}/{name}"), false)))
    .chain(
        match manifest.get("bin") {
            Some(Value::String(bin)) => vec![bin.clone()],
            Some(Value::Array(bins)) => (bins.iter().filter_map(Value::as_str))
                .map(String::from)
                .collect(),
            _ => vec![],
        }
        .into_iter()
        .map(|bin| (bin, true)),
    );
    let mut previous: Option<String> = None;
    let mut out = vec![];
    for (bin, local) in found {
        if !(for_display && local) && previous.as_ref().is_some_and(|p| bin == format!("{p}.bat")) {
            continue;
        }
        // PHP's `basename`.
        let name = bin
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or_default()
            .to_string();
        previous = Some(bin);
        out.push((name, local));
    }
    out
}

/// `text` in a Symfony console colour (`33` for comment, `32` for info), or as it is without `color`.
fn paint(code: &str, text: &str, color: bool) -> String {
    if color {
        format!("\x1b[{code}m{text}\x1b[39m")
    } else {
        text.to_string()
    }
}
