mod common;

use common::{PUN, scratch};

use std::path::Path;
use std::process::{Command, Output};

fn run(cmd: &mut Command) -> Output {
    (cmd.env("PUN_AGENT", "0"))
        .env("PUN_CACHE_DIR", common::test_cache_dir())
        .output()
        .unwrap()
}

#[test]
fn shim_runs_the_workspace_version_with_its_arguments() {
    if Command::new("php").arg("-v").output().is_err() {
        return;
    }
    let dir = scratch("composer", "shim");
    let shim = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../composer/bin/pun");
    std::fs::copy(&shim, dir.join("pun")).unwrap();
    let os = match std::env::consts::OS {
        "macos" => "apple-darwin",
        _ => "unknown-linux-musl",
    };
    let binary = dir.join(format!(
        "pun-{}-{}-{os}",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::ARCH
    ));
    std::fs::copy(PUN, &binary).unwrap();
    std::fs::write(dir.join("Bad.php"), "<?php\n$a = array( 1 );\n").unwrap();

    for args in [
        &["--version"][..],
        &["format", "--test", "Bad.php"],
        &["no-such-command"],
    ] {
        let via_shim = run(Command::new("php")
            .arg(dir.join("pun"))
            .args(args)
            .current_dir(&dir));
        let direct = run(Command::new(&binary).args(args).current_dir(&dir));
        assert_eq!(
            via_shim.status.code(),
            direct.status.code(),
            "exit code of {args:?}"
        );
        assert_eq!(via_shim.stdout, direct.stdout, "stdout of {args:?}");
    }

    // A new file at the same path, not a write into the old one: Linux refuses to write to a binary any process still
    // executes (ETXTBSY), and a just-exited run can hold it for a moment.
    std::fs::remove_file(&binary).unwrap();
    std::fs::write(&binary, b"\x7fELF not a binary").unwrap();
    let broken = run(Command::new("php").arg(dir.join("pun")).current_dir(&dir));
    assert_eq!(broken.status.code(), Some(2));
    assert_eq!(
        String::from_utf8_lossy(&broken.stderr),
        format!(
            "pun: could not start {}; delete it to download it again\n",
            binary.display()
        )
    );
}

/// A local GitHub serving one release asset per request path; every other path gets 404.
fn serve(routes: Vec<(String, Vec<u8>)>) -> String {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let mut head = Vec::new();
            let mut byte = [0];
            while !head.ends_with(b"\r\n\r\n") && stream.read(&mut byte).unwrap_or(0) == 1 {
                head.push(byte[0]);
            }
            let head = String::from_utf8_lossy(&head).into_owned();
            let path = head.split(' ').nth(1).unwrap_or_default();
            let (status, body) = (routes.iter().find(|(p, _)| p == path))
                .map_or(("404 Not Found", &b"Not Found"[..]), |(_, b)| ("200 OK", b));
            let reply = format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(&[reply.as_bytes(), body].concat());
        }
    });
    url
}

