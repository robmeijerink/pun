use crate::Exit;
use crate::console::{self, format as fmt};
use crate::fix_report::{self, Change};
use crate::render::{
    CheckItem, ColorChoice, FileLine, FileStatus, FormatSummary, Look, Renderer, Role, RunError,
    Step, StepLine, Summary,
};
use crate::{github, pint_error};
use anyhow::{Result, bail};
use pun_lint::problem::{Level, LineIndex, Problem, Span};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Status {
    NoChanges,
    Skipped,
    Fixed,
    Invalid,
    Lint,
    Exception,
    Link,
}

pub(crate) struct FileResult {
    pub(crate) path: PathBuf,
    pub(crate) rel: String,
    pub(crate) status: Status,
    pub(crate) applied: Vec<&'static str>,
    pub(crate) diff: String,
    pub(crate) message: String,
    pub(crate) line: usize,
    pub(crate) problems: Vec<Problem>,
    pub(crate) source: String,
    pub(crate) warned: Vec<&'static str>,
}

impl FileResult {
    pub(crate) fn new(path: PathBuf, rel: String) -> FileResult {
        FileResult {
            path,
            rel,
            status: Status::NoChanges,
            applied: vec![],
            diff: String::new(),
            message: String::new(),
            line: 0,
            problems: vec![],
            source: String::new(),
            warned: vec![],
        }
    }

    pub(crate) fn is_error(&self) -> bool {
        matches!(
            self.status,
            Status::Invalid | Status::Exception | Status::Lint
        )
    }
}

#[expect(
    clippy::struct_excessive_bools,
    reason = "independent output switches, each set from its own flag"
)]
pub(crate) struct Ctx<'a> {
    pub(crate) total: usize,
    pub(crate) dry_run: bool,
    pub(crate) remaining: &'a [FileResult],
    pub(crate) verbose: bool,
    pub(crate) color: ColorChoice,
    pub(crate) look: Look,
    pub(crate) root: &'a Path,
    pub(crate) elapsed: Duration,
    pub(crate) github: bool,
    pub(crate) concise: bool,
    /// Baseline entries (counts) no problem matched any more: the lint baseline's, the analyse baseline's.
    pub(crate) unmatched_baseline: (usize, usize),
    /// `pun check`: the analysis's errors that belong to no file (`[[analyse.ignore]]` entries nothing matched).
    pub(crate) general: &'a [String],
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Counts {
    pub(crate) errors: usize,
    pub(crate) warnings: usize,
    pub(crate) fixable: usize,
    pub(crate) need_formatting: usize,
}

fn rule_id(fixer: &str) -> &str {
    pun_config::rule_of(fixer).unwrap_or(fixer)
}

fn lint_fixers(f: &FileResult) -> (usize, usize) {
    let lint = || (f.applied.iter()).filter(|n| !rule_id(n).starts_with("format/"));
    let warned = lint().filter(|n| f.warned.contains(n)).count();
    (lint().count() - warned, warned)
}

pub(crate) fn counts(ctx: &Ctx, results: &[FileResult]) -> Counts {
    let mut c = Counts {
        errors: results.iter().filter(|f| f.is_error()).count() + ctx.general.len(),
        ..Counts::default()
    };
    for f in results {
        if ctx.dry_run && f.status == Status::Fixed {
            let (errors, warnings) = lint_fixers(f);
            c.errors += errors;
            c.warnings += warnings;
            c.fixable += errors + warnings;
            c.need_formatting += usize::from(errors + warnings < f.applied.len());
        }
        for p in &f.problems {
            match p.level {
                Level::Error => c.errors += 1,
                Level::Warning => c.warnings += 1,
            }
            c.fixable += usize::from(ctx.dry_run && p.fix.is_some());
        }
    }
    for f in ctx.remaining {
        let (errors, warnings) = lint_fixers(f);
        c.errors += errors;
        c.warnings += warnings;
    }
    c
}

#[derive(Debug)]
pub(crate) struct Abort {
    pub(crate) message: String,
    pub(crate) pint: bool,
    pub(crate) before: String,
    pub(crate) error: RunError,
    pub(crate) thrown: pint_error::Thrown,
}

