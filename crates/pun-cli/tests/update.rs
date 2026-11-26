mod common;

use common::pty::{pun, pun_with};
use common::read;
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    let d = common::scratch("update", name);
    let source =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../pun-packages/tests/fixtures/project/.");
    let copied = std::process::Command::new("cp")
        .arg("-R")
        .arg(source)
        .arg(&d)
        .status()
        .unwrap();
    assert!(copied.success());
    d
}

fn expected(name: &str) -> String {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/update")
            .join(name),
    )
    .unwrap()
}

#[test]
fn update_writes_composers_lock() {
    let d = fixture("update");
    let composer_lock = read(&d, "composer.lock");
    std::fs::remove_file(d.join("composer.lock")).unwrap();
    let (out, err, code) = pun(&d, &["update", "--dry-run"]);
    assert_eq!(code, Some(0), "{out}{err}");
    assert!(err.contains(" + acme/lib 1.2.0\n"), "{err}");
    assert!(!d.join("composer.lock").exists());

    let (out, err, code) = pun(&d, &["update", "--no-install"]);
    assert_eq!(code, Some(0), "{out}{err}");
    assert_eq!(read(&d, "composer.lock"), composer_lock);
    assert!(!d.join("vendor").exists());

    let (out, err, code) = pun(&d, &["update", "acme/lib", "acme/nope"]);
    assert_eq!(code, Some(0), "{out}{err}");
    assert!(
        err.contains(" ! acme/nope is not in the lock, so there is nothing of it to update\n")
            && err.contains("Nothing to change: the lock is up to date\n")
            && err.contains(" + packages: 2 installed\n"),
        "{err}"
    );
    assert_eq!(read(&d, "composer.lock"), composer_lock);
    assert!(d.join("vendor/composer/installed.php").exists());

    std::fs::remove_file(d.join("composer.json")).unwrap();
    let (_, err, code) = pun(&d, &["install"]);
    assert_eq!(code, Some(2));
    assert!(err.contains("no composer.json in "), "{err}");
}

#[test]
fn remove_matches_composer() {
    let d = fixture("remove");
    let (out, err, code) = pun(&d, &["install"]);
    assert_eq!(code, Some(0), "{out}{err}");
    assert!(d.join("vendor/acme/tools").exists());
    let (out, err, code) = pun(&d, &["remove", "acme/tools", "acme/absent"]);
    assert_eq!(code, Some(0), "{out}{err}");
    assert!(
        err.contains(" ! acme/absent is not required, so there is nothing to remove\n")
            && err.contains(" - acme/tools 0.3.1\n"),
        "{err}"
    );
    assert_eq!(
        read(&d, "composer.json"),
        expected("remove-tools.composer.json")
    );
    assert_eq!(
        read(&d, "composer.lock"),
        expected("remove-tools.composer.lock")
    );
    assert!(!d.join("vendor/acme/tools").exists());
    let (_, err, code) = pun(&d, &["remove", "acme/absent"]);
    assert_eq!(code, Some(0), "{err}");
}

/// Removing a package another one requires: Composer 2.11.2 writes the lock (acme/lib moves to packages-dev, as
/// acme/tools needs it), keeps the edit of composer.json, and exits 2 because acme/lib is still installed.
#[test]
fn removing_a_required_package_fails_as_composer() {
    let d = fixture("remove-lib");
    let (out, err, code) = pun(&d, &["install"]);
    assert_eq!(code, Some(0), "{out}{err}");
    let (out, err, code) = pun(&d, &["remove", "acme/lib"]);
    assert_eq!(code, Some(2), "{out}{err}");
    assert!(
        err.contains("acme/lib is still installed: another package may require it"),
        "{err}"
    );
    assert_eq!(
        read(&d, "composer.json"),
        expected("remove-lib.composer.json")
    );
    assert_eq!(
        read(&d, "composer.lock"),
        expected("remove-lib.composer.lock")
    );
    assert!(d.join("vendor/acme/lib").exists());
    // Again: acme/lib is no longer required, but Composer still updates and fails on it; with --no-update it exits 0.
    let (out, err, code) = pun(&d, &["remove", "acme/lib"]);
    assert_eq!(code, Some(2), "{out}{err}");
    assert_eq!(
        read(&d, "composer.lock"),
        expected("remove-lib.composer.lock")
    );
    let (out, err, code) = pun(&d, &["remove", "acme/lib", "--no-update"]);
    assert_eq!(code, Some(0), "{out}{err}");
}

