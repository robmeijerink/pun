mod common;

use common::{PUN, clean_env, project, read};
use std::path::Path;
use std::process::Command;

const TOML: &str = r#"[format]
preset = "empty"

[lint.rules]
"security/no-eval" = "error"
"security/no-global" = "warn"
"complexity/no-negated-boolean-literal" = "error"
"comments/no-unused-ignore" = "error"

[[lint.overrides]]
files = ["src/legacy/**"]
rules = { "security/no-eval" = "off" }
"#;

const A: &str = "<?php\n\nfunction f()\n{\n    global $y;\n    eval('1;');\n}\n\n$a = !true; // @pun-ignore lint/security/no-eval\n";
const LEGACY: &str = "<?php\n\neval('2;');\n";

fn agent(dir: &Path, args: &[&str]) -> (Vec<String>, String, Option<i32>) {
    let out = clean_env(&mut Command::new(PUN), &["PATH", "HOME"])
        .current_dir(dir)
        .env("PUN_AGENT", "1")
        .args(args)
        .output()
        .unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    let mut lines = stdout.lines().map(str::to_string);
    let summary = lines.next().unwrap_or_default();
    let summary = summary.split(" (").next().unwrap().to_string();
    (lines.collect(), summary, out.status.code())
}

fn sample() -> std::path::PathBuf {
    project(
        "lint",
        "sample",
        &[
            ("pun.toml", TOML),
            ("src/a.php", A),
            ("src/legacy/b.php", LEGACY),
        ],
    )
}

#[test]
fn levels_overrides_and_ignores() {
    let d = sample();
    let (lines, summary, code) = agent(&d, &["lint"]);
    assert_eq!(summary, "pun lint: fail, 3 errors, 1 warning");
    assert_eq!(
        lines,
        [
            "src/a.php:5:5 lint/security/no-global [warn] Unsafe use of `global` keyword.",
            "src/a.php:6:5 lint/security/no-eval Unsafe use of `eval` construct.",
            "src/a.php:9:6 lint/complexity/no-negated-boolean-literal [fix:safe] Use `false` instead of negating a boolean literal.",
            "src/a.php:9:28 lint/comments/no-unused-ignore [fix:safe] `lint/security/no-eval` suppresses no problem here",
        ]
    );
    assert_eq!(code, Some(1));
}

/// Lint and check read standard input only to fix it: a run that would only check it is a run error, not a pass.
#[test]
fn stdin_is_not_passed_unchecked() {
    use std::io::Write as _;
    let d = sample();
    let pipe = |args: &[&str]| {
        let mut child = clean_env(&mut Command::new(PUN), &["PATH", "HOME"])
            .current_dir(&d)
            .env("PUN_AGENT", "0")
            .args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        // A run that refuses standard input may exit before reading it.
        let _ = child.stdin.take().unwrap().write_all(LEGACY.as_bytes());
        let out = child.wait_with_output().unwrap();
        (String::from_utf8(out.stdout).unwrap(), out.status.code())
    };
    for args in [
        &["lint", "-"][..],
        &["lint", "--stdin-filename=src/a.php"],
        &["check", "-"],
        &["check", "--test", "--stdin-filename=src/a.php", "-"],
    ] {
        assert_eq!(pipe(args), (String::new(), Some(2)), "{args:?}");
    }
    assert_eq!(pipe(&["lint", "--fix", "-"]), (LEGACY.to_string(), Some(0)));
}

/// A file that isn't UTF-8 still gets its native rules (on its lossy text), without a fix that would rewrite it.
#[test]
fn files_that_are_not_utf8_are_still_linted() {
    let d = project("lint", "latin1", &[("pun.toml", TOML)]);
    let latin1 = b"<?php\n\n// caf\xe9\neval('1;');\n";
    std::fs::write(d.join("a.php"), latin1).unwrap();
    let (lines, _, code) = agent(&d, &["lint"]);
    assert_eq!(
        lines,
        ["a.php:4:1 lint/security/no-eval Unsafe use of `eval` construct."]
    );
    assert_eq!(code, Some(1));
    let (_, _, code) = agent(&d, &["lint", "--fix"]);
    assert_eq!(code, Some(1));
    assert_eq!(std::fs::read(d.join("a.php")).unwrap(), latin1);
}

#[test]
fn command_line_levels_apply_in_order() {
    let d = sample();
    let (lines, _, code) = agent(
        &d,
        &[
            "lint",
            "-A",
            "security",
            "-D",
            "security/no-global",
            "-A",
            "complexity/no-negated-boolean-literal",
        ],
    );
    assert_eq!(
        lines,
        ["src/a.php:5:5 lint/security/no-global Unsafe use of `global` keyword.",]
    );
    assert_eq!(code, Some(1));
    let (lines, _, code) = agent(&d, &["lint", "-D", "nope/nothing"]);
    assert_eq!(lines, Vec::<String>::new());
    assert_eq!(code, Some(2));
}

#[test]
fn warnings_fail_only_when_asked() {
    let d = project(
        "lint",
        "warnings",
        &[
            (
                "pun.toml",
                "[lint.rules]\n\"security/no-global\" = \"warn\"\n",
            ),
            (
                "a.php",
                "<?php\n\nfunction f()\n{\n    global $y; // @pun-ignore lint/security/no-eval\n}\n",
            ),
        ],
    );
    assert_eq!(agent(&d, &["lint"]).2, Some(0));
    assert_eq!(agent(&d, &["lint", "--deny-warnings"]).2, Some(1));
    assert_eq!(agent(&d, &["lint", "--max-warnings", "1"]).2, Some(0));
    assert_eq!(agent(&d, &["lint", "--max-warnings", "0"]).2, Some(1));
}

#[test]
fn fix_applies_safe_fixes_and_deletes_unused_ignores() {
    let d = sample();
    let (lines, summary, code) = agent(&d, &["lint", "--fix"]);
    assert_eq!(
        read(&d, "src/a.php"),
        "<?php\n\nfunction f()\n{\n    global $y;\n    eval('1;');\n}\n\n$a = false;\n"
    );
    assert_eq!(read(&d, "src/legacy/b.php"), LEGACY);
    assert_eq!(summary, "pun lint: fail, 1 error, 1 file fixed, 1 warning");
    assert!(lines.contains(
        &"src/a.php:6:5 lint/security/no-eval Unsafe use of `eval` construct.".to_string()
    ));
    assert_eq!(code, Some(1));
}