impl Abort {
    pub(crate) fn pint(message: impl Into<String>, hint: Option<String>) -> Abort {
        let message = message.into();
        let (first, rest) = message.split_once('\n').unwrap_or((&message, ""));
        let rest: Vec<&str> = rest.lines().filter(|l| !l.trim().is_empty()).collect();
        Abort {
            error: RunError {
                path: None,
                line: None,
                message: first.to_string(),
                hint: hint.or_else(|| (!rest.is_empty()).then(|| rest.join("\n"))),
            },
            message,
            pint: true,
            before: String::new(),
            thrown: pint_error::Thrown::Console,
        }
    }

    pub(crate) fn pun(message: impl Into<String>, error: RunError) -> Abort {
        Abort {
            message: message.into(),
            pint: false,
            before: String::new(),
            error,
            thrown: pint_error::Thrown::Console,
        }
    }

    pub(crate) fn plain(message: String, hint: Option<String>) -> Abort {
        let error = RunError {
            path: None,
            line: None,
            message: message.clone(),
            hint,
        };
        Abort::pun(message, error)
    }
}

impl std::fmt::Display for Abort {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Abort {}

#[expect(
    clippy::struct_excessive_bools,
    reason = "independent output switches, each set from its own flag"
)]
pub(crate) struct Failure {
    pub(crate) pint: bool,
    pub(crate) machine: bool,
    pub(crate) github_format: bool,
    pub(crate) annotate: bool,
    pub(crate) color: ColorChoice,
}

pub(crate) fn fail(e: anyhow::Error, f: &Failure) -> Exit {
    let abort = match e.downcast::<Abort>() {
        Ok(a) => a,
        Err(e) if f.pint => {
            let mut abort = Abort::pint(e.to_string(), None);
            abort.thrown = pint_error::Thrown::of(&e);
            abort
        }
        Err(e) => Abort::plain(format!("{e:#}"), None),
    };
    if crate::agent::enabled() {
        print!("{}", crate::agent::error(&abort.to_string()));
    } else if f.github_format {
        print!("{}", github::run_error(&abort.message));
    } else if f.machine && f.pint && abort.pint {
        print!(
            "{}{}",
            abort.before,
            pint_error::render(&abort.message, abort.thrown, f.color)
        );
    } else {
        let root = std::env::current_dir().unwrap_or_default();
        let r = Renderer::new(Look::stderr(f.color), root);
        eprint!("{}", r.run_error(&abort.error));
        if f.annotate {
            print!("{}", github::run_error(&abort.message));
        }
    }
    Exit::RunError
}

fn error_line(message: &str) -> usize {
    message
        .rsplit(" on line ")
        .next()
        .and_then(|n| n.trim_end_matches('.').parse().ok())
        .unwrap_or(0)
}