#[test]
fn require_edits_composer_json() {
    let d = fixture("require");
    let (json, lock) = (read(&d, "composer.json"), read(&d, "composer.lock"));

    let (out, err, code) = pun(&d, &["remove", "acme/lib", "--no-install"]);
    assert_eq!(code, Some(0), "{out}{err}");
    assert_eq!(
        read(&d, "composer.json"),
        json.replace(",\n        \"acme/lib\": \"^1.2\"", "")
    );
    let (out, err, code) = pun(&d, &["require", "acme/lib"]);
    assert_eq!(code, Some(0), "{out}{err}");
    assert!(err.contains("Using version ^1.2 for acme/lib\n"), "{err}");
    assert_eq!(read(&d, "composer.json"), json);
    assert_eq!(read(&d, "composer.lock"), lock);
    assert!(d.join("vendor/acme/lib").exists());

    let (out, err, code) = pun(&d, &["require", "acme/lib:^9.0"]);
    assert_eq!(code, Some(2), "{out}{err}");
    assert!(
        err.contains("x The requirements can't be resolved:\n\n  Problem 1\n    - your project requires acme/lib ^9.0, but the only version found is 1.2.0\n"),
        "{err}"
    );
    assert_eq!(
        (read(&d, "composer.json"), read(&d, "composer.lock")),
        (json.clone(), lock)
    );

    let (_, err, code) = pun(&d, &["require", "--no-update", "--dev", "acme/new:^1.0"]);
    assert_eq!(code, Some(0), "{err}");
    assert_eq!(
        read(&d, "composer.json"),
        json.replace(
            "\"acme/tools\": \"^0.3\"\n",
            "\"acme/tools\": \"^0.3\",\n        \"acme/new\": \"^1.0\"\n"
        )
    );
    for bad in ["nope", "acme/app"] {
        let (_, err, code) = pun(&d, &["require", bad]);
        assert_eq!(code, Some(2), "{err}");
    }
}

#[test]
fn workspace_members_share_one_lock() {
    let d = common::scratch("update", "workspace");
    common::write_files(
        &d,
        &[
            (
                "ext/log/composer.json",
                r#"{"name": "ext/log", "version": "2.3.0"}"#,
            ),
            ("repo/pun.toml", "[workspace]\nmembers = [\"packages/*\"]\n"),
            (
                "repo/composer.json",
                r#"{"repositories": [{"type": "path", "url": "../ext/log"}, {"packagist.org": false}]}"#,
            ),
            (
                "repo/packages/core/composer.json",
                "{\n    \"name\": \"acme/core\"\n}\n",
            ),
            (
                "repo/packages/billing/composer.json",
                "{\n    \"name\": \"acme/billing\",\n    \"require\": {\n        \"acme/core\": \"*\"\n    }\n}\n",
            ),
        ],
    );
    let (root, billing) = (d.join("repo"), d.join("repo/packages/billing"));
    let (out, err, code) = pun(&billing, &["install"]);
    assert_eq!(code, Some(0), "{out}{err}");
    for (link, target) in [("billing", "packages/billing"), ("core", "packages/core")] {
        let path = root.join("vendor/acme").join(link);
        assert_eq!(
            std::fs::read_link(&path).unwrap(),
            Path::new("../..").join(target),
            "{link}"
        );
    }
    assert!(!billing.join("vendor").exists() && !billing.join("composer.lock").exists());

    let (out, err, code) = pun(&billing, &["require", "ext/log"]);
    assert_eq!(code, Some(0), "{out}{err}");
    assert_eq!(
        read(&billing, "composer.json"),
        "{\n    \"name\": \"acme/billing\",\n    \"require\": {\n        \"acme/core\": \"*\",\n        \"ext/log\": \"^2.3\"\n    }\n}\n"
    );
    let lock = read(&root, "composer.lock");
    assert!(lock.contains("\"ext/log\": \"^2.3\""), "{lock}");
    assert!(root.join("vendor/ext/log").exists());

    let core = read(&root, "packages/core/composer.json");
    let (out, err, code) = pun(&root, &["require", "-p", "acme/core", "ext/log:^9"]);
    assert_eq!(code, Some(2), "{out}{err}");
    assert!(
        err.contains("ext/log ^9, but the only version found is 2.3.0"),
        "{err}"
    );
    assert_eq!(
        (
            read(&root, "packages/core/composer.json"),
            read(&root, "composer.lock")
        ),
        (core, lock)
    );

    let (_, err, code) = pun(&root, &["require", "-p", "acme/nope", "ext/log"]);
    assert_eq!(code, Some(2));
    assert!(
        err.contains(
            "acme/nope is not a workspace member; the members are acme/billing, acme/core"
        ),
        "{err}"
    );
}