#[test]
fn fix_repeats_until_no_rule_has_work_left() {
    let d = project(
        "lint",
        "fix-settles",
        &[
            (
                "pun.toml",
                "[lint.rules]\n\"phpdoc/no-useless-var\" = \"error\"\n\"style/add-property-type-from-phpdoc\" = \"error\"\n",
            ),
            (
                "a.php",
                "<?php\n\nclass A extends B\n{\n    /**\n     * The name of the thing.\n     *\n     * @var string\n     */\n    protected $name = 'a';\n}\n",
            ),
        ],
    );
    let (_, _, code) = agent(&d, &["lint", "--fix"]);
    assert_eq!(
        read(&d, "a.php"),
        "<?php\n\nclass A extends B\n{\n    /**\n     * The name of the thing.\n     */\n    protected string $name = 'a';\n}\n"
    );
    assert_eq!(code, Some(0));
}

#[test]
fn fix_runs_the_format_rules_once_when_no_lint_rule_fires() {
    let d = project(
        "lint",
        "fix-format-once",
        &[
            (
                "pun.toml",
                "[format]\npreset = \"empty\"\n\n[format.rules]\n\"tags/blank-line-after-opening\" = true\n\"whitespace/no-extra-blank-lines\" = [\"extra\", \"throw\", \"use\"]\n\"whitespace/no-blank-line-whitespace\" = true\n\"whitespace/final-newline\" = true\n\n[lint.rules]\n\"style/no-alias-function\" = \"error\"\n",
            ),
            ("a.php", "<?php  \n  \nnamespace A;  \n"),
        ],
    );
    agent(&d, &["check", "--fix"]);
    assert_eq!(read(&d, "a.php"), "<?php\n\n  namespace A;\n");
}

#[test]
fn fix_off_keeps_a_rule_report_only() {
    let toml = TOML.replace(
        "\"complexity/no-negated-boolean-literal\" = \"error\"",
        "\"complexity/no-negated-boolean-literal\" = { level = \"error\", fix = \"off\" }",
    );
    let d = project(
        "lint",
        "fix-off",
        &[("pun.toml", &toml), ("a.php", "<?php\n\n$a = !true;\n")],
    );
    let (lines, _, code) = agent(&d, &["lint", "--fix"]);
    assert_eq!(read(&d, "a.php"), "<?php\n\n$a = !true;\n");
    assert_eq!(
        lines,
        [
            "a.php:3:6 lint/complexity/no-negated-boolean-literal Use `false` instead of negating a boolean literal."
        ]
    );
    assert_eq!(code, Some(1));
}

#[test]
fn ignore_comment_forms() {
    let source = "<?php\n\n// @pun-ignore-next-line security/no-eval\neval('1;');\neval('2;'); // @pun-ignore lint/security\n// @pun-ignore\neval('3;');\n";
    let d = project("lint", "ignores", &[("pun.toml", TOML), ("a.php", source)]);
    let (lines, _, code) = agent(&d, &["lint"]);
    assert_eq!(
        lines,
        [
            "a.php:6:4 invalid-ignore-comment `@pun-ignore` needs one or more rule IDs",
            "a.php:7:1 lint/security/no-eval Unsafe use of `eval` construct.",
        ]
    );
    assert_eq!(code, Some(1));
}

#[test]
fn lint_skips_blade_and_reports_syntax_errors() {
    let d = project(
        "lint",
        "files",
        &[
            ("pun.toml", TOML),
            ("v.blade.php", "<?php eval('1;'); ?>\n"),
            ("storage/s.php", "<?php\neval('1;');\n"),
            ("bad.php", "<?php\nfunction (\n"),
        ],
    );
    let (lines, summary, code) = agent(&d, &["lint"]);
    assert_eq!(summary, "pun lint: fail, 1 error");
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].starts_with("bad.php:") && lines[0].contains(" syntax-error "),
        "{lines:?}"
    );
    assert_eq!(code, Some(1));
}

#[test]
fn lint_survives_a_deep_expression() {
    let deep = format!("<?php\n\n$a = 1{};\neval('1;');\n", " . 'a'".repeat(20_000));
    let d = project("lint", "deep", &[("pun.toml", TOML), ("a.php", &deep)]);
    let (lines, summary, code) = agent(&d, &["lint"]);
    assert_eq!(summary, "pun lint: fail, 1 error");
    assert!(lines[0].starts_with("a.php:4:"), "{lines:?}");
    assert_eq!(code, Some(1));
}