/// Prints the run's report. `quiet` (`-q`) prints nothing, as PHP-CS-Fixer's `fix -q` (`pun lint`), and as Pint under
/// a coding agent or with `--format`; with `pint_lines` (`pun format` and `pun check`, which match Pint) and neither,
/// Pint's one stderr line per file.
pub(crate) fn emit(
    ctx: &Ctx,
    results: &[FileResult],
    format: Option<&str>,
    (quiet, pint_lines): (bool, bool),
    to_file: Option<&Path>,
    file_format: Option<&str>,
) -> Result<()> {
    let out_dec = console::decorated(1, ctx.color);
    let unsupported = |e: anyhow::Error| {
        let hint = "use json, agent, checkstyle, junit, gitlab or github";
        anyhow::Error::new(Abort::pint(e.to_string(), Some(hint.into())))
    };
    let err_dec = console::decorated(2, ctx.color);
    let mut out = String::new();
    let parsed = (format.map(str::parse::<ReportFormat>).transpose()).map_err(unsupported)?;
    if quiet {
        let lines = if pint_lines && format.is_none() && !crate::agent::enabled() {
            issues(results)
        } else {
            vec![]
        };
        for i in lines {
            console::eprintln_raw(&fmt(
                &format!(
                    "  {} {}: {}",
                    symbol(i.status, ctx.dry_run, out_dec),
                    i.rel,
                    description(i)
                ),
                err_dec,
            ))?;
        }
    } else if crate::agent::enabled() && format.is_none() {
        out = agent(ctx, results);
    } else if parsed == Some(ReportFormat::Github) {
        print!("{}", annotations(ctx, results));
    } else if let Some(f) = parsed {
        out = reporter(ctx, results, f, out_dec)?;
    } else {
        console::print_raw(&human(ctx, results))?;
        if ctx.github {
            print!("{}", annotations(ctx, results));
        }
    }
    console::print_raw(&fmt(&out, out_dec))?;
    std::io::Write::flush(&mut std::io::stdout())?;
    if let (Some(path), Some(f)) = (to_file, file_format.or(format))
        && let Err(e) = crate::run::write_generated(
            path,
            &console::raw(&reporter(
                ctx,
                results,
                f.parse().map_err(unsupported)?,
                out_dec,
            )?),
        )
    {
        let e = e.to_string();
        let e = e.split(" (os error").next().unwrap_or(&e);
        let msg = format!(
            "file_put_contents({}): Failed to open stream: {e}",
            path.display(),
        );
        let mut abort = Abort::pint(msg, None);
        abort.thrown = pint_error::Thrown::ReportNotWritable;
        abort.error = RunError {
            path: None,
            line: None,
            message: format!("can't write the report to {}: {e}", path.display()),
            hint: None,
        };
        return Err(abort.into());
    }
    if format.is_some() {
        for r in errors(results) {
            console::eprintln_raw(&fmt(
                &format!("  <error>ERROR</error> {}: {}", r.path.display(), r.message),
                err_dec,
            ))?;
        }
    }
    Ok(())
}

fn annotations(ctx: &Ctx, results: &[FileResult]) -> String {
    let file = |f: &FileResult| {
        let ids: Vec<String> = (f.applied.iter())
            .map(|n| pun_config::rule_of(n).unwrap_or(n).to_string())
            .collect();
        let lint: Vec<&str> = (ids.iter().map(String::as_str))
            .filter(|id| !id.starts_with("format/"))
            .collect();
        let message = if lint.is_empty() {
            "File needs formatting".to_string()
        } else {
            format!("{} can fix this file", lint.join(", "))
        };
        github::file(&f.rel, f.line, &ids, &message)
    };
    let mut lines: Vec<(&str, String)> = vec![];
    for f in results {
        let line = match f.status {
            Status::Fixed if ctx.dry_run => file(f),
            Status::Invalid => {
                let (problem, _, index) = syntax_error(f);
                github::problem(&problem, &index)
            }
            Status::Exception | Status::Lint => github::file(
                &f.rel,
                error_line(&f.message).max(1),
                &["fixer-error".into()],
                &console::lossy(&f.message),
            ),
            _ => continue,
        };
        lines.push((&f.rel, line));
    }
    lines.extend(ctx.remaining.iter().map(|f| (f.rel.as_str(), file(f))));
    for f in results.iter().filter(|f| !f.problems.is_empty()) {
        let index = LineIndex::new(&f.source);
        lines.extend((f.problems.iter()).map(|p| (f.rel.as_str(), github::problem(p, &index))));
    }
    lines.sort_by(|a, b| a.0.cmp(b.0));
    lines.into_iter().map(|l| l.1).collect()
}