/// A first run downloads an archive and a checksum from the same release, so a replaced pair still matches: a
/// signed-in gh must verify the build attestation before the binary is unpacked, as `docs/public/install` does.
#[test]
fn shim_checks_the_build_attestation_before_it_unpacks_a_download() {
    let Ok(php) = which("php") else { return };
    let os = match std::env::consts::OS {
        "macos" => "apple-darwin",
        _ => "unknown-linux-musl",
    };
    let target = format!("{}-{os}", std::env::consts::ARCH);
    let version = env!("CARGO_PKG_VERSION");

    let (server, path) = release(&target, version);
    let name = format!("punphp-{target}.tar.gz");

    let dir = scratch("composer", "attestation");
    // Only the tools the shim runs, so the machine's own gh never answers.
    let tools = dir.join("tools");
    std::fs::create_dir_all(&tools).unwrap();
    for tool in ["tar", "gzip", "rm"] {
        std::os::unix::fs::symlink(which(tool).unwrap(), tools.join(tool)).unwrap();
    }
    let log = dir.join("gh.log");
    let gh = |verify: u8| fake_gh(&dir, &log, &tools, verify);
    let shim = |case: &str, path: &str, env: &[(&str, &str)]| {
        let bin = dir.join(case);
        std::fs::create_dir_all(&bin).unwrap();
        let shim = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../composer/bin/pun");
        std::fs::copy(&shim, bin.join("pun")).unwrap();
        let out = run(Command::new(&php)
            .arg(bin.join("pun"))
            .arg("--version")
            .env("PATH", path)
            .env("PUN_GITHUB_URL", &server)
            .env_remove("PUN_REQUIRE_ATTESTATION")
            .envs(env.iter().copied()));
        let installed = bin.join(format!("pun-{version}-{target}")).exists();
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        (
            out.status.code(),
            String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr,
            installed,
        )
    };
    let downloading = format!("pun: downloading {server}{path}\n");

    // (exit code, stdout, installed) and stderr after the downloading line.
    let ran = (Some(0), "downloaded pun --version\n".to_owned(), true);
    let outcome = |out: (Option<i32>, String, String, bool)| ((out.0, out.1, out.3), out.2);
    let tampered = format!(
        "{downloading}pun: the build attestation of {name} does not verify; the download may have been tampered with\n"
    );
    assert_eq!(
        outcome(shim("tampered", &gh(1), &[])),
        ((Some(2), String::new(), false), tampered)
    );
    // Every argument but the third, the archive's path in the shim's temporary directory.
    let log = std::fs::read_to_string(&log).unwrap();
    let args: Vec<&str> = log.lines().collect();
    assert_eq!(
        [&args[..2], &args[3..]].concat().join(" "),
        format!(
            "attestation verify --repo shipfastlabs/pun --signer-workflow \
             github.com/shipfastlabs/pun/.github/workflows/release.yml --source-ref refs/tags/v{version} \
             --deny-self-hosted-runners"
        )
    );

    let tools = tools.display().to_string();
    let required = format!(
        "{downloading}pun: PUN_REQUIRE_ATTESTATION=1 needs the GitHub CLI (gh), signed in\n"
    );
    let env = [("PUN_REQUIRE_ATTESTATION", "1")];
    assert_eq!(
        outcome(shim("required", &tools, &env)),
        ((Some(2), String::new(), false), required.clone())
    );
    let verified = format!("{downloading}pun: verified the GitHub build attestation\n");
    assert_eq!(
        outcome(shim("verified", &gh(0), &[])),
        (ran.clone(), verified)
    );
    // Without gh the download installs as it always has.
    assert_eq!(
        outcome(shim("no-gh", &tools, &[])),
        (ran.clone(), downloading.clone())
    );

    // A dependency's bin proxy at the binary's name in the shared vendor/bin is a script, not Pun: it is downloaded over.
    let claimed = dir.join("claimed");
    std::fs::create_dir_all(&claimed).unwrap();
    std::fs::write(
        claimed.join(format!("pun-{version}-{target}")),
        "#!/usr/bin/env php\n<?php\necho \"impostor\\n\";\n",
    )
    .unwrap();
    assert_eq!(outcome(shim("claimed", &tools, &[])), (ran, downloading));
    // Composer scripts put vendor/bin first in PATH; a dependency's `gh` there does not vouch for the download.
    let with_gh = dir.join("bin-gh");
    std::fs::create_dir_all(&with_gh).unwrap();
    std::fs::copy(dir.join("gh-0/gh"), with_gh.join("gh")).unwrap();
    let path = format!("{}:{tools}", with_gh.display());
    assert_eq!(
        outcome(shim("bin-gh", &path, &env)),
        ((Some(2), String::new(), false), required)
    );
}

/// A local release `v<version>` holding a `pun` script for `target` and its checksum, as cargo-dist writes it:
/// (server URL, archive path).
fn release(target: &str, version: &str) -> (String, String) {
    use sha2::{Digest, Sha256};
    use std::fmt::Write as _;
    let mut tar = tar::Builder::new(flate2::write::GzEncoder::new(
        vec![],
        flate2::Compression::fast(),
    ));
    let script = b"#!/bin/sh\necho downloaded pun \"$@\"\n";
    let mut header = tar::Header::new_gnu();
    header.set_size(script.len() as u64);
    header.set_mode(0o755);
    (tar.append_data(&mut header, format!("punphp-{target}/pun"), &script[..])).unwrap();
    let archive = tar.into_inner().unwrap().finish().unwrap();
    let digest = Sha256::digest(&archive)
        .iter()
        .fold(String::new(), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        });
    let name = format!("punphp-{target}.tar.gz");
    let path = format!("/shipfastlabs/pun/releases/download/v{version}/{name}");
    let sum = format!("{digest} *{name}\n").into_bytes();
    let server = serve(vec![
        (path.clone(), archive),
        (format!("{path}.sha256"), sum),
    ]);
    (server, path)
}

/// A PATH with a signed-in `gh` that logs its `attestation verify` arguments to `log` and exits `verify`, then `tools`.
fn fake_gh(dir: &Path, log: &Path, tools: &Path, verify: u8) -> String {
    use std::os::unix::fs::PermissionsExt;
    let bin = dir.join(format!("gh-{verify}"));
    std::fs::create_dir_all(&bin).unwrap();
    let script = format!(
        "#!/bin/sh\n[ \"$1\" = auth ] && exit 0\nprintf '%s\\n' \"$@\" > '{}'\nexit {verify}\n",
        log.display()
    );
    std::fs::write(bin.join("gh"), script).unwrap();
    std::fs::set_permissions(bin.join("gh"), PermissionsExt::from_mode(0o755)).unwrap();
    format!("{}:{}", bin.display(), tools.display())
}