#[test]
fn types_rules_see_the_project_classes() {
    let d = project(
        "lint",
        "codebase",
        &[
            (
                "pun.toml",
                "[lint.rules]\n\"types/add-string-return-type-from-scalar\" = \"error\"\n\"types/add-arrow-function-return-type\" = \"error\"\n",
            ),
            (
                "src/Job.php",
                "<?php\n\nnamespace App;\n\nuse Vendor\\PendingDispatch;\n\nclass Job\n{\n    public static function go(): ?PendingDispatch\n    {\n        return null;\n    }\n}\n",
            ),
            (
                "src/T.php",
                "<?php\n\nuse App\\Job;\n\n$x = fn () => Job::go()->job;\n",
            ),
            (
                "src/Base.php",
                "<?php\n\nnamespace App;\n\nabstract class Base\n{\n}\n",
            ),
            (
                "src/Child.php",
                "<?php\n\nnamespace App;\n\nclass Child extends Base\n{\n    public function prefix()\n    {\n        return '';\n    }\n}\n",
            ),
            (
                "composer.json",
                "{\"autoload\": {\"psr-0\": {\"Legacy_\": \"vendor/legacy/lib/\"}}}\n",
            ),
            (
                "vendor/legacy/lib/Legacy/Base.php",
                "<?php\n\nclass Legacy_Base\n{\n}\n",
            ),
            (
                "src/Old.php",
                "<?php\n\nnamespace App;\n\nclass Old extends \\Legacy_Base\n{\n    public function prefix()\n    {\n        return '';\n    }\n}\n",
            ),
            (
                "vendor/mapped/Base.php",
                "<?php\n\nnamespace Mapped;\n\nclass Base\n{\n}\n",
            ),
            ("lib/LocalBase.php", "<?php\n\nclass Local_Base\n{\n}\n"),
            (
                "src/FromMap.php",
                "<?php\n\nnamespace App;\n\nclass FromMap extends \\Mapped\\Base\n{\n    public function prefix()\n    {\n        return '';\n    }\n}\n",
            ),
            (
                "src/FromLib.php",
                "<?php\n\nnamespace App;\n\nclass FromLib extends \\Local_Base\n{\n    public function prefix()\n    {\n        return '';\n    }\n}\n",
            ),
        ],
    );
    std::fs::create_dir_all(d.join("vendor/composer")).unwrap();
    std::fs::write(
        d.join("vendor/composer/autoload_classmap.php"),
        [
            &b"<?php\n\n// autoload_classmap.php @generated by Composer\n\n$vendorDir = dirname(__DIR__);\n$baseDir = dirname($vendorDir);\n\nreturn array(\n    '"[..],
            b"\xa9",
            b"' => $vendorDir . '/symfony/cache/Traits/ValueWrapper.php',\n    'Composer\\\\InstalledVersions' => $vendorDir . '/composer/InstalledVersions.php',\n    'Local_Base' => $baseDir . '/lib/LocalBase.php',\n    'Mapped\\\\Base' => $vendorDir . '/mapped/Base.php',\n);\n",
        ]
        .concat(),
    )
    .unwrap();
    // Composer's `files` autoloads: a vendor file's classes are known (a listed file that is gone is skipped).
    std::fs::write(
        d.join("vendor/composer/autoload_files.php"),
        "<?php\n\n// autoload_files.php @generated by Composer\n\n$vendorDir = dirname(__DIR__);\n$baseDir = dirname($vendorDir);\n\nreturn array(\n    '0e6d7bf4a5811bfa5cf40c5ccd6fae6a' => $vendorDir . '/acme/functions.php',\n    '1f6d7bf4a5811bfa5cf40c5ccd6fae6a' => $vendorDir . '/acme/gone.php',\n    '2a6d7bf4a5811bfa5cf40c5ccd6fae6a' => $baseDir . '/lib/LocalBase.php',\n);\n",
    )
    .unwrap();
    common::write_files(
        &d,
        &[
            (
                "vendor/acme/functions.php",
                "<?php\n\nclass Acme_Files_Base\n{\n}\n",
            ),
            (
                "src/FromFiles.php",
                "<?php\n\nnamespace App;\n\nclass FromFiles extends \\Acme_Files_Base\n{\n    public function prefix()\n    {\n        return '';\n    }\n}\n",
            ),
        ],
    );
    let (lines, summary, code) = agent(
        &d,
        &[
            "lint",
            "src/Child.php",
            "src/T.php",
            "src/Old.php",
            "src/FromMap.php",
            "src/FromLib.php",
            "src/FromFiles.php",
        ],
    );
    assert_eq!(summary, "pun lint: fail, 5 errors");
    assert!(lines[0].starts_with("src/Child.php:7:"), "{lines:?}");
    assert!(lines[1].starts_with("src/FromFiles.php:7:"), "{lines:?}");
    assert!(lines[2].starts_with("src/FromLib.php:7:"), "{lines:?}");
    assert!(lines[3].starts_with("src/FromMap.php:7:"), "{lines:?}");
    assert!(lines[4].starts_with("src/Old.php:7:"), "{lines:?}");
    assert_eq!(code, Some(1));
}

#[test]
fn fact_rules_see_the_project_classes() {
    let child = "<?php\n\nnamespace App;\n\nclass Child extends Base\n{\n    public function run()\n    {\n    }\n}\n";
    let d = project(
        "lint",
        "parent",
        &[
            (
                "pun.toml",
                "[lint.rules]\n\"style/require-parent-visibility\" = \"error\"\n",
            ),
            (
                "src/Base.php",
                "<?php\n\nnamespace App;\n\nclass Base\n{\n    protected function run()\n    {\n    }\n}\n",
            ),
            ("src/Child.php", child),
        ],
    );
    let (_, code) = plain(&d, &["--fix", "src/Child.php"], &[]);
    assert_eq!(
        read(&d, "src/Child.php"),
        child.replace("public function run()", "protected function run()")
    );
    assert_eq!(code, Some(0));
}

#[test]
fn native_rule_options_reach_the_rule() {
    let d = project(
        "lint",
        "options",
        &[
            (
                "pun.toml",
                "[format]\npreset = \"empty\"\n[lint.rules]\n\"security/no-global\" = { level = \"error\", allow = [\"x\"] }\n",
            ),
            ("a.php", "<?php\n"),
        ],
    );
    let (lines, summary, code) = agent(&d, &["lint"]);
    assert_eq!(
        (lines, summary.as_str(), code),
        (
            vec![],
            "pun lint: error: pun.toml: `lint/security/no-global` takes no options",
            Some(2)
        )
    );
    std::fs::write(
        d.join("pun.toml"),
        "[format]\npreset = \"empty\"\n[lint.rules]\n\"security/no-global\" = \"error\"\n[[lint.overrides]]\nfiles = [\"a.php\"]\nrules = { \"security/no-global\" = { allow = [\"x\"] } }\n",
    )
    .unwrap();
    let (_, summary, code) = agent(&d, &["lint"]);
    assert_eq!(
        (summary.as_str(), code),
        (
            "pun lint: error: pun.toml: `lint/security/no-global` takes options in [lint.rules] only, not in [[lint.overrides]]",
            Some(2)
        )
    );
}