#[expect(
    clippy::too_many_lines,
    reason = "long; to be split (code audit: functions over 100 lines)"
)]
fn human(ctx: &Ctx, results: &[FileResult]) -> String {
    let cmd = crate::agent::cmd();
    if cmd == "check" {
        return check_human(ctx, results);
    }
    let r = Renderer::new(ctx.look, ctx.root);
    let ids = |f: &FileResult| -> Vec<String> {
        (f.applied.iter())
            .map(|n| pun_config::rule_of(n).unwrap_or(n).to_string())
            .collect()
    };
    let changed = if ctx.dry_run {
        FileStatus::NeedsFix
    } else {
        FileStatus::Fixed
    };
    let mut lines: Vec<(FileLine, &str)> = vec![];
    for f in results {
        let status = match f.status {
            Status::Fixed => changed,
            Status::NoChanges | Status::Skipped if ctx.verbose => FileStatus::Unchanged,
            _ => continue,
        };
        lines.push((
            FileLine {
                path: f.rel.clone().into(),
                status,
                rules: ids(f),
            },
            &f.diff,
        ));
    }
    for f in ctx.remaining {
        let line = FileLine {
            path: f.rel.clone().into(),
            status: FileStatus::NeedsFix,
            rules: ids(f),
        };
        lines.push((line, &f.diff));
    }
    lines.sort_by(|a, b| a.0.path.cmp(&b.0.path));
    let (lines, diffs): (Vec<FileLine>, Vec<&str>) = lines.into_iter().unzip();
    let mut out = String::new();
    for (line, diff) in r.file_lines(&lines).into_iter().zip(diffs) {
        out.push_str(&line);
        if ctx.verbose && !diff.is_empty() {
            out.push_str(&r.diff(diff));
        }
    }
    let mut failed: Vec<&FileResult> = results.iter().filter(|f| f.is_error()).collect();
    failed.sort_by(|a, b| a.rel.cmp(&b.rel));
    let syntax: Vec<(Problem, String, LineIndex)> = (failed.iter())
        .filter(|f| f.status == Status::Invalid)
        .map(|f| syntax_error(f))
        .collect();
    let mut blocks: Vec<String> = vec![];
    let mut with_problems: Vec<&FileResult> = (results.iter())
        .filter(|f| !f.problems.is_empty())
        .collect();
    with_problems.sort_by(|a, b| a.rel.cmp(&b.rel));
    let indexes: Vec<LineIndex> = with_problems
        .iter()
        .map(|f| LineIndex::new(&f.source))
        .collect();
    let mut r = r;
    r.concise = ctx.concise;
    let problems: Vec<(&Problem, &str, &LineIndex)> = (with_problems.iter().zip(&indexes))
        .flat_map(|(f, i)| f.problems.iter().map(move |p| (p, f.source.as_str(), i)))
        .chain(syntax.iter().map(|(p, s, i)| (p, s.as_str(), i)))
        .collect();
    if !problems.is_empty() {
        blocks.push(r.problems(problems));
    }
    blocks.extend(
        (failed.iter())
            .filter(|f| f.status != Status::Invalid)
            .map(|f| fixer_error(&r, f)),
    );
    if !out.is_empty() && !blocks.is_empty() {
        out.push('\n');
    }
    out.push_str(&blocks.join("\n"));
    let formatted = results.iter().filter(|f| f.status == Status::Fixed).count();
    if cmd == "format" {
        let unchanged = (results.iter())
            .filter(|f| matches!(f.status, Status::NoChanges | Status::Skipped))
            .count();
        out.push_str(&r.format_summary(&FormatSummary {
            formatted,
            unchanged,
            errors: failed.len(),
            files: ctx.total,
            test: ctx.dry_run,
            elapsed: ctx.elapsed,
        }));
        return out;
    }
    out.push_str(&unmatched_note(&r, ctx));
    let c = counts(ctx, results);
    let fix_command = format!("pun {cmd} --fix");
    out.push_str(&r.summary(&Summary {
        errors: c.errors,
        warnings: c.warnings,
        fixable: c.fixable,
        fix_command: &fix_command,
        need_formatting: c.need_formatting,
        files: ctx.total,
        elapsed: ctx.elapsed,
    }));
    out
}