/// An allowed Composer plugin runs through the plugin host, in the order Composer 2.11.2 logged on the same project
/// (tests/fixtures/plugins): listeners at priority 0 and up before the project's scripts and negative ones after,
/// `init` only for a plugin installed before the command started, `--no-scripts` keeping the plugins, a plugin
/// update activating the new version mid-command, and the plugin `allow-plugins` denies never loading.
#[test]
fn composer_plugins_run_as_composer_runs_them() {
    let d = common::scratch("update", "plugins");
    let copied = std::process::Command::new("cp")
        .arg("-R")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../pun-packages/tests/fixtures/plugins/."))
        .arg(&d)
        .status()
        .unwrap();
    assert!(copied.success());
    let cache = Path::new(env!("CARGO_TARGET_TMPDIR")).join("composer-cache");
    let home = d.join(".composer");
    let env = [
        ("COMPOSER_CACHE_DIR", cache.to_str().unwrap()),
        ("COMPOSER_HOME", home.to_str().unwrap()),
    ];
    let log = |args: &[&str]| {
        let (out, err, code) = common::pty::pun_with(&d, args, &env);
        assert_eq!(code, Some(0), "{out}{err}");
        let log = std::fs::read_to_string(d.join("events.log")).unwrap_or_default();
        let _ = std::fs::remove_file(d.join("events.log"));
        log
    };
    let installed = "pre-autoload-dump plugin 0 1\npost-autoload-dump plugin 10 1\npost-autoload-dump plugin 0 1\npost-autoload-dump script 1\npost-autoload-dump plugin -10 1\n";
    assert_eq!(
        log(&["install"]),
        format!(
            "activate acme/app\n{installed}post-install-cmd plugin 0 1\npost-install-cmd script 1\n"
        )
    );
    let started = "activate acme/app\ninit plugin 0 \n";
    assert_eq!(
        log(&["install"]),
        format!(
            "{started}pre-install-cmd plugin 0 1\n{installed}post-install-cmd plugin 0 1\npost-install-cmd script 1\n"
        )
    );
    assert_eq!(
        log(&["install", "--no-scripts"]),
        format!(
            "{started}pre-install-cmd plugin 0 1\n{}post-install-cmd plugin 0 1\n",
            installed.replace("post-autoload-dump script 1\n", "")
        )
    );
    assert_eq!(
        log(&["install", "--no-plugins"]),
        "post-autoload-dump script 1\npost-install-cmd script 1\n"
    );
    std::fs::write(d.join("fail"), "post-install-cmd echo").unwrap();
    let (out, err, code) = common::pty::pun_with(&d, &["install"], &env);
    assert_eq!(code, Some(0), "{out}{err}");
    assert!(out.contains("the recorder says hello"), "{out}");
    std::fs::remove_file(d.join("fail")).unwrap();
    std::fs::remove_file(d.join("events.log")).unwrap();
    let manifest = d.join("packages/recorder/composer.json");
    let text = std::fs::read_to_string(&manifest).unwrap();
    std::fs::write(&manifest, text.replace("\"1.0.0\"", "\"1.1.0\"")).unwrap();
    assert_eq!(
        log(&["update"]),
        format!(
            "{started}pre-update-cmd plugin 0 1\nactivate acme/app\n{installed}post-update-cmd plugin 0 1\n"
        )
    );
}

/// Copies `tests/fixtures/update/<name>` to `file` in `d`.
fn put(d: &Path, name: &str, file: &str) {
    std::fs::write(d.join(file), expected(name)).unwrap();
}

/// The lock changes `pun update` prints, from Composer 2.11.2's `Upgrading`/`Downgrading` lines on the same project
/// (tests/fixtures/update/flags.composer.json): `-w` frees a named package's dependencies but not the root's own
/// requirements, `-W` frees those too, and `--prefer-lowest` writes the lock Composer writes.
#[test]
fn update_flags_match_composer() {
    let d = common::scratch("update", "flags");
    put(&d, "flags.composer.json", "composer.json");
    let (out, err, code) = pun(&d, &["update", "--no-install"]);
    assert_eq!(code, Some(0), "{out}{err}");
    let (out, err, code) = pun(&d, &["update", "--prefer-lowest", "--no-install"]);
    assert_eq!(code, Some(0), "{out}{err}");
    assert!(
        err.contains("Lock changes:\n v acme/a 1.1.0 -> 1.0.0\n v acme/b 1.1.0 -> 1.0.0\n v acme/c 1.1.0 -> 1.0.0\n v acme/d 1.1.0 -> 1.0.0\nWrote composer.lock"),
        "{err}"
    );
    assert_eq!(
        read(&d, "composer.lock"),
        expected("flags-lowest.composer.lock")
    );

    let nothing = "Nothing to change: the lock is up to date\n";
    for (args, changes) in [
        (["acme/a", "--dry-run"].as_slice(), nothing),
        (
            &["acme/a", "-w", "--dry-run"],
            "Lock changes:\n ^ acme/a 1.0.0 -> 1.1.0\n ^ acme/c 1.0.0 -> 1.1.0\n",
        ),
        (&["acme/d", "--with-dependencies", "--dry-run"], nothing),
        (
            &["acme/d", "-W", "--dry-run"],
            "Lock changes:\n ^ acme/b 1.0.0 -> 1.1.0\n ^ acme/d 1.0.0 -> 1.1.0\n",
        ),
    ] {
        let (out, err, code) = pun(&d, &[&["update"], args].concat());
        assert_eq!((err.as_str(), code), (changes, Some(0)), "{args:?} {out}");
    }
    assert_eq!(
        read(&d, "composer.lock"),
        expected("flags-lowest.composer.lock")
    );

    let (_, err, code) = pun(&d, &["remove", "--no-update", "acme/d"]);
    assert_eq!((err.as_str(), code), ("Updated composer.json\n", Some(0)));
    assert_eq!(
        read(&d, "composer.json"),
        expected("flags-remove-d.composer.json")
    );
    put(&d, "flags.composer.json", "composer.json");
    let (out, err, code) = pun(&d, &["require", "acme/e:^1.0", "--no-install"]);
    assert_eq!(code, Some(0), "{out}{err}");
    assert!(
        err.starts_with(" ! acme/e moves from require-dev to require\n"),
        "{err}"
    );
    assert_eq!(
        read(&d, "composer.json"),
        expected("flags-move-e.composer.json")
    );
    assert_eq!(
        read(&d, "composer.lock"),
        expected("flags-move-e.composer.lock")
    );

    let (_, err, code) = pun(&d, &["require", "acme/e:^^1"]);
    assert_eq!(code, Some(2), "{err}");
    assert!(
        err.contains("acme/e: ^^1 is not a version constraint"),
        "{err}"
    );
    let (_, err, code) = pun(&d, &["require", "-p", "acme/core", "acme/e"]);
    assert_eq!(code, Some(2), "{err}");
    assert!(
        err.contains("-p names a workspace member, and this project has no [workspace]"),
        "{err}"
    );
    assert_eq!(
        read(&d, "composer.json"),
        expected("flags-move-e.composer.json")
    );

    // A platform package has no version to pick: Composer 2.11.2 writes `*`.
    put(&d, "flags.composer.json", "composer.json");
    put(&d, "flags-lowest.composer.lock", "composer.lock");
    let (out, err, code) = pun(&d, &["require", "php", "--no-install"]);
    assert_eq!(code, Some(0), "{out}{err}");
    assert!(!err.contains("Using version"), "{err}");
    assert_eq!(
        read(&d, "composer.json"),
        expected("flags.composer.json").replace(
            "\"acme/d\": \"^1.0\"\n",
            "\"acme/d\": \"^1.0\",\n        \"php\": \"*\"\n"
        )
    );
}

