//! `pun analyse --format=<f>`: PHPStan 2.3.5's error formatters, byte for byte
//! (.oracle/phpstan-src/src/Command/ErrorFormatter/*.php). `json` and `raw` print absolute paths, the others paths
//! relative to the project root (PHPStan's simple relative path helper, the cwd being the root). The errors about
//! ignores can't be ignored, and an unmatched `ignoreErrors` entry has no line (or, without a file path, no file:
//! a not-file-specific error). PHPStan's warnings have no Pun counterpart.

use crate::analyse::Finding;
use anyhow::{Result, bail};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;

pub(crate) const FORMATS: [&str; 7] = [
    "raw",
    "json",
    "checkstyle",
    "junit",
    "gitlab",
    "github",
    "teamcity",
];

pub(crate) struct Context<'a> {
    pub(crate) verbose: bool,
    pub(crate) general: &'a [String],
    pub(crate) config: Option<String>,
}

pub(crate) fn render(format: &str, findings: &[Finding], cx: &Context) -> Result<String> {
    let (verbose, general) = (cx.verbose, cx.general);
    Ok(match format {
        "raw" => raw(findings, general, verbose),
        "json" => json(findings, general),
        "checkstyle" => checkstyle(findings, general),
        "junit" => junit(findings, general),
        "gitlab" => gitlab(findings, general),
        "github" => github(findings, general),
        "teamcity" => teamcity(findings, general, cx.config.as_deref()),
        _ => bail!(
            "Error formatter \"{format}\" not found. Available error formatters are: {}",
            FORMATS.join(", ")
        ),
    })
}

fn line(f: &Finding) -> String {
    f.line().map_or_default(|l| l.to_string())
}

fn raw(findings: &[Finding], general: &[String], verbose: bool) -> String {
    let mut out = String::new();
    for g in general {
        let _ = writeln!(out, "?:?:{g}");
    }
    for f in findings {
        let at = f.line().map_or("?".to_string(), |l| l.to_string());
        let _ = write!(out, "{}:{at}:{}", f.file.display(), f.message);
        if verbose && !f.identifier.is_empty() {
            let _ = write!(out, " [identifier={}]", f.identifier);
        }
        out.push('\n');
    }
    out
}

fn encode(value: &serde_json::Value, pretty: bool) -> String {
    let out = if pretty {
        let mut buf = Vec::new();
        let formatter = serde_json::ser::PrettyFormatter::with_indent(b"    ");
        let mut ser = serde_json::Serializer::with_formatter(&mut buf, formatter);
        serde::Serialize::serialize(value, &mut ser).expect("json");
        String::from_utf8(buf).expect("utf-8")
    } else {
        value.to_string()
    };
    out.replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
}

fn json(findings: &[Finding], general: &[String]) -> String {
    use serde_json::{Map, Value, json};
    let mut files = Map::new();
    for f in findings {
        let entry = files
            .entry(f.file.to_string_lossy().into_owned())
            .or_insert_with(|| json!({ "errors": 0, "messages": [] }));
        entry["errors"] = json!(entry["errors"].as_u64().unwrap_or(0) + 1);
        let mut message = Map::new();
        message.insert("message".into(), json!(f.message));
        message.insert("line".into(), json!(f.line()));
        message.insert("ignorable".into(), json!(f.ignorable()));
        if let Some(tip) = &f.tip {
            message.insert("tip".into(), json!(tip));
        }
        // an error without an identifier (type-coverage's measure mode) has no key
        if !f.identifier.is_empty() {
            message.insert("identifier".into(), json!(f.identifier));
        }
        if f.fixable {
            message.insert("fixable".into(), json!(true));
        }
        entry["messages"]
            .as_array_mut()
            .expect("an array")
            .push(Value::Object(message));
    }
    let value = json!({
        "totals": { "errors": general.len(), "file_errors": findings.len() },
        "files": files,
        "errors": general,
    });
    encode(&value, false)
}

fn xml(text: &str) -> String {
    crate::fix_report::xml_chars(text)
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn by_file<'f>(
    findings: &'f [Finding],
    file: impl Fn(&'f Finding) -> &'f str,
) -> Vec<(&'f str, Vec<&'f Finding>)> {
    let mut files: Vec<(&str, Vec<&Finding>)> = vec![];
    for f in findings {
        match files.iter_mut().find(|(rel, _)| *rel == file(f)) {
            Some((_, list)) => list.push(f),
            None => files.push((file(f), vec![f])),
        }
    }
    files
}

fn checkstyle(findings: &[Finding], general: &[String]) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<checkstyle>\n");
    let files = by_file(findings, Finding::source_rel);
    for (rel, list) in files {
        let _ = writeln!(out, "<file name=\"{}\">", xml(rel));
        for f in list {
            let source = if f.identifier.is_empty() {
                String::new()
            } else {
                format!(" source=\"{}\"", xml(&f.identifier))
            };
            let _ = writeln!(
                out,
                "  <error line=\"{}\" column=\"1\" severity=\"error\" message=\"{}\"{source} />",
                line(f),
                xml(&f.message),
            );
        }
        out.push_str("</file>\n");
    }
    if !general.is_empty() {
        out.push_str("<file>\n");
        for g in general {
            let _ = writeln!(out, "  <error severity=\"error\" message=\"{}\" />", xml(g));
        }
        out.push_str("</file>\n");
    }
    out.push_str("</checkstyle>\n");
    out
}