#[expect(
    clippy::too_many_lines,
    reason = "long; to be split (code audit: functions over 100 lines)"
)]
fn check_human(ctx: &Ctx, results: &[FileResult]) -> String {
    #[derive(Default)]
    struct Group {
        items: Vec<CheckItem>,
        errors: usize,
        warnings: usize,
        fixable: usize,
        files: Vec<String>,
    }
    impl Group {
        fn add(&mut self, rel: &str, item: CheckItem, warning: bool, fixable: bool) {
            if warning {
                self.warnings += 1;
            } else {
                self.errors += 1;
            }
            self.fixable += usize::from(fixable);
            if self.files.last().is_none_or(|f| f != rel) {
                self.files.push(rel.to_string());
            }
            self.items.push(item);
        }
    }
    fn step(step: Step, g: &Group, text: Option<String>, failed: bool) -> StepLine<'_> {
        StepLine {
            step,
            errors: g.errors,
            warnings: g.warnings,
            fixable: g.fixable,
            files: g.files.len(),
            text,
            failed,
            items: &g.items,
        }
    }
    let mut r = Renderer::new(ctx.look, ctx.root);
    r.concise = ctx.concise;
    let mut sorted: Vec<&FileResult> = results.iter().collect();
    sorted.sort_by(|a, b| a.rel.cmp(&b.rel));
    let (mut format, mut lint, mut analyse) =
        (Group::default(), Group::default(), Group::default());
    let mut formatted = 0;
    let summary_of = |id: &str| {
        pun_config::rule(id)
            .and_then(|m| m.summary)
            .map(String::from)
    };
    let lint_fixes = |g: &mut Group, f: &FileResult, fixable: bool| {
        for n in f
            .applied
            .iter()
            .filter(|n| !rule_id(n).starts_with("format/"))
        {
            let id = rule_id(n);
            let item = CheckItem {
                path: f.rel.clone().into(),
                at: format!(":{}", f.line),
                rules: vec![id.to_string()],
                message: summary_of(id),
                below: String::new(),
            };
            g.add(&f.rel, item, f.warned.contains(n), fixable);
        }
    };
    for f in &sorted {
        if f.status == Status::Invalid {
            let (p, _, index) = syntax_error(f);
            let at = index.line_col(p.span.start);
            let item = CheckItem {
                path: p.path,
                at: format!(":{}:{}", at.line, at.column),
                rules: vec![p.rule],
                message: Some(p.message),
                below: String::new(),
            };
            format.add(&f.rel, item, false, false);
        }
        if f.status == Status::Fixed {
            let rules: Vec<String> = (f.applied.iter())
                .map(|n| rule_id(n))
                .filter(|id| id.starts_with("format/"))
                .map(String::from)
                .collect();
            if ctx.dry_run && !rules.is_empty() {
                let below = if ctx.verbose && !f.diff.is_empty() {
                    r.diff(&f.diff)
                } else {
                    String::new()
                };
                format.items.push(CheckItem {
                    path: f.rel.clone().into(),
                    rules,
                    below,
                    ..CheckItem::default()
                });
            } else if !rules.is_empty() {
                formatted += 1;
            }
            if ctx.dry_run {
                lint_fixes(&mut lint, f, true);
            }
        }
        if !f.problems.is_empty() {
            let index = LineIndex::new(&f.source);
            for p in &f.problems {
                let at = index.line_col(p.span.start);
                let item = CheckItem {
                    path: p.path.clone(),
                    at: format!(":{}:{}", at.line, at.column),
                    rules: vec![p.rule.clone()],
                    message: Some(p.message.clone()),
                    below: String::new(),
                };
                let group = if p.rule.starts_with("lint/") {
                    &mut lint
                } else {
                    &mut analyse
                };
                let fixable = ctx.dry_run && p.fix.is_some();
                group.add(&f.rel, item, p.level == Level::Warning, fixable);
            }
        }
    }
    let mut remaining: Vec<&FileResult> = ctx.remaining.iter().collect();
    remaining.sort_by(|a, b| a.rel.cmp(&b.rel));
    for f in remaining {
        lint_fixes(&mut lint, f, false);
    }
    let need = format.items.len() - format.errors;
    let format_text = if need > 0 {
        let verb = if need == 1 { "needs" } else { "need" };
        let mut text = format!(
            "{} {verb} formatting",
            crate::render::plural(need, "file", "files")
        );
        if format.errors > 0 {
            let _ = write!(
                text,
                ", {}",
                crate::render::plural(format.errors, "syntax error", "syntax errors")
            );
        }
        Some(text)
    } else if format.errors > 0 {
        None
    } else if formatted > 0 {
        Some(format!(
            "Formatted {}",
            crate::render::plural(formatted, "file", "files")
        ))
    } else {
        Some("All files formatted".into())
    };
    analyse.errors += ctx.general.len();
    let analyse_text = (analyse.files.is_empty() && analyse.errors > 0)
        .then(|| crate::render::plural(analyse.errors, "error", "errors"));
    let mut out = r.check_step(&step(Step::Format, &format, format_text, need > 0));
    out.push_str(&r.check_step(&step(Step::Lint, &lint, None, false)));
    out.push_str(&r.check_step(&step(Step::Analyse, &analyse, analyse_text, false)));
    let glyph = r.look.paint(Role::Error, r.look.glyph("✖", "x"));
    for g in ctx.general {
        let _ = writeln!(out, "   {glyph} {g}");
    }
    for f in sorted
        .iter()
        .filter(|f| f.is_error() && f.status != Status::Invalid)
    {
        out.push_str(&fixer_error(&r, f));
    }
    out.push_str(&unmatched_note(&r, ctx));
    let c = counts(ctx, results);
    out.push_str(&r.check_summary(&Summary {
        errors: c.errors,
        warnings: c.warnings,
        fixable: c.fixable,
        fix_command: "pun check --fix",
        need_formatting: c.need_formatting,
        files: ctx.total,
        elapsed: ctx.elapsed,
    }));
    out
}