#[test]
fn overrides_set_levels_and_name_only_ported_rules() {
    let toml = "[format]\npreset = \"empty\"\n\n[lint.rules]\n\"security/no-eval\" = \"error\"\n\"security/no-global\" = \"warn\"\n\n[[lint.overrides]]\nfiles = [\"src/legacy/**\"]\nrules = { \"security/no-eval\" = \"warn\" }\n";
    let d = project(
        "lint",
        "override-levels",
        &[
            ("pun.toml", toml),
            ("src/a.php", "<?php\n\nfunction f()\n{\n    global $y;\n}\n"),
            ("src/legacy/b.php", LEGACY),
        ],
    );
    let global = "src/a.php:5:5 lint/security/no-global Unsafe use of `global` keyword.";
    let eval = "src/legacy/b.php:3:1 lint/security/no-eval Unsafe use of `eval` construct.";
    let warn = |l: &str| l.replacen(" Unsafe", " [warn] Unsafe", 1);
    assert_eq!(
        agent(&d, &["lint"]),
        (
            vec![warn(global), warn(eval)],
            "pun lint: pass, 2 warnings".into(),
            Some(0)
        )
    );
    assert_eq!(
        agent(
            &d,
            &[
                "lint",
                "-W",
                "lint/security/no-global",
                "-D",
                "security/no-eval"
            ]
        ),
        (
            vec![warn(global), eval.into()],
            "pun lint: fail, 1 error, 1 warning".into(),
            Some(1)
        )
    );
    // An override that turns on a rule Pun has not ported is a config error naming it.
    std::fs::write(
        d.join("pun.toml"),
        format!("{toml}\n[[lint.overrides]]\nfiles = [\"x\"]\nrules = {{ \"laravel/prefer-pivot-class\" = \"error\" }}\n"),
    )
    .unwrap();
    let (lines, summary, code) = agent(&d, &["lint"]);
    assert_eq!((lines, code), (vec![], Some(2)));
    assert!(
        summary.ends_with("rule not supported by Pun yet: `lint/laravel/prefer-pivot-class`"),
        "{summary}"
    );
}

fn plain(dir: &Path, args: &[&str], env: &[(&str, &str)]) -> (String, Option<i32>) {
    let out = clean_env(&mut Command::new(PUN), &["PATH", "HOME"])
        .current_dir(dir)
        .env("PUN_AGENT", "0")
        .envs(env.iter().copied())
        .arg("lint")
        .args(args)
        .output()
        .unwrap();
    (String::from_utf8(out.stdout).unwrap(), out.status.code())
}

#[test]
fn problems_in_every_report() {
    let toml = "[format]\npreset = \"empty\"\n\n[lint.rules]\n\"complexity/no-negated-boolean-literal\" = \"error\"\n\"security/no-eval\" = \"error\"\n";
    let d = project(
        "lint",
        "reports",
        &[
            ("pun.toml", toml),
            ("a.php", "<?php\n$n = !true;\neval('1;');\neval('2;');\n"),
        ],
    );
    let negated = "Use `false` instead of negating a boolean literal.";
    let (lines, _, code) = agent(&d, &["lint"]);
    assert_eq!(
        lines,
        [
            format!("a.php:2:6 lint/complexity/no-negated-boolean-literal [fix:safe] {negated}"),
            "a.php:3:1 lint/security/no-eval Unsafe use of `eval` construct.".into(),
            "a.php:4:1 lint/security/no-eval Unsafe use of `eval` construct.".into(),
        ]
    );
    assert_eq!(code, Some(1));
    assert_eq!(
        plain(&d, &["-v", "--format=json"], &[]),
        (
            r#"{"about":"PHP CS Fixer 3.95.25 Adalbertus by Fabien Potencier, Dariusz Ruminski and contributors.","files":[{"name":"a.php","appliedFixers":["lint\/complexity\/no-negated-boolean-literal","lint\/security\/no-eval"]}],"time":{"total":0},"memory":0}"#.into(),
            Some(1)
        )
    );
    let annotations = format!(
        "::error file=a.php,line=2,col=6,title=lint/complexity/no-negated-boolean-literal::{negated}\n::error file=a.php,line=3,col=1,title=lint/security/no-eval::Unsafe use of `eval` construct.\n::error file=a.php,line=4,col=1,title=lint/security/no-eval::Unsafe use of `eval` construct.\n"
    );
    assert_eq!(
        plain(&d, &["--format=github"], &[]),
        (annotations.clone(), Some(1))
    );
    assert_eq!(
        plain(
            &d,
            &["--output-to-file=r.txt", "--output-format=github", "-q"],
            &[]
        )
        .1,
        Some(1)
    );
    assert_eq!(read(&d, "r.txt"), annotations);
    let (out, code) = plain(&d, &["--color=never"], &[("GITHUB_ACTIONS", "true")]);
    assert!(out.ends_with(&annotations), "{out}");
    assert_eq!(code, Some(1));
}

#[test]
fn token_rules_beside_native_ones() {
    let toml = "[format]\npreset = \"empty\"\n\n[lint.rules]\n\"style/no-alias-function\" = \"warn\"\n\"comments/no-unused-ignore\" = \"error\"\n\"security/no-eval\" = \"error\"\n";
    let ignore = "<?php\n$a = 1; // @pun-ignore lint/style/no-alias-function\n$b = 2; // @pun-ignore lint/comments/no-unused-ignore\n";
    let d = project(
        "lint",
        "token",
        &[
            ("pun.toml", toml),
            ("a.php", &format!("{ignore}$n = sizeof($a); eval('1;');\n")),
        ],
    );
    let alias = "a.php lint/style/no-alias-function [fix:unsafe] Master functions shall be used instead of aliases.";
    let (lines, _, code) = agent(&d, &["lint"]);
    assert_eq!(
        lines,
        [
            "a.php:4:18 lint/security/no-eval Unsafe use of `eval` construct.",
            alias
        ]
    );
    assert_eq!(code, Some(1));
    std::fs::write(d.join("a.php"), format!("{ignore}$n = sizeof($a);\n")).unwrap();
    assert_eq!(agent(&d, &["lint"]).2, Some(0));
    assert_eq!(agent(&d, &["lint", "--deny-warnings"]).2, Some(1));
    // With --fix, the native rule fixes both negations, the token rule shortens `sizeof` to `count`, and the
    // problem left is reported where it is in the fixed file (column 17, not 18).
    std::fs::write(
        d.join("pun.toml"),
        format!("{toml}\"complexity/no-negated-boolean-literal\" = \"error\"\n"),
    )
    .unwrap();
    let line = "$n = sizeof($a); eval('1;'); $b = !true; $c = !false;\n";
    std::fs::write(d.join("a.php"), format!("<?php\n{line}")).unwrap();
    let (lines, _, code) = agent(&d, &["lint", "--fix"]);
    assert_eq!(
        read(&d, "a.php"),
        "<?php\n$n = count($a); eval('1;'); $b = false; $c = true;\n"
    );
    let eval = "a.php:2:17 lint/security/no-eval Unsafe use of `eval` construct.";
    assert!(lines.iter().any(|l| l == eval), "{lines:?}");
    assert_eq!(code, Some(1));
}