fn junit(findings: &[Finding], general: &[String]) -> String {
    let n = findings.len() + general.len();
    let mut out = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><testsuite failures=\"{n}\" name=\"phpstan\" tests=\"{}\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xsi:noNamespaceSchemaLocation=\"https://raw.githubusercontent.com/junit-team/junit5/r5.5.1/platform-tests/src/test/resources/jenkins-junit.xsd\">",
        n.max(1)
    );
    for f in findings {
        let _ = write!(
            out,
            "<testcase name=\"{}\"><failure type=\"ERROR\" message=\"{}\" /></testcase>",
            xml(&format!("{}:{}", f.rel, line(f))),
            xml(&f.message)
        );
    }
    for g in general {
        let _ = write!(
            out,
            "<testcase name=\"General error\"><failure type=\"ERROR\" message=\"{}\" /></testcase>",
            xml(g)
        );
    }
    if n == 0 {
        out.push_str("<testcase name=\"phpstan\"></testcase>");
    }
    out.push_str("</testsuite>");
    out
}

fn gitlab(findings: &[Finding], general: &[String]) -> String {
    let sha = |s: String| -> String {
        Sha256::digest(s).iter().fold(String::new(), |mut out, b| {
            let _ = write!(out, "{b:02x}");
            out
        })
    };
    let mut list: Vec<serde_json::Value> = findings
        .iter()
        .map(|f| {
            serde_json::json!({
                "description": f.message,
                "check_name": (!f.identifier.is_empty()).then_some(&f.identifier),
                "fingerprint": sha(format!("{}{}{}", f.file.display(), line(f), f.message)),
                "severity": if f.ignorable() { "major" } else { "blocker" },
                "location": { "path": f.rel, "lines": { "begin": f.line } },
            })
        })
        .collect();
    list.extend(general.iter().map(|g| {
        serde_json::json!({
            "description": g,
            "fingerprint": sha(g.clone()),
            "severity": "major",
            "location": { "path": "", "lines": { "begin": 0 } },
        })
    }));
    encode(&serde_json::Value::Array(list), true)
}

/// PHPStan escapes only a message's newlines. A `\r` also ends a workflow command, and a path is a property, so
/// those get GitHub's own escapes too (`@actions/core`); output differs only when they hold such characters.
fn github(findings: &[Finding], general: &[String]) -> String {
    let message = |m: &str| crate::github::mentions(&m.replace('\n', "%0A").replace('\r', "%0D"));
    let mut out = String::new();
    for f in findings {
        let _ = writeln!(
            out,
            "::error file={},line={},col=0::{}",
            crate::github::escape_property(&f.rel),
            line(f),
            message(&f.message)
        );
    }
    for g in general {
        let _ = writeln!(out, "::error ::{}", message(g));
    }
    out
}

fn teamcity(findings: &[Finding], general: &[String], config: Option<&str>) -> String {
    if findings.is_empty() && general.is_empty() {
        return String::new();
    }
    let escape = |s: &str| {
        let mut o = String::new();
        for c in s.chars() {
            match c {
                '\'' | '|' | '[' | ']' => {
                    o.push('|');
                    o.push(c);
                }
                c => o.push(c),
            }
        }
        o
    };
    let mut out = String::from(
        "##teamcity[inspectionType id='phpstan' name='phpstan' category='phpstan' description='phpstan Inspection']\n",
    );
    for f in findings {
        let message = if f.ignorable() && !f.identifier.is_empty() {
            format!("{} (🪪 {})", f.message, f.identifier)
        } else {
            f.message.clone()
        };
        let _ = writeln!(
            out,
            "##teamcity[inspection typeId='phpstan' message='{}' file='{}' line='{}' SEVERITY='ERROR' ignorable='{}' tip='{}']",
            escape(&message),
            escape(&f.rel),
            line(f),
            if f.ignorable() { "1" } else { "" },
            escape(f.tip.as_deref().unwrap_or(""))
        );
    }
    for g in general {
        let _ = writeln!(
            out,
            "##teamcity[inspection typeId='phpstan' message='{}' file='{}' SEVERITY='ERROR']",
            escape(g),
            escape(config.unwrap_or("."))
        );
    }
    out
}

#[cfg(test)]
mod tests {
    /// A control character XML forbids, in a file name or a message, doesn't reach the Checkstyle or `JUnit` report.
    #[test]
    fn xml_drops_forbidden_characters() {
        assert_eq!(super::xml("a\x0c<\tb\u{FFFE}"), "a\u{FFFD}&lt;\tb\u{FFFD}");
    }
}