/// The note for baseline entries no problem matched any more, if any.
fn unmatched_note(r: &Renderer, ctx: &Ctx) -> String {
    match ctx.unmatched_baseline {
        (0, 0) => String::new(),
        (lint, analyse) => crate::analyse::baseline_note(r, lint, analyse),
    }
}

fn syntax_error(f: &FileResult) -> (Problem, String, LineIndex) {
    let source = String::from_utf8_lossy(&std::fs::read(&f.path).unwrap_or_default()).into_owned();
    let index = LineIndex::new(&source);
    let last = index.line_col(source.len()).line;
    let range = index
        .line_range(error_line(&f.message).clamp(1, last))
        .expect("a line of the file");
    let line = &source[range.clone()];
    let at = range.start + line.len() - line.trim_start().len();
    let message = console::lossy(&f.message);
    let message = message.strip_prefix("Parse error: ").unwrap_or(&message);
    let message = message.rsplit_once(" on line ").map_or(message, |m| m.0);
    let problem = Problem {
        rule: "syntax-error".into(),
        path: f.rel.clone().into(),
        span: Span::new(at, at),
        message: message.to_string(),
        help: None,
        level: Level::Error,
        fix: None,
    };
    (problem, source, index)
}

fn fixer_error(r: &Renderer, f: &FileResult) -> String {
    r.run_error(&RunError {
        path: Some(f.rel.clone().into()),
        line: None,
        message: console::lossy(&f.message),
        hint: Some(
            "a rule failed on this file; please report it at https://github.com/shipfastlabs/pun/issues"
                .into(),
        ),
    })
}