/// `require` without a version on branches, as Composer 2.11.2 writes it (tests/fixtures/update/branches-*): the
/// branch name itself, with a warning for a feature branch; `remove` without a lock resolves everything again; a
/// `remove` that leaves requirements that don't resolve keeps composer.json; and an install that fails puts back
/// composer.json and composer.lock, deleting them when `require` created them.
#[test]
fn require_and_remove_on_branches() {
    let d = common::scratch("update", "branches");
    put(&d, "branches.composer.json", "composer.json");
    let (out, err, code) = pun(&d, &["require", "acme/f", "acme/m", "--no-install"]);
    assert_eq!(code, Some(0), "{out}{err}");
    assert!(
        err.contains("Using version dev-feature for acme/f\n ! dev-feature looks like a feature branch, which may not keep working for long\nUsing version dev-main for acme/m\nUpdated composer.json\n"),
        "{err}"
    );
    assert_eq!(
        read(&d, "composer.json"),
        expected("branches-require.composer.json")
    );
    assert_eq!(
        read(&d, "composer.lock"),
        expected("branches-require.composer.lock")
    );

    std::fs::remove_file(d.join("composer.lock")).unwrap();
    let (out, err, code) = pun(&d, &["remove", "acme/m", "--no-install"]);
    assert_eq!(code, Some(0), "{out}{err}");
    assert_eq!(
        read(&d, "composer.json"),
        expected("branches-remove.composer.json")
    );
    assert_eq!(
        read(&d, "composer.lock"),
        expected("branches-remove.composer.lock")
    );

    let (json, lock) = (read(&d, "composer.json"), read(&d, "composer.lock"));
    let (_, err, code) = pun(&d, &["require", "acme/h:^1.0"]);
    assert_eq!(code, Some(2), "{err}");
    assert!(
        err.contains("Your configuration does not allow connections to http://127.0.0.1:9/h.zip")
            && err.contains(
                "The install failed, so composer.json and composer.lock are back as they were\n"
            ),
        "{err}"
    );
    assert_eq!(
        (read(&d, "composer.json"), read(&d, "composer.lock")),
        (json.clone(), lock)
    );

    let unresolvable = json.replace(
        "\"acme/f\": \"dev-feature\"",
        "\"acme/f\": \"^9.0\",\n        \"acme/m\": \"dev-main\"",
    );
    std::fs::write(d.join("composer.json"), &unresolvable).unwrap();
    let (_, err, code) = pun(&d, &["remove", "acme/m"]);
    assert_eq!(code, Some(2), "{err}");
    assert!(
        err.contains("x The requirements can't be resolved:"),
        "{err}"
    );
    assert_eq!(read(&d, "composer.json"), unresolvable);

    let fresh = common::scratch("update", "fresh");
    let home = fresh.join(".composer");
    common::write_files(
        &home,
        &[(
            "config.json",
            r#"{"repositories": [{"packagist.org": false}, {"type": "package", "package": {"name": "acme/h", "version": "1.0.0", "dist": {"type": "zip", "url": "http://127.0.0.1:9/h.zip"}}}]}"#,
        )],
    );
    let project = fresh.join("app");
    std::fs::create_dir(&project).unwrap();
    let env = [("COMPOSER_HOME", home.to_str().unwrap())];
    let (_, err, code) = common::pty::pun_with(&project, &["require", "acme/h:^1.0"], &env);
    assert_eq!(code, Some(2), "{err}");
    assert!(!project.join("composer.json").exists() && !project.join("composer.lock").exists());
    let (_, err, code) =
        common::pty::pun_with(&project, &["require", "--no-update", "acme/x:^1.0"], &env);
    assert_eq!(code, Some(0), "{err}");
    assert_eq!(
        read(&project, "composer.json"),
        "{\n    \"require\": {\n        \"acme/x\": \"^1.0\"\n    }\n}\n"
    );
}

