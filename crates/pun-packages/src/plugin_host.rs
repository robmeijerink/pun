//! Composer plugins, run on Composer's own classes. A plugin is PHP written against Composer's API, so pun runs it
//! in a host process (`assets/plugin-host.php`) on the system PHP and the pinned `composer.phar`, which builds the
//! same `Composer` object `composer install` builds and lets Composer's `PluginManager` activate the allowed plugins.
//! The [`Dispatcher`] sends each event to the host around the project's scripts, in Composer's order.

use crate::Env;
use crate::error::{Context, Error, Result, bail};
use crate::fetch::{Auth, Fetcher, store};
use crate::install::dirs;
use crate::php::sha256;
use crate::plugins::{NATIVE, allowed};
use crate::scripts::{Helper, Runner};
use serde_json::{Map, Value, json};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

/// The Composer the host runs: the version pun's behaviour is checked against.
const COMPOSER: &str = "2.11.2";
const COMPOSER_SHA256: &str = "dd158c0afd41d15fac89e1b4e47b5d42911fe316c3687cd5edac98319b25d028";

/// The pinned composer.phar in Composer's cache (`cache`), downloaded and checked the first time. Without a cache it
/// goes in the host's own directory (`private`), not a shared one where another account could swap it after the
/// check.
fn phar(cache: Option<&Path>, private: &Path) -> Result<PathBuf> {
    let dir = cache.map_or_else(|| private.to_path_buf(), |c| c.join("pun"));
    let path = dir.join(format!("composer-{COMPOSER}.phar"));
    if std::fs::read(&path).is_ok_and(|b| sha256(&b) == COMPOSER_SHA256) {
        return Ok(path);
    }
    let url = format!("https://getcomposer.org/download/{COMPOSER}/composer.phar");
    let fetched = Fetcher::new(None, Auth::default(), true).get(&url);
    let bytes = fetched.context(format!(
        "can't download Composer {COMPOSER}, which runs the project's Composer plugins, from {url}"
    ))?;
    let got = sha256(&bytes);
    if got != COMPOSER_SHA256 {
        bail!("{url} has sha256 {got}, but Composer {COMPOSER}'s is {COMPOSER_SHA256}");
    }
    store(&path, &bytes).with_context(|| format!("can't write {}", path.display()))?;
    Ok(path)
}

/// A running plugin host.
struct Host {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    token: String,
    /// The host script's directory, kept while PHP runs (and reads the phar when it has no cache).
    _helper: Helper,
}