#[expect(
    clippy::too_many_lines,
    reason = "long; to be split (code audit: functions over 100 lines)"
)]
fn agent(ctx: &Ctx, results: &[FileResult]) -> String {
    use crate::agent::count;
    let mut sorted: Vec<(&FileResult, bool)> = (results.iter().map(|r| (r, false)))
        .chain(ctx.remaining.iter().map(|r| (r, true)))
        .collect();
    sorted.sort_by(|a, b| (&a.0.rel, a.1).cmp(&(&b.0.rel, b.1)));
    let mut items = vec![];
    let (mut fixed, mut errors, mut left) = (0, 0, 0);
    let mut warnings = 0;
    for (r, remaining) in sorted {
        if !remaining && !r.problems.is_empty() {
            let index = LineIndex::new(&r.source);
            for p in &r.problems {
                let at = index.line_col(p.span.start);
                let fix = match p.fix.as_ref().map(|f| f.safety) {
                    Some(pun_lint::problem::Safety::Safe) => " [fix:safe]",
                    Some(pun_lint::problem::Safety::Unsafe) => " [fix:unsafe]",
                    None => "",
                };
                match p.level {
                    Level::Error => errors += 1,
                    Level::Warning => warnings += 1,
                }
                let level = if p.level == Level::Warning {
                    " [warn]"
                } else {
                    ""
                };
                items.push(format!(
                    "{}:{}:{} {}{fix}{level} {}",
                    r.rel, at.line, at.column, p.rule, p.message
                ));
            }
        }
        if r.is_error() {
            errors += 1;
            let rule = if r.status == Status::Invalid {
                "syntax-error"
            } else {
                "fixer-error"
            };
            let at = match error_line(&r.message) {
                0 => String::new(),
                n => format!(":{n}"),
            };
            items.push(format!(
                "{}{at} {rule} {}",
                r.rel,
                console::lossy(&r.message)
            ));
        } else if r.status == Status::Fixed {
            if remaining {
                left += 1;
            } else {
                fixed += 1;
            }
            let tag = if remaining { " [remaining]" } else { "" };
            let (format, lint): (Vec<&str>, Vec<&str>) = (r.applied.iter())
                .map(|f| pun_config::rule_of(f).unwrap_or(f))
                .partition(|id| id.starts_with("format/"));
            let mut item = String::new();
            if !format.is_empty() {
                let _ = writeln!(item, "{} {}", r.rel, format.join(", "));
            }
            for id in lint {
                let (fix, summary) = lint_rule(id);
                let _ = writeln!(item, "{} {id} [fix:{fix}]{tag}{summary}", r.rel);
            }
            item.pop();
            if ctx.verbose {
                item.push('\n');
                item.push_str(r.diff.trim_end_matches('\n'));
            }
            items.push(item);
        }
    }
    for g in ctx.general {
        errors += 1;
        items.push(format!("unmatched-ignore {g}"));
    }
    let cmd = crate::agent::cmd();
    let mut detail = vec![];
    if errors > 0 {
        detail.push(count(errors, "error"));
    }
    if fixed > 0 {
        let files = count(fixed, "file");
        detail.push(match (cmd, ctx.dry_run) {
            ("format", true) => format!(
                "{files} need{} formatting",
                if fixed == 1 { "s" } else { "" }
            ),
            ("format", false) => format!("{files} formatted"),
            (_, true) => format!("{files} with fixes"),
            (_, false) => format!("{files} fixed"),
        });
    }
    if left > 0 {
        detail.push(format!("{} with remaining fixes", count(left, "file")));
    }
    if warnings > 0 {
        detail.push(count(warnings, "warning"));
    }
    let (lint, analyse) = ctx.unmatched_baseline;
    if lint + analyse > 0 {
        let n = lint + analyse;
        let entries = if n == 1 { "entry" } else { "entries" };
        detail.push(format!("{n} baseline {entries} no longer matched"));
    }
    let result = if errors > 0 || left > 0 || (fixed > 0 && ctx.dry_run) {
        "fail"
    } else if fixed > 0 {
        "fixed"
    } else {
        "pass"
    };
    let checked = format!("{} checked", ctx.total);
    crate::agent::render(result, &detail, &checked, &items, ctx.verbose)
}

fn lint_rule(id: &str) -> (&'static str, String) {
    let m = pun_config::rule(id);
    let fix = if m
        .as_ref()
        .is_some_and(|m| m.fix == pun_config::FixSafety::Safe)
    {
        "safe"
    } else {
        "unsafe"
    };
    let summary = m.and_then(|m| m.summary);
    (fix, summary.map_or(String::new(), |s| format!(" {s}")))
}

fn symbol(s: Status, testing: bool, decorated: bool) -> String {
    let (sym, style) = match s {
        Status::Fixed if testing => ("⨯", "options=bold;fg=red"),
        Status::Fixed => ("✓", "options=bold;fg=green"),
        _ => ("!", "options=bold;fg=red"),
    };
    if decorated {
        format!("<{style}>{sym}</>")
    } else {
        sym.to_string()
    }
}

