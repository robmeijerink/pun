use crate::analyse::Finding;
use crate::render::RunError;
use crate::report::Abort;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// The command a baseline belongs to: `pun analyse` (`[analyse] baseline`) or `pun lint` (`[lint] baseline`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Analyse,
    Lint,
}

impl Kind {
    pub(crate) fn command(self) -> &'static str {
        match self {
            Kind::Analyse => "analyse",
            Kind::Lint => "lint",
        }
    }

    pub(crate) fn default_file(self) -> &'static str {
        match self {
            Kind::Analyse => "pun-baseline.toml",
            Kind::Lint => "pun-lint-baseline.toml",
        }
    }

    fn header(self) -> String {
        let (cmd, what) = match self {
            Kind::Analyse => ("analyse", "errors"),
            Kind::Lint => ("lint", "problems"),
        };
        format!(
            "# Pun's {cmd} baseline: {what} `pun {cmd}` doesn't report. Regenerate it with\n# `pun {cmd} --generate-baseline`.\n\n"
        )
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Baseline {
    variant: Variant,
    #[serde(default)]
    issues: Vec<Issue>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Variant {
    Loose,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
struct Issue {
    file: String,
    code: String,
    message: String,
    count: u32,
}

type Key = (String, String, String);

/// The entries of a baseline not yet used up: each (file, rule, message) with the count still to hide.
pub(crate) struct Left(BTreeMap<Key, u32>);

impl Left {
    /// Hides one problem when an entry still covers it.
    pub(crate) fn take(&mut self, file: &str, rule: &str, message: &str) -> bool {
        let key = (
            file.replace('\\', "/"),
            rule.to_string(),
            message.to_string(),
        );
        match self.0.get_mut(&key) {
            Some(n) if *n > 0 => {
                *n -= 1;
                true
            }
            _ => false,
        }
    }

    /// The counts no problem used up.
    pub(crate) fn unmatched(&self) -> usize {
        self.0.values().map(|&n| n as usize).sum()
    }
}

impl Baseline {
    /// A baseline of `problems`, each a (file, rule, message).
    pub(crate) fn of<'a>(
        problems: impl IntoIterator<Item = (&'a str, &'a str, &'a str)>,
    ) -> Baseline {
        let mut counts: BTreeMap<Key, u32> = BTreeMap::new();
        for (file, rule, message) in problems {
            let key = (
                file.replace('\\', "/"),
                rule.to_string(),
                message.to_string(),
            );
            *counts.entry(key).or_default() += 1;
        }
        Baseline {
            variant: Variant::Loose,
            issues: (counts.into_iter())
                .map(|((file, code, message), count)| Issue {
                    file,
                    code,
                    message,
                    count,
                })
                .collect(),
        }
    }

    pub(crate) fn read(path: &Path, kind: Kind) -> Result<Baseline> {
        let fail = |message: String| -> anyhow::Error {
            let error = RunError {
                path: Some(path.to_path_buf()),
                line: None,
                message: message.clone(),
                hint: Some(format!(
                    "run `pun {} --generate-baseline` to write it again",
                    kind.command()
                )),
            };
            Abort::pun(format!("{}: {message}", path.display()), error).into()
        };
        let text = std::fs::read_to_string(path)
            .map_err(|e| fail(format!("can't read the baseline: {e}")))?;
        toml::from_str(&text).map_err(|e| fail(format!("invalid baseline: {}", e.message())))
    }

    pub(crate) fn write(&self, path: &Path, kind: Kind) -> Result<()> {
        let text = format!("{}{}", kind.header(), toml::to_string(self)?);
        crate::run::write_generated(path, text.as_bytes()).map_err(|e| {
            let message = format!("can't write the baseline to {}: {e}", path.display());
            Abort::plain(message, None).into()
        })
    }

    pub(crate) fn left(&self) -> Left {
        Left(
            (self.issues.iter())
                .map(|i| ((i.file.clone(), i.code.clone(), i.message.clone()), i.count))
                .collect(),
        )
    }

    /// Drops the ignorable findings the baseline covers; the counts no finding used up.
    pub(crate) fn filter(&self, findings: &mut Vec<Finding>) -> usize {
        let mut left = self.left();
        findings.retain(|f| !(f.ignorable() && left.take(&f.rel, &f.rule, &f.message)));
        left.unmatched()
    }
}