impl Host {
    /// Starts the host; `late` when this command installed the plugins (see the host's header).
    fn start(root: &Path, php: &str, cache: Option<&Path>, late: bool) -> Result<Host> {
        let helper = Helper::new("plugin-host.php", include_str!("../assets/plugin-host.php"))?;
        let phar = phar(cache, helper.dir())?;
        let nanos = (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH))
            .map_or(0, |d| d.as_nanos());
        let token = format!("\u{1e}pun-plugin-host-{}-{nanos}:", std::process::id());
        let child = Command::new(php)
            .arg(&helper.path)
            .arg(&phar)
            .arg(if late { "late" } else { "init" })
            .args(NATIVE)
            .current_dir(root)
            .env("PUN_PLUGIN_HOST_TOKEN", &token)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn();
        let mut child =
            child.with_context(|| format!("can't run {php} for the Composer plugins"))?;
        let pipes = child.stdin.take().zip(child.stdout.take());
        let (stdin, stdout) = pipes.context("can't talk to the Composer plugin host")?;
        let mut host = Host {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            token,
            _helper: helper,
        };
        host.answer()
            .context("Composer couldn't load the project's plugins")?;
        Ok(host)
    }

    fn request(&mut self, request: &Value) -> Result<()> {
        writeln!(self.stdin, "{request}")?;
        self.stdin.flush()?;
        self.answer()
    }

    /// Passes the plugins' output on until the host's answer.
    fn answer(&mut self) -> Result<()> {
        let mut out = std::io::stdout().lock();
        let mut line = vec![];
        loop {
            line.clear();
            if self.stdout.read_until(b'\n', &mut line)? == 0 {
                bail!("the Composer plugin host stopped");
            }
            let text = String::from_utf8_lossy(&line);
            let Some((before, reply)) = text.split_once(&self.token) else {
                out.write_all(&line)?;
                continue;
            };
            out.write_all(before.as_bytes())?;
            let reply: Map<String, Value> = serde_json::from_str(reply.trim_end())?;
            return match reply.get("error").and_then(Value::as_str) {
                Some(e) => Err(Error::msg(e)),
                None => Ok(()),
            };
        }
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The plugins the host has to run: allowed `composer-plugin` packages in installed.json that pun doesn't run
/// itself.
/// installed.json is read for the two fields it needs, not into maps: it runs before every command's first event.
fn needs_host(vendor: &Path, config: &Map<String, Value>) -> bool {
    #[derive(serde::Deserialize)]
    struct Entry {
        name: String,
        #[serde(rename = "type")]
        kind: Option<String>,
    }
    #[derive(serde::Deserialize)]
    struct Repository {
        packages: Vec<Entry>,
    }
    let Ok(bytes) = std::fs::read(vendor.join("composer/installed.json")) else {
        return false;
    };
    let entries = (serde_json::from_slice::<Repository>(&bytes).map(|r| r.packages))
        .or_else(|_| serde_json::from_slice::<Vec<Entry>>(&bytes))
        .unwrap_or_default();
    (entries.iter()).any(|e| hosted(&e.name, e.kind.as_deref(), config))
}

/// Whether the package is an allowed plugin that only the host can run.
pub(crate) fn hosted(name: &str, kind: Option<&str>, config: &Map<String, Value>) -> bool {
    let name = name.to_ascii_lowercase();
    kind == Some("composer-plugin") && !NATIVE.contains(&name.as_str()) && allowed(config, &name)
}

/// Dispatches a command's events: each one's plugin listeners at priority 0 and up, then the project's scripts,
/// then its negative-priority listeners, as Composer's `EventDispatcher` orders them. `--no-scripts` drops only the
/// scripts, as in Composer; `--no-plugins` drops the plugins.
pub struct Dispatcher {
    scripts: Runner,
    run_scripts: bool,
    plugins: Option<Plugins>,
}

struct Plugins {
    root: PathBuf,
    vendor: PathBuf,
    config: Map<String, Value>,
    cache: Option<PathBuf>,
    host: Option<Host>,
    /// installed.json's modification time and size when the command started, and when the host was last checked
    /// against it.
    initial: Option<(std::time::SystemTime, u64)>,
    seen: Option<(std::time::SystemTime, u64)>,
}

fn installed_state(vendor: &Path) -> Option<(std::time::SystemTime, u64)> {
    let m = std::fs::metadata(vendor.join("composer/installed.json")).ok()?;
    Some((m.modified().ok()?, m.len()))
}

impl Plugins {
    /// The host for the plugins installed now: started when the first one is installed, told when they change.
    fn host(&mut self, php: &str) -> Result<Option<&mut Host>> {
        let state = installed_state(&self.vendor);
        if state != self.seen {
            self.seen = state;
            if let Some(host) = &mut self.host {
                host.request(&json!({"op": "sync"}))?;
            } else if needs_host(&self.vendor, &self.config) {
                let late = state != self.initial;
                self.host = Some(Host::start(&self.root, php, self.cache.as_deref(), late)?);
            }
        }
        Ok(self.host.as_mut())
    }
}

impl Dispatcher {
    pub fn new(
        root: &Path,
        manifest: &Map<String, Value>,
        dev_mode: bool,
        run_scripts: bool,
        run_plugins: bool,
        cache: Option<&Path>,
        env: Env,
    ) -> Dispatcher {
        let d = dirs(&root.to_string_lossy(), manifest);
        let plugins = run_plugins.then(|| Plugins {
            root: root.to_path_buf(),
            vendor: PathBuf::from(&d.vendor),
            initial: installed_state(Path::new(&d.vendor)),
            config: (manifest.get("config").and_then(Value::as_object))
                .cloned()
                .unwrap_or_default(),
            cache: cache.map(Path::to_path_buf),
            host: None,
            seen: None,
        });
        Dispatcher {
            scripts: Runner::new(root, &d.vendor, &d.bin, manifest, dev_mode, env),
            run_scripts,
            plugins,
        }
    }

    pub fn dispatch(&mut self, event: &str) -> Result<()> {
        self.dispatch_script(event, &[])
    }

    /// Whether the scripts print each command before it runs (the default).
    pub fn echo(&mut self, on: bool) {
        self.scripts.echo = on;
    }

    /// Sets the scripts' timeout in seconds (0 for none), as `run-script --timeout` does.
    pub fn timeout(&mut self, seconds: u64) {
        self.scripts.timeout = seconds;
    }

    /// Dispatches `event` with the arguments `composer run-script` passes on to its scripts.
    pub fn dispatch_script(&mut self, event: &str, args: &[String]) -> Result<()> {
        let dev = self.scripts.dev_mode;
        let mut host = match &mut self.plugins {
            Some(p) => p.host(&self.scripts.php)?,
            None => None,
        };
        let phase = |host: &mut Option<&mut Host>, phase: &str| -> Result<()> {
            match host {
                Some(h) => h
                    .request(&json!({"op": "dispatch", "event": event, "dev": dev, "phase": phase}))
                    .with_context(|| format!("a Composer plugin failed on the {event} event")),
                None => Ok(()),
            }
        };
        phase(&mut host, "before")?;
        if self.run_scripts {
            self.scripts.dispatch(event, args)?;
        }
        phase(&mut host, "after")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Both shapes of installed.json Composer reads (2's `{"packages": [...]}` and 1's list), and none at all.
    #[test]
    fn needs_the_host_for_an_allowed_plugin_in_either_shape() {
        let vendor = std::env::temp_dir().join(format!("pun-needs-host-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&vendor);
        let config = serde_json::json!({"allow-plugins": {"acme/*": true}});
        let config = config.as_object().unwrap();
        assert!(!needs_host(&vendor, config));
        std::fs::create_dir_all(vendor.join("composer")).unwrap();
        let entry = r#"{"name": "acme/plugin", "type": "composer-plugin"}"#;
        for json in [
            format!(r#"{{"packages": [{entry}]}}"#),
            format!("[{entry}]"),
        ] {
            std::fs::write(vendor.join("composer/installed.json"), json).unwrap();
            assert!(needs_host(&vendor, config));
            assert!(!needs_host(&vendor, &Map::new()));
        }
        std::fs::remove_dir_all(&vendor).unwrap();
    }
}