fn description(r: &FileResult) -> String {
    if r.is_error() {
        r.message.clone()
    } else {
        r.applied.join(", ")
    }
}

fn errors(results: &[FileResult]) -> impl Iterator<Item = &FileResult> {
    let of = move |s| results.iter().filter(move |r| r.status == s);
    of(Status::Invalid)
        .chain(of(Status::Exception))
        .chain(of(Status::Lint))
}

fn issues(results: &[FileResult]) -> Vec<&FileResult> {
    let of = |s| results.iter().filter(move |r: &&FileResult| r.status == s);
    let mut v: Vec<&FileResult> = of(Status::Fixed).collect();
    let mut taken = 0;
    for s in [Status::Invalid, Status::Exception, Status::Lint] {
        let n = of(s).count();
        v.extend(of(s).skip(taken));
        taken = taken.max(n);
    }
    let key = |r: &FileResult| {
        if r.is_error() {
            r.path.to_string_lossy().into_owned()
        } else {
            r.rel.clone()
        }
    };
    v.sort_by(|a, b| key(a).as_bytes().cmp(key(b).as_bytes()));
    v
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum ReportFormat {
    Json,
    Agent,
    Checkstyle,
    Junit,
    Gitlab,
    Github,
}

impl std::str::FromStr for ReportFormat {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        Ok(match s {
            "json" => Self::Json,
            "agent" => Self::Agent,
            "checkstyle" => Self::Checkstyle,
            "junit" => Self::Junit,
            "gitlab" => Self::Gitlab,
            "github" => Self::Github,
            _ => bail!("Format [{s}] is not supported."),
        })
    }
}

fn reporter(
    ctx: &Ctx,
    results: &[FileResult],
    format: ReportFormat,
    decorated: bool,
) -> Result<String> {
    let fixers: Vec<Vec<&str>> = (results.iter())
        .map(|r| {
            let mut all: Vec<&str> = r.applied.clone();
            for p in &r.problems {
                if !all.contains(&p.rule.as_str()) {
                    all.push(&p.rule);
                }
            }
            all
        })
        .collect();
    let changes: Vec<Change> = (results.iter().zip(&fixers))
        .filter(|(r, _)| r.status == Status::Fixed || !r.problems.is_empty())
        .map(|(r, fixers)| Change {
            file: &r.rel,
            fixers,
            diff: if ctx.verbose { &r.diff } else { "" },
        })
        .collect();
    Ok(match format {
        ReportFormat::Json if changes.iter().any(|c| console::carries_bytes(c.diff)) => {
            let mut abort = Abort::pint(
                "Malformed UTF-8 characters, possibly incorrectly encoded",
                None,
            );
            abort.thrown = pint_error::Thrown::JsonMalformed;
            return Err(abort.into());
        }
        ReportFormat::Json => fix_report::json(&changes, ctx.verbose, decorated),
        ReportFormat::Agent => {
            let messages: Vec<String> = errors(results)
                .map(|r| console::lossy(&r.message))
                .collect();
            let errors: Vec<(&str, &str)> = (errors(results).map(|r| r.rel.as_str()))
                .zip(messages.iter().map(String::as_str))
                .collect();
            fix_report::agent(&changes, &errors, ctx.dry_run)
        }
        ReportFormat::Checkstyle => fix_report::checkstyle(&changes, decorated),
        ReportFormat::Junit => fix_report::junit(&changes, ctx.verbose, decorated),
        ReportFormat::Gitlab => fix_report::gitlab(&changes, decorated),
        ReportFormat::Github => annotations(ctx, results),
    })
}

pub(crate) fn diff_bytes(old: &[u8], new: &[u8], path: &Path) -> String {
    let mut p = path.to_string_lossy().into_owned();
    if p.contains(char::is_whitespace) {
        p = format!("\"{p}\"");
    }
    console::carry(&crate::php_diff::unified(old, new, &p))
}