/// The global config.json's policy applies under the project's, as Composer merges the two, and a config.json that
/// isn't JSON stops the run before any repository is asked, as Composer's `JsonFile::read`.
#[test]
fn update_reads_the_global_config_as_composer() {
    let d = common::scratch("update", "global-config");
    let home = d.join(".composer");
    let repositories = r#""repositories": [{"packagist.org": false}, {"type": "package", "package": [{"name": "acme/c", "version": "1.0.0", "time": "2020-01-01"}, {"name": "acme/c", "version": "1.1.0", "time": "2999-01-01"}]}]"#;
    common::write_files(
        &d,
        &[("app/composer.json", r#"{"require": {"acme/c": "^1.0"}}"#)],
    );
    let project = d.join("app");
    let env = [("COMPOSER_HOME", home.to_str().unwrap())];
    for (config, want) in [
        ("", "1.1.0"),
        (
            r#""config": {"policy": {"cooldown": {"period": "7 days"}}},"#,
            "1.0.0",
        ),
    ] {
        common::write_files(
            &home,
            &[("config.json", &format!("{{{config}{repositories}}}"))],
        );
        let (_, err, code) = common::pty::pun_with(&project, &["update", "--dry-run"], &env);
        assert_eq!(
            (err.as_str(), code),
            (
                format!("Lock changes:\n + acme/c {want}\n").as_str(),
                Some(0)
            ),
            "{config}"
        );
    }
    std::fs::write(home.join("config.json"), "{").unwrap();
    let (_, err, code) = common::pty::pun_with(&project, &["update", "--dry-run"], &env);
    assert_eq!(code, Some(2), "{err}");
    assert!(
        err.contains(&format!(
            "{} does not contain valid JSON",
            home.join("config.json").display()
        )),
        "{err}"
    );
}

/// The autoloader options write what Composer 2.11.2 writes for the fixture project without its
/// `optimize-autoloader` config: `-o` adds the PSR-4 classes to the class map, `-a` also makes it authoritative,
/// and the APCu options set the prefix, a random 20-character one without `--apcu-autoloader-prefix`.
#[test]
fn install_options_match_composer() {
    let d = fixture("options");
    let manifest = read(&d, "composer.json");
    let plain = manifest.replace(
        ",\n    \"config\": {\n        \"optimize-autoloader\": true\n    }",
        "",
    );
    assert_ne!(plain, manifest);
    std::fs::write(d.join("composer.json"), plain).unwrap();
    let kernel = "    'App\\\\Kernel' => $baseDir . '/src/Kernel.php',\n";
    let real = |d: &Path| read(d, "vendor/composer/autoload_real.php");
    for (flags, classmap, line) in [
        ([].as_slice(), false, None),
        (&["-o"], true, None),
        (
            &["--classmap-authoritative"],
            true,
            Some("        $loader->setClassMapAuthoritative(true);\n"),
        ),
        (
            &["--apcu-autoloader-prefix=abc"],
            false,
            Some("        $loader->setApcuPrefix('abc');\n"),
        ),
    ] {
        let (out, err, code) = pun(&d, &[&["install"], flags].concat());
        assert_eq!(code, Some(0), "{flags:?} {out}{err}");
        let map = read(&d, "vendor/composer/autoload_classmap.php");
        assert_eq!(map.contains(kernel), classmap, "{flags:?} {map}");
        for other in ["setClassMapAuthoritative", "setApcuPrefix"] {
            let want = line.is_some_and(|l| l.contains(other));
            assert_eq!(real(&d).contains(other), want, "{flags:?} {}", real(&d));
        }
        assert!(
            line.is_none_or(|l| real(&d).contains(l)),
            "{flags:?} {}",
            real(&d)
        );
    }
    let (_, err, code) = pun(&d, &["install", "--apcu-autoloader"]);
    assert_eq!(code, Some(0), "{err}");
    let prefix = real(&d)
        .split("$loader->setApcuPrefix('")
        .nth(1)
        .and_then(|r| r.split_once("');"))
        .map_or_else(String::new, |(p, _)| p.to_string());
    assert!(
        prefix.len() == 20 && prefix.bytes().all(|b| b.is_ascii_hexdigit()) && prefix != "abc",
        "{}",
        real(&d)
    );

    let fresh = fixture("no-autoloader");
    let (_, err, code) = pun(&fresh, &["install", "--no-autoloader"]);
    assert_eq!(code, Some(0), "{err}");
    assert!(!err.contains("autoloader:"), "{err}");
    let mut written: Vec<String> = std::fs::read_dir(fresh.join("vendor/composer"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    written.sort();
    assert_eq!(
        written,
        ["InstalledVersions.php", "installed.json", "installed.php"]
    );
    assert!(!fresh.join("vendor/autoload.php").exists());

    let (_, err, code) = pun(&d, &["install", "-d", "nowhere"]);
    assert_eq!(code, Some(2), "{err}");
    assert!(err.contains("can't open nowhere"), "{err}");
}

/// What `pun install` reports besides the packages: a lock older than composer.json, Composer's warning for a bin
/// the package lacks, the archives Composer's cache already held, and an empty lock.
#[test]
fn install_reports_like_composer() {
    let d = fixture("reports");
    let manifest = read(&d, "composer.json");
    // A changed constraint the locked 0.3.1 still meets: a stale lock, not a missing requirement (exit 4).
    std::fs::write(d.join("composer.json"), manifest.replace("^0.3", "^0.3.1")).unwrap();
    let lock = read(&d, "composer.lock");
    let missing = lock.replace(
        "\"bin/lib\"\n",
        "\"bin/lib\",\n                \"bin/missing\"\n",
    );
    assert_ne!(missing, lock);
    std::fs::write(d.join("composer.lock"), missing).unwrap();
    let (out, err, code) = pun(&d, &["install"]);
    assert_eq!(code, Some(0), "{out}{err}");
    assert!(
        err.contains(" ! composer.lock is out of date with composer.json")
            && err.contains(" ! Skipped installation of bin bin/missing for package acme/lib: file not found in package\n"),
        "{err}"
    );

    // Composer's cache key for an archive: the package's name, then the sha1 of its URL (worked out by hand).
    let d = common::scratch("update", "cache");
    let cache = d.join("cache");
    let archive = cache.join("files/acme/z/3b9fe2fa9f3740a78809b3c74fe51b6b5e249bdc.tar");
    std::fs::create_dir_all(archive.parent().unwrap()).unwrap();
    let mut tar = tar::Builder::new(vec![]);
    let mut header = tar::Header::new_gnu();
    header.set_size(3);
    header.set_mode(0o644);
    header.set_cksum();
    tar.append_data(&mut header, "z/README", &b"hi\n"[..])
        .unwrap();
    std::fs::write(&archive, tar.into_inner().unwrap()).unwrap();
    common::write_files(
        &d,
        &[(
            "composer.json",
            r#"{"repositories": [{"packagist.org": false}, {"type": "package", "package": {"name": "acme/z", "version": "1.0.0", "dist": {"type": "tar", "url": "https://example.invalid/acme-z-1.0.0.tar"}}}], "require": {"acme/z": "^1.0"}}"#,
        )],
    );
    let home = d.join("home");
    let env = [
        ("COMPOSER_CACHE_DIR", cache.to_str().unwrap()),
        ("COMPOSER_HOME", home.to_str().unwrap()),
    ];
    let (out, err, code) = common::pty::pun_with(&d, &["update"], &env);
    assert_eq!(code, Some(0), "{out}{err}");
    assert!(
        err.contains(" + packages: 1 installed (1 from the cache)\n"),
        "{err}"
    );
    assert_eq!(read(&d, "vendor/acme/z/README"), "hi\n");

    std::fs::write(
        d.join("composer.json"),
        r#"{"repositories": [{"packagist.org": false}]}"#,
    )
    .unwrap();
    let (out, err, code) = common::pty::pun_with(&d, &["update"], &env);
    assert_eq!(code, Some(0), "{out}{err}");
    assert!(err.contains(" + packages: 1 removed\n"), "{err}");
    let (out, err, code) = common::pty::pun_with(&d, &["install"], &env);
    assert_eq!(code, Some(0), "{out}{err}");
    assert!(err.contains(" + packages: none\n"), "{err}");
}

/// The workspace rules of docs/content/docs/guides/workspaces.mdx: members from globs (a glob that matches nothing,
/// files, and directories without a composer.json add none, and a member two globs match counts once), the root's
/// repositories in either of Composer's forms, its own constraint on a member kept, `require` at the root editing
/// the root's composer.json, and the errors for a bad `[workspace]`, a nameless member, and two members of one
/// name.
#[test]
fn workspaces_follow_the_guide() {
    let d = common::scratch("update", "guide");
    common::write_files(
        &d,
        &[
            (
                "ext/log/composer.json",
                r#"{"name": "ext/log", "version": "2.3.0"}"#,
            ),
            (
                "repo/pun.toml",
                "[workspace]\nmembers = [\"packages/*\", \"apps/*\", \"packages/core\"]\n",
            ),
            (
                "repo/composer.json",
                "{\n    \"repositories\": {\n        \"log\": {\n            \"type\": \"path\",\n            \"url\": \"../ext/log\"\n        },\n        \"packagist.org\": false\n    }\n}\n",
            ),
            (
                "repo/packages/core/composer.json",
                r#"{"name": "acme/core", "version": "1.0.0", "require": {"ext/log": "^2.3"}}"#,
            ),
            ("repo/packages/README.md", "# Packages\n"),
            ("repo/packages/docs/index.md", "# Docs\n"),
        ],
    );
    let root = d.join("repo");
    let (out, err, code) = pun(&root, &["install"]);
    assert_eq!(code, Some(0), "{out}{err}");
    assert!(err.contains(" + packages: 2 installed\n"), "{err}");
    assert!(root.join("vendor/acme/core").is_symlink() && root.join("vendor/ext/log").exists());

    let (out, err, code) = pun(&root, &["require", "ext/log:^2.0", "--no-install"]);
    assert_eq!(code, Some(0), "{out}{err}");
    assert_eq!(
        read(&root, "composer.json"),
        "{\n    \"repositories\": {\n        \"log\": {\n            \"type\": \"path\",\n            \"url\": \"../ext/log\"\n        },\n        \"packagist.org\": false\n    },\n    \"require\": {\n        \"ext/log\": \"^2.0\"\n    }\n}\n"
    );
    let lock = read(&root, "composer.lock");
    assert!(lock.contains("\"name\": \"acme/core\""), "{lock}");

    let core = read(&root, "packages/core/composer.json");
    let args = ["require", "-p", "acme/core", "ext/log:^2.0"];
    let (_, err, code) = common::pty::pun_with(&root, &args, &[("PHP_BINARY", "/nowhere/php")]);
    assert_eq!(code, Some(2), "{err}");
    assert!(
        err.contains("can't run /nowhere/php to read the platform"),
        "{err}"
    );
    assert_eq!(read(&root, "packages/core/composer.json"), core);

    let manifest = read(&root, "composer.json");
    let pinned = manifest.replace("\"ext/log\": \"^2.0\"", "\"acme/core\": \"^2.0\"");
    std::fs::write(root.join("composer.json"), pinned).unwrap();
    let (_, err, code) = pun(&root, &["update", "--dry-run"]);
    assert_eq!(code, Some(2), "{err}");
    assert!(err.contains("acme/core ^2.0"), "{err}");
    std::fs::write(root.join("composer.json"), manifest).unwrap();

    for (files, error) in [
        (
            [("pun.toml", "[workspace]\nmembers = \"packages/*\"\n")].as_slice(),
            "[workspace] takes one key, members, a list of directories or globs such as \"packages/*\"".to_string(),
        ),
        (
            &[("pun.toml", "[workspace]\nmembers = [\"packages/*\"]\nexclude = []\n")],
            "[workspace] takes one key, members".to_string(),
        ),
        (
            &[("packages/docs/composer.json", "{}")],
            format!(
                "the workspace member in {} has no name: set `name` in its composer.json",
                root.join("packages/docs").display()
            ),
        ),
        (
            &[("packages/twin/composer.json", r#"{"name": "Acme/Core"}"#)],
            format!(
                "two workspace members are named acme/core: {} and {}",
                root.join("packages/core").display(),
                root.join("packages/twin").display()
            ),
        ),
        (
            &[("pun.toml", "[workspace]\nmembers = [\"packages/*\", \"../ext/*\"]\n")],
            format!(
                "the workspace member ext/log in {} is outside the workspace root {}",
                root.join("../ext/log").display(),
                root.display()
            ),
        ),
    ] {
        let before: Vec<(String, Option<String>)> = (files.iter())
            .map(|(f, _)| (f.to_string(), std::fs::read_to_string(root.join(f)).ok()))
            .collect();
        common::write_files(&root, files);
        let (_, err, code) = pun(&root, &["install"]);
        assert_eq!(code, Some(2), "{err}");
        assert!(err.contains(&error), "{err}");
        for (f, text) in before {
            match text {
                Some(text) => std::fs::write(root.join(&f), text).unwrap(),
                None => std::fs::remove_file(root.join(&f)).unwrap(),
            }
        }
    }

    // A member linked in from outside the repo is refused too, so `require -p` can't edit its composer.json.
    let ext = read(&root, "../ext/log/composer.json");
    std::os::unix::fs::symlink(d.join("ext/log"), root.join("packages/log")).unwrap();
    let (_, err, code) = pun(
        &root,
        &["require", "-p", "ext/log", "acme/core", "--no-install"],
    );
    assert_eq!(code, Some(2), "{err}");
    assert!(
        err.contains(&format!(
            "the workspace member ext/log in {} is outside the workspace root {}",
            root.join("packages/log").display(),
            root.display()
        )),
        "{err}"
    );
    assert_eq!(read(&root, "../ext/log/composer.json"), ext);
    std::fs::remove_file(root.join("packages/log")).unwrap();

    // The root needs no composer.json; one whose `require` isn't an object (Composer's schema rejects it) holds no
    // requirements, as without one.
    let bare = d.join("bare");
    common::write_files(
        &bare,
        &[
            ("pun.toml", "[workspace]\nmembers = [\"packages/*\"]\n"),
            (
                "packages/one/composer.json",
                r#"{"name": "acme/one", "version": "1.0.0"}"#,
            ),
            (
                ".composer/config.json",
                r#"{"repositories": [{"packagist.org": false}]}"#,
            ),
        ],
    );
    let home = bare.join(".composer");
    let env = [("COMPOSER_HOME", home.to_str().unwrap())];
    for root in [None, Some(r#"{"require": []}"#)] {
        if let Some(text) = root {
            std::fs::write(bare.join("composer.json"), text).unwrap();
        }
        let (_, err, code) = common::pty::pun_with(&bare, &["update", "--dry-run"], &env);
        assert_eq!(
            (err.as_str(), code),
            ("Lock changes:\n + acme/one 1.0.0\n", Some(0)),
            "{root:?}"
        );
    }

    // A pun.toml without `[workspace]`, mentioning the word or not, leaves the project a plain one.
    let plain = d.join("plain");
    common::write_files(
        &plain,
        &[(
            "composer.json",
            r#"{"repositories": [{"packagist.org": false}]}"#,
        )],
    );
    for config in ["# Not a workspace root.\n", "# Pun's settings.\n"] {
        std::fs::write(plain.join("pun.toml"), config).unwrap();
        let (_, err, code) = pun(&plain, &["update", "--dry-run"]);
        assert_eq!(
            (err.as_str(), code),
            ("Nothing to change: the lock is up to date\n", Some(0)),
            "{config}"
        );
    }
}

/// `pun composer dump-autoload` writes Composer's files (the byte comparison is `pun-packages`'
/// `dump_autoload_matches_composer`), exits 1 as Composer does when a package is missing from `vendor/`, and any
/// other `pun composer` command is a run error that names it.
#[test]
fn composer_dump_autoload_and_unknown_commands() {
    let d = fixture("dump-autoload");
    let (out, err, code) = pun(&d, &["install"]);
    assert_eq!(code, Some(0), "{out}{err}");
    std::fs::remove_file(d.join("vendor/composer/autoload_classmap.php")).unwrap();
    // Composer's global `-d`, before the command and after it, from a directory outside the project.
    let (elsewhere, dir) = (std::env::temp_dir(), d.to_str().unwrap());
    let (out, err, code) = pun(
        &elsewhere,
        &["composer", "-d", dir, "dumpautoload", "-a", "--no-dev"],
    );
    assert_eq!(code, Some(0), "{out}{err}");
    assert!(err.contains(" + autoloader: 3 classes\n"), "{err}");
    assert!(d.join("vendor/composer/autoload_classmap.php").exists());
    std::fs::remove_dir_all(d.join("packages/lib")).unwrap();
    let (out, err, code) = pun(
        &elsewhere,
        &[
            "composer",
            "dump-autoload",
            "--dev",
            "--no-scripts",
            "--working-dir",
            dir,
        ],
    );
    assert_eq!(code, Some(1), "{out}{err}");
    assert!(err.contains("Not all dependencies are installed"), "{err}");
    let (_, err, code) = pun(&d, &["composer", "show", "--all"]);
    assert_eq!(code, Some(2));
    assert!(
        err.contains("`pun composer show` is not built yet"),
        "{err}"
    );
}

/// A lock this PHP can't install: Composer's report (`pun-packages/tests/fixtures/verify`), exit 2, no `vendor/`.
#[test]
fn install_refuses_a_lock_the_platform_cannot_run() {
    let d = fixture("platform");
    let json = read(&d, "composer.json").replace("\"php\": \">=8.4\"", "\"ext-nope\": \"^1.0\"");
    std::fs::write(d.join("composer.json"), json).unwrap();
    let lock = read(&d, "composer.lock").replace(
        "\"platform\": {\n        \"php\": \">=8.4\"\n    }",
        "\"platform\": {\n        \"ext-nope\": \"^1.0\"\n    }",
    );
    std::fs::write(d.join("composer.lock"), lock).unwrap();
    let (out, err, code) = pun(&d, &["install"]);
    assert_eq!(code, Some(2), "{out}{err}");
    assert!(
        err.contains("Your lock file does not contain a compatible set of packages. Please run pun update.\n\n  Problem 1\n    - Root composer.json requires PHP extension ext-nope ^1.0 but it is missing from your system."),
        "{err}"
    );
    assert!(!d.join("vendor").exists());
    let (out, err, code) = pun_with(
        &d,
        &["install", "--ignore-platform-req=ext-nope"],
        &[("PUN_LINK_MODE", "symlink")],
    );
    assert_eq!(code, Some(2), "{out}{err}");
    assert!(
        err.contains("unknown link mode `symlink`: use clone, hardlink or copy"),
        "{err}"
    );
    let (out, err, code) = pun_with(
        &d,
        &[
            "install",
            "--ignore-platform-req=ext-nope",
            "--link-mode=copy",
        ],
        &[("PUN_LINK_MODE", "symlink")],
    );
    assert_eq!(code, Some(0), "{out}{err}");
}

/// A lock that doesn't meet composer.json (`pun-packages/tests/fixtures/verify/U`): Composer's lines and its exit 4
/// with nothing installed, and with `config.allow-missing-requirements` the same lines and an install.
#[test]
fn install_stops_when_the_lock_misses_a_requirement() {
    let d = common::scratch("update", "missing-requirements");
    let source =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../pun-packages/tests/fixtures/verify/U/.");
    let copied = std::process::Command::new("cp")
        .arg("-R")
        .arg(source)
        .arg(&d)
        .status()
        .unwrap();
    assert!(copied.success());
    let allow = read(&d, "composer.json");
    let strict = allow.replace(
        "\"allow-missing-requirements\": true",
        "\"allow-missing-requirements\": false",
    );
    std::fs::write(d.join("composer.json"), strict).unwrap();
    let lines = "- Required package \"b/missing\" is not present in the lock file.\n- Required package \"h/ok\" is in the lock file as \"1.2.0\" but that does not satisfy your constraint \"^9.0\".\nThis usually happens when composer files are incorrectly merged or the composer.json file is manually edited.\n";
    let (out, err, code) = pun(&d, &["install"]);
    assert_eq!(code, Some(4), "{out}{err}");
    assert!(err.contains(lines), "{err}");
    assert!(!d.join("vendor").exists());
    std::fs::write(d.join("composer.json"), allow).unwrap();
    let (out, err, code) = pun(&d, &["install"]);
    assert_eq!(code, Some(0), "{out}{err}");
    assert!(err.contains(lines), "{err}");
    assert!(d.join("vendor/h/ok/composer.json").exists());
}