fn which(tool: &str) -> Result<std::path::PathBuf, ()> {
    let out = Command::new("sh")
        .args(["-c", &format!("command -v {tool}")])
        .output()
        .map_err(drop)?;
    let path = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    if out.status.success() && !path.is_empty() {
        Ok(path.into())
    } else {
        Err(())
    }
}

/// A project with the scripts every `run-script` test runs against `composer run-script` 2.11.2 (the pinned
/// Composer), and a `tool` in its bin dir.
fn scripts_project(name: &str) -> std::path::PathBuf {
    let dir = scratch("composer", name);
    std::fs::create_dir_all(dir.join("vendor/bin")).unwrap();
    std::fs::write(
        dir.join("composer.json"),
        r#"{"name":"a/b","version":"1.0.0","scripts":{"test":["echo \"dev=$COMPOSER_DEV_MODE\"","tool"],"fail":"@b","b":"exit 4","post-autoload-dump":"echo pad","lönger":"echo l"},"scripts-descriptions":{"test":"Run the tests"},"scripts-aliases":{"lönger":["lo"]}}"#,
    )
    .unwrap();
    let tool = dir.join("vendor/bin/tool");
    std::fs::write(&tool, "#!/bin/sh\necho tool \"$@\"\n").unwrap();
    std::fs::set_permissions(&tool, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    dir
}

/// `pun composer <args>` in `dir`: the exit code, stdout and stderr.
fn composer(dir: &Path, args: &[&str]) -> (Option<i32>, String, String) {
    let out = run(Command::new(PUN)
        .arg("composer")
        .args(args)
        .current_dir(dir));
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn ok(out: &str, err: &str) -> (Option<i32>, String, String) {
    (Some(0), out.to_string(), err.to_string())
}

/// The `> command` lines, the arguments after `--`, `COMPOSER_DEV_MODE`, the bin dir on the PATH, the `--list` table,
/// a script's own command (which echoes nothing) and its alias, and a failed reference's lines and exit code, as
/// Composer prints them.
#[test]
fn run_script_matches_composer() {
    let dir = scripts_project("run-script");
    let pun = |args: &[&str]| composer(&dir, args);
    assert_eq!(
        pun(&["run", "--list"]),
        ok(
            "  test               Run the tests                                      \n  fail               Runs the fail script as defined in composer.json   \n  b                  Runs the b script as defined in composer.json      \n  post-autoload-dump                                                    \n  lönger             Runs the lönger script as defined in composer.json \n",
            "scripts:\n"
        )
    );
    assert_eq!(
        pun(&["run", "test", "--", "--x", "a b"]),
        ok(
            "dev=1 --x a b\ntool --x a b\n",
            "> echo \"dev=$COMPOSER_DEV_MODE\" '--x' 'a b'\n> tool '--x' 'a b'\n"
        )
    );
    assert_eq!(
        pun(&["run-script", "test", "--no-dev"]),
        ok(
            "dev=0\ntool\n",
            "> echo \"dev=$COMPOSER_DEV_MODE\"\n> tool\n"
        )
    );
    assert_eq!(pun(&["test"]), ok("dev=1\ntool\n", ""));
    assert_eq!(pun(&["lo"]), ok("l\n", ""));
    // A script's own command reads options up to the first it doesn't know and drops the words from there on.
    assert_eq!(pun(&["test", "--bogus"]), ok("dev=1\ntool\n", ""));
    assert_eq!(
        pun(&["test", "foo", "--bogus", "bar"]),
        ok("dev=1 foo\ntool foo\n", "")
    );
    assert_eq!(
        pun(&["test", "--no-dev", "foo", "-z"]),
        ok("dev=0 foo\ntool foo\n", "")
    );
    assert_eq!(
        pun(&["test", "--", "--bogus"]),
        ok("dev=1 --bogus\ntool --bogus\n", "")
    );
    assert_eq!(pun(&["lo", "--bogus=1", "x"]), ok("l\n", ""));
    assert_eq!(
        pun(&["run", "post-autoload-dump"]),
        ok("pad\n", "> echo pad\n")
    );
    assert_eq!(
        pun(&["run", "fail"]),
        (
            Some(4),
            String::new(),
            "> exit 4\nScript exit 4 handling the b event returned with error code 4\nScript @b was called via fail\n".into()
        )
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

/// `--timeout` over `process-timeout`, and Composer's lines before Symfony's message (a run error: exit 2).
#[test]
fn run_script_stops_at_the_timeout() {
    let dir = scratch("composer", "timeout");
    std::fs::write(
        dir.join("composer.json"),
        r#"{"name":"a/b","scripts":{"slow":"sleep 3; echo done"}}"#,
    )
    .unwrap();
    let (code, out, err) = composer(&dir, &["run", "--timeout=1", "slow"]);
    assert_eq!((code, out.as_str()), (Some(2), ""), "{err}");
    assert!(
        err.starts_with("> sleep 3; echo done\nThe following exception is caused by a process timeout\nCheck https://getcomposer.org/doc/06-config.md#process-timeout for details\n"),
        "{err}"
    );
    assert!(
        err.contains("The process \"sleep 3; echo done\" exceeded the timeout of 1 seconds."),
        "{err}"
    );
    assert_eq!(
        composer(&dir, &["run", "--timeout=0", "slow"]),
        ok("done\n", "> sleep 3; echo done\n")
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

/// Composer's global `-d` and `-n`, before or after the command, from another directory; and in a terminal, without a
/// script name, the question which script to run.
#[test]
fn run_script_takes_global_options_and_asks() {
    let dir = scripts_project("globals");
    let elsewhere = scratch("composer", "elsewhere");
    let d = dir.to_str().unwrap();
    for args in [
        ["-d", d, "-n", "run", "lönger"],
        ["run", "lönger", "-n", "-d", d],
        ["-d", d, "-n", "lo", "--dev"],
        ["lo", "-n", "--working-dir", d, "--no-dev"],
    ] {
        let (code, out, err) = composer(&elsewhere, &args);
        assert_eq!((code, out.as_str()), (Some(0), "l\n"), "{args:?}: {err}");
    }
    let (out, code) =
        common::pty::pty(&dir, "composer run", "\\033[B\\033[B\\033[B\\033[B\\r", &[]);
    assert_eq!(code, Some(0), "{out}");
    assert!(out.contains("Script to run"), "{out}");
    assert!(out.ends_with("\nl\n"), "{out:?}");
    std::fs::remove_dir_all(&dir).unwrap();
    std::fs::remove_dir_all(&elsewhere).unwrap();
}

/// Composer 2.11.2 skips a script named after one of its commands or aliases (`info` is `show`'s) as a command of its
/// own, and a command wins over a script's alias (`status`); `run-script` still runs it, and `--list` shows the
/// command's description beside it.
#[test]
fn scripts_named_after_composer_commands() {
    let dir = scratch("composer", "command-scripts");
    std::fs::write(
        dir.join("composer.json"),
        r#"{"name":"a/b","version":"1.0.0","scripts":{"info":"echo INFO","st":"echo ST","foo":"echo FOO"},"scripts-aliases":{"foo":["status","bar"]}}"#,
    )
    .unwrap();
    assert_eq!(
        composer(&dir, &["run", "--list"]),
        ok(
            "  info Shows information about packages                \n  st   Runs the st script as defined in composer.json  \n  foo  Runs the foo script as defined in composer.json \n",
            "scripts:\n"
        )
    );
    assert_eq!(
        composer(&dir, &["run", "info"]),
        ok("INFO\n", "> echo INFO\n")
    );
    assert_eq!(composer(&dir, &["st"]), ok("ST\n", ""));
    assert_eq!(composer(&dir, &["bar"]), ok("FOO\n", ""));
    for name in ["info", "status"] {
        let (code, out, err) = composer(&dir, &[name]);
        assert_eq!((code, out.as_str()), (Some(2), ""), "{name}: {err}");
        assert!(
            err.contains(&format!("`pun composer {name}` is not built yet")),
            "{name}: {err}"
        );
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

/// Composer's run errors (exit 1 there, 2 in Pun); a script named after an event is no command of its own; and no
/// list at all, not even `scripts:`, for a project without scripts.
#[test]
fn run_script_errors_like_composer() {
    let dir = scripts_project("errors");
    for (args, message) in [
        (
            &["run", "nope"][..],
            "Script \"nope\" is not defined in this package",
        ),
        (&["run"], "Missing required argument \"script\""),
        (&["run", "-n"], "Missing required argument \"script\""),
        (
            &["run", "--timeout=x", "test"],
            "Timeout value must be numeric and positive if defined, or 0 for forever",
        ),
        (
            &["post-autoload-dump"],
            "`pun composer post-autoload-dump` is not built yet",
        ),
    ] {
        let (code, out, err) = composer(&dir, args);
        assert_eq!((code, out.as_str()), (Some(2), ""), "{args:?}: {err}");
        assert!(err.contains(message), "{args:?}: {err}");
    }
    std::fs::write(dir.join("composer.json"), r#"{"name":"a/b"}"#).unwrap();
    assert_eq!(composer(&dir, &["run", "--list"]), ok("", ""));
    std::fs::remove_dir_all(&dir).unwrap();
}