#[test]
fn dirty_narrows_to_the_lint_files() {
    let eval = "<?php\n\neval('1;');\n";
    let d = project(
        "lint",
        "dirty",
        &[("pun.toml", TOML), ("a.php", eval), ("b.php", eval)],
    );
    let git = |args: &[&str]| {
        let ok = Command::new("git")
            .args([
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .current_dir(&d)
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok, "git {args:?}");
    };
    git(&["init", "-q"]);
    git(&["add", "."]);
    git(&["commit", "-qm", "init"]);
    std::fs::write(d.join("b.php"), format!("{eval}\n")).unwrap();
    std::fs::write(d.join("v.blade.php"), "<?php eval('1;'); ?>\n").unwrap();
    let (lines, _, code) = agent(&d, &["lint", "--dirty"]);
    assert_eq!(
        lines,
        ["b.php:3:1 lint/security/no-eval Unsafe use of `eval` construct."]
    );
    assert_eq!(code, Some(1));

    // `[format] exclude` is for formatting only: the changed file it names is still linted.
    let toml = TOML.replace(
        "preset = \"empty\"",
        "preset = \"empty\"\nexclude = [\"fmt\"]",
    );
    std::fs::write(d.join("pun.toml"), toml).unwrap();
    std::fs::create_dir(d.join("fmt")).unwrap();
    std::fs::write(d.join("fmt/c.php"), eval).unwrap();
    git(&["add", "fmt/c.php"]);
    std::fs::remove_file(d.join("b.php")).unwrap();
    let (lines, _, code) = agent(&d, &["lint", "--dirty"]);
    assert_eq!(
        lines,
        ["fmt/c.php:3:1 lint/security/no-eval Unsafe use of `eval` construct."]
    );
    assert_eq!(code, Some(1));
}

#[test]
fn class_method_rules_see_project_declarations() {
    let external = r"<?php

abstract class Base
{
    public function __construct(int $n = 1) {}
    abstract public function required();
    public function passthrough(int $n): int { return $n; }
    protected function defaults($n = 1) {}
}

trait CallsKept
{
    public function callKept() { $this->kept(); }
}
";
    let child = r"<?php

final class Child extends Base
{
    use CallsKept;

    public function __construct(int $n = 1)
    {
        parent::__construct($n);
    }

    public function required()
    {
    }

    public function passthrough(int $n): int
    {
        return parent::passthrough($n);
    }

    public function execute()
    {
        parent::defaults(1);
    }

    private function kept()
    {
    }

    private function gone()
    {
    }
}
";
    let expected = r"<?php

final class Child extends Base
{
    use CallsKept;

    public function __construct(int $n = 1)
    {
        parent::__construct($n);
    }

    public function required()
    {
    }

    public function execute()
    {
        parent::defaults();
    }

    private function kept()
    {
    }
}
";
    let config = r#"[format]
preset = "empty"

[lint.rules]
"dead-code/no-empty-method" = "error"
"dead-code/no-redundant-default-argument" = "error"
"dead-code/no-unused-private-method" = "error"
"dead-code/no-useless-override" = "error"
"#;
    let d = project(
        "lint",
        "class-method-project",
        &[
            ("pun.toml", config),
            ("Child.php", child),
            ("External.php", external),
        ],
    );
    let out = clean_env(&mut Command::new(PUN), &["PATH", "HOME"])
        .current_dir(&d)
        .env("PUN_AGENT", "0")
        .args(["lint", "--fix", "Child.php"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert_eq!(read(&d, "Child.php"), expected);
    assert_eq!(read(&d, "External.php"), external);
    std::fs::remove_dir_all(d).unwrap();
}

/// `pun check`'s grouped view (context/plan/output.md, Build 5): a step line with its counts, then each
/// problem as `path:line:col  rule` with the message under it. A file that needs formatting and one that doesn't
/// parse share the format step, a lint warning is ⚠ (`!` in ASCII), and with --fix a fix the rule may not make shows
/// under lint at the first line it changes, with the rule's summary.
#[test]
fn check_groups_problems_under_their_step() {
    let toml = "[format]\npreset = \"empty\"\n\n[format.rules]\n\"arrays/syntax\" = true\n\n[lint.rules]\n\"security/no-global\" = \"warn\"\n\"dead-code/no-self-assignment\" = { fix = \"off\" }\n\n[analyse]\npaths = [\"app/Clean.php\"]\n";
    let d = project(
        "lint",
        "check-groups",
        &[
            ("pun.toml", toml),
            ("app/A.php", "<?php\n$a = array(1);\n"),
            ("app/Broken.php", "<?php\n$a = (;\n"),
            ("app/Clean.php", "<?php\n$c = [1];\n"),
            ("app/G.php", "<?php\n\nfunction f()\n{\n    global $y;\n}\n"),
        ],
    );
    let check = |args: &[&str]| {
        let out = clean_env(&mut Command::new(PUN), &["PATH", "HOME"])
            .current_dir(&d)
            .env("PUN_AGENT", "0")
            .arg("check")
            .args(args)
            .output()
            .unwrap();
        (String::from_utf8(out.stdout).unwrap(), out.status.code())
    };
    let (out, code) = check(&["--color=never"]);
    assert!(
        out.starts_with("x  format    1 file needs formatting, 1 syntax error\n   app/A.php  format/arrays/syntax\n   app/Broken.php:2:"),
        "{out}"
    );
    assert!(
        out.contains("\n!   lint     1 warning in 1 file\n   app/G.php:5:5  lint/security/no-global\n   Unsafe use of `global` keyword.\n\nok  analyse   No problems\n"),
        "{out}"
    );
    assert_eq!(code, Some(1));
    std::fs::remove_file(d.join("app/Broken.php")).unwrap();
    std::fs::write(d.join("app/S.php"), "<?php\n$a = 1;\n$a = $a;\n").unwrap();
    let (out, code) = check(&["--fix", "--color=never"]);
    assert_eq!(read(&d, "app/A.php"), "<?php\n$a = [1];\n");
    assert!(
        out.starts_with("ok  format    Formatted 1 file\nx   lint     1 error, 1 warning in 2 files\n   app/G.php:5:5  lint/security/no-global\n   Unsafe use of `global` keyword.\n   app/S.php:3  lint/dead-code/no-self-assignment\n   Detects and removes self-assignments where a variable or property is assigned to itself.\n\n"),
        "{out}"
    );
    assert_eq!(code, Some(1));
}

/// A `@php-cs-fixer-ignore` naming a rule that isn't on stops the run (PHP-CS-Fixer's "not in the current set of
/// enabled rules" error), also when `--fix` only reports what a `fix = "off"` rule would change.
#[test]
fn ignore_annotation_errors_stop_report_only_fixes_too() {
    let d = project(
        "lint",
        "annotation",
        &[
            (
                "pun.toml",
                "[lint.rules]\n\"dead-code/no-self-assignment\" = { fix = \"off\" }\n",
            ),
            ("a.php", "<?php\n// @php-cs-fixer-ignore nope\n$a = $a;\n"),
        ],
    );
    for args in [&["lint"][..], &["lint", "--fix"]] {
        let (lines, summary, code) = agent(&d, args);
        assert_eq!((lines, code), (vec![], Some(2)), "{args:?}");
        assert!(
            summary.starts_with("pun lint: error: @php-cs-fixer-ignore annotation(s) used for rules that are not in the current set of enabled rules:"),
            "{args:?}: {summary}"
        );
    }
}

/// `pun check` reports a malformed `@pun-ignore` once, though both lint and analyse read the file's comments.
#[test]
fn check_reports_a_malformed_ignore_once() {
    let d = project(
        "lint",
        "malformed-once",
        &[
            (
                "pun.toml",
                "[format]\npreset = \"empty\"\n\n[lint.rules]\n\"security/no-eval\" = \"error\"\n",
            ),
            ("a.php", "<?php\n\nnope(); // @pun-ignore\n"),
        ],
    );
    let (lines, _, code) = agent(&d, &["check"]);
    assert_eq!(
        lines,
        [
            "a.php:3:1 analyse/undefined/function Function nope not found.",
            "a.php:3:12 invalid-ignore-comment `@pun-ignore` needs one or more rule IDs",
        ]
    );
    assert_eq!(code, Some(1));
}

/// Two rules that undo each other (a token rule makes the private method static, the native one makes it an
/// instance method again): `--fix` stops once a round leaves the file as the one before it, with the token rule's
/// result (PHP-CS-Fixer's `static_private_method` on the same file), and names the token rule once though it ran in
/// both rounds.
#[test]
fn rules_that_undo_each_other_settle_and_are_named_once() {
    let d = project(
        "lint",
        "undo",
        &[
            (
                "pun.toml",
                "[format]\npreset = \"empty\"\n\n[lint.rules]\n\"style/prefer-static-private-method\" = \"error\"\n\"complexity/prefer-instance-method\" = \"error\"\n",
            ),
            (
                "a.php",
                "<?php\n\nfinal class A\n{\n    public function run(): void\n    {\n        $this->go();\n    }\n\n    private function go(): void\n    {\n    }\n}\n",
            ),
        ],
    );
    let (lines, _, code) = agent(&d, &["lint", "--fix"]);
    assert_eq!(
        read(&d, "a.php"),
        "<?php\n\nfinal class A\n{\n    public function run(): void\n    {\n        self::go();\n    }\n\n    private static function go(): void\n    {\n    }\n}\n"
    );
    let named = (lines.iter())
        .filter(|l| l.starts_with("a.php lint/style/prefer-static-private-method "))
        .count();
    assert_eq!((named, code), (1, Some(0)), "{lines:?}");
}

/// `pun check` reports the files Pint would format, with Pint's fixers, though a lint fix changed them too: Pint 1.32
/// (laravel preset) on these two files flags only `B.php`, for `not_operator_with_successor_space`. Before, a lint fix
/// in `A.php` made `unary_operator_spaces` and the space after `!` count as applied there.
#[test]
fn check_formats_like_pint_beside_lint_fixes() {
    let toml = "[format]\npreset = \"laravel\"\n\n[lint.rules]\n\"laravel/no-implicit-model-query\" = \"error\"\n\n[analyse]\npaths = [\"Clean.php\"]\n";
    let d = project(
        "lint",
        "check-format-beside-lint",
        &[
            ("pun.toml", toml),
            (
                "User.php",
                "<?php\n\nuse Illuminate\\Database\\Eloquent\\Model;\n\nclass User extends Model\n{\n    //\n}\n",
            ),
            (
                "A.php",
                "<?php\n\n$a = User::where(1)->first();\nif (! $a) {\n    echo 1;\n}\n",
            ),
            (
                "B.php",
                "<?php\n\n$b = User::where(1)->first();\nif (!$b) {\n    echo 1;\n}\n",
            ),
            ("Clean.php", "<?php\n\n$c = [1];\n"),
        ],
    );
    let out = clean_env(&mut Command::new(PUN), &["PATH", "HOME"])
        .current_dir(&d)
        .env("PUN_AGENT", "0")
        .args(["check", "--test", "--no-cache", "--color=never"])
        .output()
        .unwrap();
    let out = String::from_utf8(out.stdout).unwrap();
    assert!(
        out.starts_with("x  format    1 file needs formatting\n   B.php  format/operators/not-with-successor-space\n\nx   lint     2 errors in 2 files"),
        "{out}"
    );
}

/// `-q` prints nothing on either stream, with or without `--fix` and under a coding agent too, as PHP-CS-Fixer's
/// `fix -q` (which prints nothing with or without `--dry-run`); the exit code is the one the run has without `-q`.
#[test]
fn quiet_prints_nothing() {
    let toml = "[format]\npreset = \"empty\"\n\n[lint.rules]\n\"complexity/no-negated-boolean-literal\" = \"error\"\n";
    let files = [("pun.toml", toml), ("a.php", "<?php\n$n = !true;\n")];
    let d = project("lint", "quiet", &files);
    let run = |args: &[&str], agent: &str| {
        common::write_files(&d, &files);
        let out = clean_env(&mut Command::new(PUN), &["PATH", "HOME"])
            .current_dir(&d)
            .env("PUN_AGENT", agent)
            .args(["lint", "--no-cache"])
            .args(args)
            .output()
            .unwrap();
        let s = |b: Vec<u8>| String::from_utf8(b).unwrap();
        (s(out.stdout), s(out.stderr), out.status.code())
    };
    for agent in ["0", "1"] {
        for args in [&[][..], &["--fix"]] {
            let (out, _, code) = run(args, agent);
            assert!(!out.is_empty(), "{args:?} {agent}");
            let quiet = run(&[args, &["-q"]].concat(), agent);
            assert_eq!(
                quiet,
                (String::new(), String::new(), code),
                "{args:?} {agent}"
            );
        }
    }
    assert_eq!(read(&d, "a.php"), "<?php\n$n = false;\n");
}

/// `pun lint --generate-baseline` records each problem `@pun-ignore` leaves (native ones by message, token rules once
/// per file), and `[lint] baseline` then hides as many of each as it lists: a new one fails the run, a gone one is a
/// note, and `--fix` still fixes what the baseline hides.
#[test]
fn lint_baseline_hides_the_problems_it_lists() {
    let toml = "[format]\npreset = \"empty\"\n\n[lint.rules]\n\"style/no-alias-function\" = \"error\"\n\"security/no-eval\" = \"error\"\n";
    let code = "<?php\n$n = sizeof($a);\neval('1;');\neval('2;');\neval('3;'); // @pun-ignore lint/security/no-eval\n";
    let d = project(
        "lint",
        "baseline",
        &[("pun.toml", toml), ("src/a.php", code)],
    );
    assert_eq!(plain(&d, &[], &[]).1, Some(1));
    let (out, status) = plain(&d, &["--generate-baseline", "--color=never"], &[]);
    assert_eq!(status, Some(0), "{out}");
    assert!(
        out.contains("Baseline generated with 3 problems in pun-lint-baseline.toml; add `baseline = \"pun-lint-baseline.toml\"` under [lint] in pun.toml to use it"),
        "{out}"
    );
    let baseline = read(&d, "pun-lint-baseline.toml");
    assert!(
        baseline.starts_with("# Pun's lint baseline: problems `pun lint` doesn't report. Regenerate it with\n# `pun lint --generate-baseline`.\n\nvariant = \"loose\"\n"),
        "{baseline}"
    );
    assert!(
        baseline.contains("[[issues]]\nfile = \"src/a.php\"\ncode = \"lint/security/no-eval\"\nmessage = \"Unsafe use of `eval` construct.\"\ncount = 2\n"),
        "{baseline}"
    );
    assert!(
        baseline.contains("[[issues]]\nfile = \"src/a.php\"\ncode = \"lint/style/no-alias-function\"\nmessage = \"the rule's fix changes this file\"\ncount = 1\n"),
        "{baseline}"
    );
    // Not configured yet: the run still fails.
    assert_eq!(plain(&d, &[], &[]).1, Some(1));
    let configured = toml.replace(
        "[lint.rules]\n",
        "[lint]\nbaseline = \"pun-lint-baseline.toml\"\n\n[lint.rules]\n",
    );
    std::fs::write(d.join("pun.toml"), &configured).unwrap();
    let (lines, summary, status) = agent(&d, &["lint"]);
    assert_eq!((lines, status), (vec![], Some(0)), "{summary}");
    assert_eq!(plain(&d, &["--format=json"], &[]).1, Some(0));

    // A third `eval` is one more than the entry counts.
    std::fs::write(
        d.join("src/a.php"),
        code.replace("// @pun-ignore lint/security/no-eval", ""),
    )
    .unwrap();
    let (lines, _, status) = agent(&d, &["lint"]);
    assert_eq!(
        lines,
        ["src/a.php:5:1 lint/security/no-eval Unsafe use of `eval` construct."]
    );
    assert_eq!(status, Some(1));

    // `--fix` applies the fix the baseline hides.
    std::fs::write(d.join("src/a.php"), code).unwrap();
    assert_eq!(plain(&d, &["--fix"], &[]).1, Some(0));
    assert!(read(&d, "src/a.php").contains("$n = count($a);"));

    // The fix and two removed `eval`s leave three entries unmatched.
    std::fs::write(d.join("src/a.php"), "<?php\n$n = count($a);\n").unwrap();
    let (out, status) = plain(&d, &["--color=never"], &[]);
    assert!(
        out.contains("! 3 baseline entries no longer match: run `pun lint --generate-baseline` to drop them\n"),
        "{out}"
    );
    assert_eq!(status, Some(0));
    let (_, summary, _) = agent(&d, &["lint"]);
    assert_eq!(
        summary,
        "pun lint: pass, 3 baseline entries no longer matched"
    );

    // The configured file is the default; a file named on the command line wins.
    let (out, status) = plain(&d, &["--generate-baseline", "--color=never"], &[]);
    assert_eq!(status, Some(0));
    assert!(
        out.contains("Baseline generated with 0 problems in pun-lint-baseline.toml ("),
        "{out}"
    );
    assert_eq!(
        plain(&d, &["--generate-baseline", "other.toml"], &[]).1,
        Some(0)
    );
    assert!(read(&d, "other.toml").contains("variant = \"loose\""));
}

/// A dry run's diff leaves out the token rules the baseline hides, and a file only those rules change has no diff;
/// `--generate-baseline` says that token-rule entries count files, and that it wrote nothing when a file failed.
#[test]
fn lint_baseline_dry_run_diff_and_notes() {
    let toml = "[format]\npreset = \"empty\"\n\n[lint]\nbaseline = \"pun-lint-baseline.toml\"\n\n[lint.rules]\n\"style/no-alias-function\" = \"error\"\n";
    let d = project(
        "lint",
        "baseline-diff",
        &[
            ("pun.toml", toml),
            ("a.php", "<?php\n$n = sizeof($a);\n$s = implode($a, ',');\n"),
            ("b.php", "<?php\n$s = join($b, ',');\n"),
        ],
    );
    let note = "2 entries for PHP-CS-Fixer rules: each counts a file, not its problems, so a new problem in a listed file stays hidden";
    let (out, status) = plain(&d, &["--generate-baseline", "--color=never"], &[]);
    assert_eq!(status, Some(0), "{out}");
    assert!(out.contains(&format!("\n  {note}\n")), "{out}");
    let (_, summary, status) = agent(&d, &["lint", "--generate-baseline"]);
    assert_eq!(status, Some(0));
    assert_eq!(
        summary,
        format!("pun lint: ok, baseline pun-lint-baseline.toml written with 2 problems, {note}")
    );

    // `implode()`'s argument order is the only change left in a.php; b.php's `join()` becomes `implode()` only
    // through the hidden rule, so the separator rule has nothing to change there.
    std::fs::write(
        d.join("pun.toml"),
        format!("{toml}\"style/require-implode-separator\" = \"error\"\n"),
    )
    .unwrap();
    let (out, status) = plain(&d, &["-v", "--color=never"], &[]);
    assert_eq!(status, Some(1), "{out}");
    assert!(out.contains("+$s = implode(',', $a);"), "{out}");
    assert!(!out.contains("count("), "{out}");
    assert!(!out.contains("$b"), "{out}");

    // a.php no longer matches its entry.
    std::fs::write(d.join("a.php"), "<?php\n").unwrap();
    let (_, summary, status) = agent(&d, &["lint"]);
    assert_eq!(
        (summary.as_str(), status),
        (
            "pun lint: pass, 1 baseline entry no longer matched",
            Some(0)
        )
    );

    // A file that fails leaves the baseline as it was.
    let before = read(&d, "pun-lint-baseline.toml");
    std::fs::write(d.join("c.php"), "<?php\n$x = ;\n").unwrap();
    let out = clean_env(&mut Command::new(PUN), &["PATH", "HOME"])
        .current_dir(&d)
        .env("PUN_AGENT", "0")
        .args(["lint", "--generate-baseline", "--color=never"])
        .output()
        .unwrap();
    let err = String::from_utf8(out.stderr).unwrap();
    assert!(
        err.contains("! No baseline written: 1 file failed; fix it and run `pun lint --generate-baseline` again\n"),
        "{err}"
    );
    assert_ne!(out.status.code(), Some(0));
    assert_eq!(read(&d, "pun-lint-baseline.toml"), before);
    let _ = std::fs::remove_dir_all(&d);
}

/// A lint baseline that doesn't parse, or a `[lint] baseline` outside the project, stops the run (exit 2).
#[test]
fn lint_baseline_errors() {
    let configured = "[format]\npreset = \"empty\"\n\n[lint]\nbaseline = \"pun-lint-baseline.toml\"\n\n[lint.rules]\n\"security/no-eval\" = \"error\"\n";
    let d = project(
        "lint",
        "baseline-errors",
        &[
            ("pun.toml", configured),
            ("a.php", "<?php\n"),
            ("pun-lint-baseline.toml", "variant = \"strict\"\n"),
        ],
    );
    let out = clean_env(&mut Command::new(PUN), &["PATH", "HOME"])
        .current_dir(&d)
        .env("PUN_AGENT", "0")
        .args(["lint", "--color=never"])
        .output()
        .unwrap();
    let err = String::from_utf8(out.stderr).unwrap();
    assert_eq!(out.status.code(), Some(2), "{err}");
    assert!(
        err.contains("run `pun lint --generate-baseline` to write it again"),
        "{err}"
    );

    std::fs::write(
        d.join("pun.toml"),
        configured.replace("pun-lint-baseline.toml", "../outside.toml"),
    )
    .unwrap();
    let out = clean_env(&mut Command::new(PUN), &["PATH", "HOME"])
        .current_dir(&d)
        .env("PUN_AGENT", "0")
        .arg("lint")
        .output()
        .unwrap();
    let err = String::from_utf8(out.stderr).unwrap();
    assert_eq!(out.status.code(), Some(2), "{err}");
    assert!(
        err.contains("[lint] baseline: \"../outside.toml\" is outside the project"),
        "{err}"
    );
}

/// `pun check` hides lint problems with the lint baseline and analyse errors with the analyse one, and its note
/// counts the unmatched entries of both.
#[test]
fn check_applies_both_baselines() {
    let toml = "[format]\npreset = \"empty\"\n\n[lint.rules]\n\"security/no-eval\" = \"error\"\n\n[analyse]\npaths = [\"src\"]\n";
    let code = "<?php\neval('1;');\nundefined_fn();\n";
    let d = project(
        "lint",
        "check-baselines",
        &[("pun.toml", toml), ("src/a.php", code)],
    );
    let check = |args: &[&str]| {
        let out = clean_env(&mut Command::new(PUN), &["PATH", "HOME"])
            .current_dir(&d)
            .env("PUN_AGENT", "0")
            .args(args)
            .output()
            .unwrap();
        (String::from_utf8(out.stdout).unwrap(), out.status.code())
    };
    assert_eq!(check(&["check"]).1, Some(1));
    assert_eq!(check(&["lint", "--generate-baseline"]).1, Some(0));
    assert_eq!(check(&["analyse", "--generate-baseline"]).1, Some(0));
    let both = toml
        .replace(
            "[lint.rules]",
            "[lint]\nbaseline = \"pun-lint-baseline.toml\"\n\n[lint.rules]",
        )
        .replace(
            "[analyse]\n",
            "[analyse]\nbaseline = \"pun-baseline.toml\"\n",
        );
    std::fs::write(d.join("pun.toml"), &both).unwrap();
    let (out, status) = check(&["check", "--color=never"]);
    assert_eq!(status, Some(0), "{out}");
    // Only the lint baseline: the analyse error is still reported.
    std::fs::write(
        d.join("pun.toml"),
        both.replace("baseline = \"pun-baseline.toml\"\n", ""),
    )
    .unwrap();
    let (out, status) = check(&["check", "--color=never"]);
    assert!(out.contains("undefined_fn not found"), "{out}");
    assert!(!out.contains("no-eval"), "{out}");
    assert_eq!(status, Some(1));
    std::fs::write(d.join("pun.toml"), &both).unwrap();
    std::fs::write(d.join("src/a.php"), "<?php\n").unwrap();
    let (out, status) = check(&["check", "--color=never"]);
    assert!(
        out.contains("! 2 baseline entries no longer match: run `pun lint --generate-baseline` and `pun analyse --generate-baseline` to drop them\n"),
        "{out}"
    );
    assert_eq!(status, Some(0));
}
