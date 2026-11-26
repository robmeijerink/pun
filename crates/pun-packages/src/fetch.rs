use crate::error::{Context, Error, Result, bail};
use crate::php::sha1;
use pcre2::bytes::Regex;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::io::{Read, Write as _};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

#[derive(Debug, Default, Clone)]
pub struct Auth {
    headers: Vec<(String, String)>,
}

impl Auth {
    pub fn load(project: &Path, home: Option<&Path>, env: Option<String>) -> Auth {
        let mut auth = Auth::default();
        let mut texts: Vec<String> = env.into_iter().collect();
        texts.extend(std::fs::read_to_string(project.join("auth.json")));
        if let Some(home) = home {
            texts.extend(std::fs::read_to_string(home.join("auth.json")));
        }
        for text in texts {
            if let Ok(Value::Object(json)) = serde_json::from_str::<Value>(&text) {
                auth.add(&json);
            }
        }
        auth
    }

    fn add(&mut self, json: &Map<String, Value>) {
        let entries = |key: &str| {
            json.get(key)
                .and_then(Value::as_object)
                .into_iter()
                .flatten()
        };
        for (host, token) in entries("github-oauth") {
            if let Some(token) = token.as_str() {
                self.set(host, format!("token {token}"));
                if host == "github.com" {
                    self.set("api.github.com", format!("token {token}"));
                }
            }
        }
        for (host, token) in entries("gitlab-token") {
            match token {
                Value::String(t) => self.set(host, format!("Bearer {t}")),
                Value::Object(t) => self.set(host, basic(t.get("username"), t.get("token"))),
                _ => {}
            }
        }
        for (host, token) in entries("bearer") {
            if let Some(token) = token.as_str() {
                self.set(host, format!("Bearer {token}"));
            }
        }
        for (host, cred) in entries("http-basic") {
            if let Value::Object(c) = cred {
                self.set(host, basic(c.get("username"), c.get("password")));
            }
        }
    }

    fn set(&mut self, host: &str, header: String) {
        if !self.headers.iter().any(|h| h.0 == host) {
            self.headers.push((host.to_string(), header));
        }
    }

    /// The credential for the URL's origin: its host, plus the port when the URL names one. As Composer's
    /// `Url::getOrigin`, a credential for `host` never goes to `host:8443`, which may be another service.
    fn header(&self, url: &str) -> Option<&str> {
        let host = url.split("://").nth(1)?.split(['/', '?', '#']).next()?;
        let host = host.rsplit_once('@').map_or(host, |h| h.1);
        self.headers
            .iter()
            .find(|h| h.0 == host)
            .map(|h| h.1.as_str())
    }
}

/// A URL as errors show it: without the user information and the query, which can carry a token or a signature
/// that grants the download, as Composer's `Url::sanitize` hides passwords and access tokens.
pub fn redact(url: &str) -> String {
    let (scheme, rest) = url.split_once("://").unwrap_or(("", url));
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, path) = rest.split_at(end);
    let authority = authority
        .rsplit_once('@')
        .map_or(authority.to_string(), |(_, h)| format!("***@{h}"));
    let path = match path.find(['?', '#']) {
        Some(i) => format!("{}?***", &path[..i]),
        None => path.to_string(),
    };
    let prefix = if url.contains("://") {
        format!("{scheme}://")
    } else {
        String::new()
    };
    format!("{prefix}{authority}{path}")
}

fn basic(user: Option<&Value>, password: Option<&Value>) -> String {
    let text = |v: Option<&Value>| v.and_then(Value::as_str).unwrap_or_default().to_string();
    format!(
        "Basic {}",
        base64(format!("{}:{}", text(user), text(password)).as_bytes())
    )
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, b)| n | u32::from(*b) << (16 - 8 * i));
        for i in 0..4 {
            out.push(if i <= chunk.len() {
                ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char
            } else {
                '='
            });
        }
    }
    out
}

pub fn composer_dirs() -> Option<(PathBuf, PathBuf)> {
    composer_dirs_in(&|k| std::env::var(k).ok())
}

/// [`composer_dirs`] with the variables `env` gives. Whether XDG applies still comes from the process's own
/// environment, which Composer reads from `$_SERVER`; it only decides which directory under `HOME` comes first.
pub fn composer_dirs_in(env: crate::Env) -> Option<(PathBuf, PathBuf)> {
    let xdg_env = std::env::vars_os().any(|(k, _)| k.to_string_lossy().starts_with("XDG_"));
    let xdg = Path::new("/etc/xdg").is_dir() || xdg_env;
    dirs_in(env, xdg, cfg!(target_os = "macos"))
}

fn dirs_in(
    env: &dyn Fn(&str) -> Option<String>,
    xdg: bool,
    macos: bool,
) -> Option<(PathBuf, PathBuf)> {
    let env = |k: &str| env(k).filter(|v| !v.is_empty());
    let composer_home = env("COMPOSER_HOME");
    let user = || env("HOME").map(|h| h.trim_end_matches('/').to_string());
    let home = if let Some(home) = &composer_home {
        home.clone()
    } else {
        let user = user()?;
        let mut dirs = vec![];
        if xdg {
            let config = env("XDG_CONFIG_HOME").unwrap_or_else(|| format!("{user}/.config"));
            dirs.push(format!("{config}/composer"));
        }
        dirs.push(format!("{user}/.composer"));
        (dirs.iter().find(|d| Path::new(d).is_dir()))
            .unwrap_or(&dirs[0])
            .clone()
    };
    let cache = if let Some(dir) = env("COMPOSER_CACHE_DIR") {
        dir
    } else if let Some(home) = &composer_home {
        format!("{home}/cache")
    } else {
        let user = user()?;
        if macos {
            format!("{user}/Library/Caches/composer")
        } else if home == format!("{user}/.composer") && Path::new(&home).join("cache").is_dir() {
            format!("{home}/cache")
        } else if xdg {
            let cache = env("XDG_CACHE_HOME").unwrap_or_else(|| format!("{user}/.cache"));
            format!("{cache}/composer")
        } else {
            format!("{home}/cache")
        }
    };
    Some((PathBuf::from(home), PathBuf::from(cache)))
}

pub fn dist_url(url: &str, reference: Option<&str>) -> String {
    static RULES: LazyLock<Vec<(Regex, &str)>> = LazyLock::new(|| {
        [
            (r"(?i)^https?://(?:www\.)?github\.com/([^/]+)/([^/]+)/(zip|tar)ball/(.+)$", "github"),
            (r"(?i)^https?://(?:www\.)?github\.com/([^/]+)/([^/]+)/archive/.+\.(zip|tar)(?:\.gz)?$", "github"),
            (r"(?i)^https?://api\.github\.com/repos/([^/]+)/([^/]+)/(zip|tar)ball(?:/.+)?$", "github"),
            (r"(?i)^https?://(?:www\.)?bitbucket\.org/([^/]+)/([^/]+)/get/(.+)\.(zip|tar\.gz|tar\.bz2)$", "bitbucket"),
            (r"(?i)^https?://(?:www\.)?gitlab\.com/api/v[34]/projects/([^/]+)/repository/archive\.(zip|tar\.gz|tar\.bz2|tar)\?sha=.+$", "gitlab"),
        ]
        .into_iter()
        .map(|(p, kind)| (Regex::new(p).expect("valid pattern"), kind))
        .collect()
    });
    let Some(reference) = reference.filter(|r| !r.is_empty()) else {
        return url.to_string();
    };
    for (re, kind) in RULES.iter() {
        let Ok(Some(c)) = re.captures(url.as_bytes()) else {
            continue;
        };
        let g = |i: usize| String::from_utf8_lossy(&c[i]).into_owned();
        return match *kind {
            "github" => format!(
                "https://api.github.com/repos/{}/{}/{}ball/{reference}",
                g(1),
                g(2),
                g(3)
            ),
            "bitbucket" => format!(
                "https://bitbucket.org/{}/{}/get/{reference}.{}",
                g(1),
                g(2),
                g(4)
            ),
            _ => format!(
                "https://gitlab.com/api/v4/projects/{}/repository/archive.{}?sha={reference}",
                g(1),
                g(2)
            ),
        };
    }
    url.to_string()
}

#[derive(Debug)]
pub enum Fetched {
    NotModified,
    NotFound,
    Body(Vec<u8>, Option<String>),
}

#[derive(Debug, Default)]
struct OnceResolver {
    resolved: std::sync::Mutex<HashMap<String, ureq::unversioned::resolver::ResolvedSocketAddrs>>,
}

impl ureq::unversioned::resolver::Resolver for OnceResolver {
    fn resolve(
        &self,
        uri: &ureq::http::Uri,
        config: &ureq::config::Config,
        timeout: ureq::unversioned::transport::NextTimeout,
    ) -> Result<ureq::unversioned::resolver::ResolvedSocketAddrs, ureq::Error> {
        let key = format!("{:?}{:?}", uri.scheme(), uri.authority());
        let mut resolved = self.resolved.lock().expect("the resolver's cache");
        if let Some(addrs) = resolved.get(&key) {
            return Ok(addrs.clone());
        }
        let mut addrs = ureq::unversioned::resolver::DefaultResolver::default()
            .resolve(uri, config, timeout)?;
        prefer_reachable(&mut addrs, ATTEMPT_DELAY, GIVE_UP, |addr, limit| {
            std::net::TcpStream::connect_timeout(&addr, limit).is_ok()
        });
        resolved.insert(key, addrs.clone());
        Ok(addrs)
    }
}

/// RFC 8305's connection attempt delay: how long one address has before the next is tried as well.
const ATTEMPT_DELAY: Duration = Duration::from_millis(250);
/// How long the race waits for any address; after it, ureq tries them in the resolver's order.
const GIVE_UP: Duration = Duration::from_secs(10);

/// Moves the first of `addrs` that accepts a TCP connection to the front, racing them as happy eyeballs (RFC 8305)
/// does: the families interleaved, a new attempt every `delay` or as soon as one fails. ureq tries a host's
/// addresses one after another without a connect timeout, so on a network whose IPv6 route drops packets every
/// connection to a host with an AAAA record (repo.packagist.org) waited out the kernel's SYN timeout (31 s at
/// `tcp_syn_retries=4`) before it tried IPv4. curl, and so Composer, races the families.
fn prefer_reachable(
    addrs: &mut [SocketAddr],
    delay: Duration,
    give_up: Duration,
    connect: fn(SocketAddr, Duration) -> bool,
) {
    if addrs.len() < 2 {
        return;
    }
    let (v6, v4): (Vec<usize>, Vec<usize>) = (0..addrs.len()).partition(|&i| addrs[i].is_ipv6());
    let first_v6 = addrs[0].is_ipv6();
    let (a, b) = if first_v6 { (v6, v4) } else { (v4, v6) };
    let mut order = vec![];
    for i in 0..a.len().max(b.len()) {
        order.extend(a.get(i));
        order.extend(b.get(i));
    }
    let (tx, rx) = std::sync::mpsc::channel();
    let deadline = std::time::Instant::now() + give_up;
    let (mut started, mut failed) = (0, 0);
    let won = loop {
        if let Some(&i) = order.get(started) {
            let (tx, addr) = (tx.clone(), addrs[i]);
            // Detached: an attempt into a black hole outlives the race, until `give_up`.
            std::thread::spawn(move || tx.send((i, connect(addr, give_up))));
            started += 1;
        }
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        let wait = if started < order.len() {
            delay.min(left)
        } else {
            left
        };
        match rx.recv_timeout(wait) {
            Ok((i, true)) => break Some(i),
            Ok(_) => failed += 1,
            Err(_) if started < order.len() => {}
            Err(_) => break None,
        }
        if failed == order.len() {
            break None;
        }
    };
    if let Some(i) = won {
        addrs[..=i].rotate_right(1);
    }
}

pub struct Fetcher {
    agent: ureq::Agent,
    cache: Option<PathBuf>,
    auth: Auth,
    secure_http: bool,
}

impl Fetcher {
    pub fn new(cache: Option<PathBuf>, auth: Auth, secure_http: bool) -> Fetcher {
        let config = ureq::Agent::config_builder()
            .user_agent(format!("pun/{}", env!("CARGO_PKG_VERSION")))
            .https_only(secure_http)
            .http_status_as_error(false)
            .max_idle_connections(64)
            .max_idle_connections_per_host(64)
            .build();
        let agent = ureq::Agent::with_parts(
            config,
            ureq::unversioned::transport::DefaultConnector::new(),
            OnceResolver::default(),
        );
        Fetcher {
            agent,
            cache,
            auth,
            secure_http,
        }
    }

    pub fn archive(&self, package: &Map<String, Value>) -> Result<(Vec<u8>, bool)> {
        let name = package["name"]
            .as_str()
            .unwrap_or_default()
            .to_ascii_lowercase();
        let dist = package
            .get("dist")
            .and_then(Value::as_object)
            .context("no dist")?;
        let kind = dist.get("type").and_then(Value::as_str).unwrap_or("zip");
        // The type names the cache file, so one `unpack` can't extract is refused before the cache is touched.
        if !matches!(kind, "zip" | "tar") {
            bail!("dist type `{kind}` is not supported yet");
        }
        let raw = dist
            .get("url")
            .and_then(Value::as_str)
            .context("dist without a url")?;
        let url = dist_url(raw, dist.get("reference").and_then(Value::as_str));
        let shasum = dist
            .get("shasum")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty());
        let cached = (self.cache.as_ref()).map(|c| {
            c.join("files")
                .join(&name)
                .join(format!("{}.{kind}", sha1(url.as_bytes())))
        });
        if let Some(path) = &cached
            && let Ok(bytes) = std::fs::read(path)
            && shasum.is_none_or(|s| s == sha1(&bytes))
        {
            return Ok((bytes, true));
        }
        let scheme = url
            .split("://")
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        let shown = redact(&url);
        if self.secure_http
            && url.contains("://")
            && ["http", "git", "ftp", "svn"].contains(&scheme.as_str())
        {
            bail!(
                "Your configuration does not allow connections to {shown}. See https://getcomposer.org/doc/06-config.md#secure-http for details."
            );
        }
        let bytes = self
            .get(&url)
            .with_context(|| format!("downloading {name} from {shown}"))?;
        if let Some(want) = shasum {
            let got = sha1(&bytes);
            if got != want {
                bail!(
                    "the {name} archive from {shown} has sha1 {got}, but composer.lock expects {want}; it may have been tampered with"
                );
            }
        }
        if let Some(path) = &cached {
            // The cache is best effort, as Composer's: a failed write only costs a download next time.
            let _ = store(path, &bytes);
        }
        Ok((bytes, false))
    }

    pub fn cache_dir(&self) -> Option<&Path> {
        self.cache.as_deref()
    }

    pub fn metadata(&self, url: &str, since: Option<&str>) -> Result<Fetched> {
        let mut attempt = 0;
        loop {
            let mut req = self.agent.get(url);
            if let Some(h) = self.auth.header(url) {
                req = req.header("Authorization", h);
            }
            if let Some(since) = since {
                req = req.header("If-Modified-Since", since);
            }
            let error = match req.call() {
                Ok(res) if res.status().as_u16() == 304 => return Ok(Fetched::NotModified),
                Ok(res) if res.status().as_u16() == 404 => return Ok(Fetched::NotFound),
                Ok(mut res) if res.status().is_success() => {
                    let modified = (res.headers().get("last-modified"))
                        .and_then(|v| v.to_str().ok())
                        .map(String::from);
                    let mut body = vec![];
                    res.body_mut()
                        .with_config()
                        .limit(256 << 20)
                        .reader()
                        .read_to_end(&mut body)?;
                    return Ok(Fetched::Body(body, modified));
                }
                Ok(res) => {
                    let status = res.status().as_u16();
                    if !(status == 429 || status >= 500) {
                        bail!("HTTP {status} from {}", redact(url));
                    }
                    Error::msg(format!("HTTP {status} from {}", redact(url)))
                }
                Err(e) => e.into(),
            };
            attempt += 1;
            if attempt == 3 {
                return Err(error);
            }
            std::thread::sleep(std::time::Duration::from_millis(500 << (attempt - 1)));
        }
    }

    pub(crate) fn get(&self, url: &str) -> Result<Vec<u8>> {
        let mut attempt = 0;
        loop {
            let mut req = self.agent.get(url);
            if let Some(h) = self.auth.header(url) {
                req = req.header("Authorization", h);
            }
            let error = match req.call() {
                Ok(mut res) if res.status().is_success() => {
                    let mut body = vec![];
                    (res.body_mut().with_config().limit(MAX_DOWNLOAD))
                        .reader()
                        .read_to_end(&mut body)?;
                    return Ok(body);
                }
                Ok(res) => {
                    let status = res.status().as_u16();
                    if !(status == 429 || status >= 500) {
                        bail!("HTTP {status}");
                    }
                    Error::msg(format!("HTTP {status}"))
                }
                Err(e) => e.into(),
            };
            attempt += 1;
            if attempt == 3 {
                return Err(error);
            }
            std::thread::sleep(std::time::Duration::from_millis(500 << (attempt - 1)));
        }
    }
}

/// Writes `bytes` to `path` through a temporary file, so a reader never sees half a file. The temporary file is
/// created new (`O_EXCL`), so an entry already at its name, such as a link to another file, is never written through.
pub fn store(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    std::fs::create_dir_all(path.parent().unwrap_or(Path::new("/")))?;
    let mut attempt = 0;
    let (tmp, mut file) = loop {
        let mut tmp = path.as_os_str().to_owned();
        tmp.push(format!(".{}-{attempt}.tmp", std::process::id()));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
        {
            Ok(file) => break (PathBuf::from(tmp), file),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && attempt < 100 => {
                attempt += 1;
            }
            Err(e) => return Err(e),
        }
    };
    let written = file.write_all(bytes);
    drop(file);
    written
        .and_then(|()| std::fs::rename(&tmp, path))
        .inspect_err(|_| {
            let _ = std::fs::remove_file(&tmp);
        })
}

static METADATA: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:\._.*|\.DS_Store|__MACOSX|\.AppleDouble|\.fseventsd|\.Spotlight-V100|\.Trashes|(?i:thumbs\.db|desktop\.ini))$")
        .expect("valid pattern")
});

/// The most bytes a download may hold, the most one archive may expand to, and the most entries it may hold. The
/// largest Composer dists are a few hundred MB with some tens of thousands of files, so these only stop an archive
/// made to fill the memory or the disk.
const MAX_DOWNLOAD: u64 = 2 << 30;
const MAX_EXPANDED: u64 = 8 << 30;
const MAX_ENTRIES: usize = 1_000_000;

/// Reads `inner`, failing once the readers sharing `used` have read more than `MAX_EXPANDED` bytes.
struct Budget<'a, R> {
    inner: R,
    used: &'a AtomicU64,
}

impl<R: Read> Read for Budget<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        if self.used.fetch_add(n as u64, Ordering::Relaxed) + n as u64 > MAX_EXPANDED {
            return Err(std::io::Error::other(format!(
                "the archive expands past {} GiB",
                MAX_EXPANDED >> 30
            )));
        }
        Ok(n)
    }
}

pub fn unpack(kind: &str, bytes: &[u8], target: &Path) -> Result<()> {
    let parent = target.parent().context("target without a parent")?;
    std::fs::create_dir_all(parent)?;
    let tmp = parent.join(format!(
        ".pun-{}-{}",
        target.file_name().unwrap_or_default().to_string_lossy(),
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp)?;
    let used = AtomicU64::new(0);
    let result = (|| -> Result<()> {
        let links = match kind {
            "zip" => unzip(bytes, &tmp, &used)?,
            "tar" => untar(bytes, &tmp, &used)?,
            other => bail!("dist type `{other}` is not supported yet"),
        };
        let entries: Vec<PathBuf> = std::fs::read_dir(&tmp)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .collect();
        let mut content: Vec<&PathBuf> = vec![];
        for p in &entries {
            if !METADATA.is_match(p.file_name().unwrap_or_default().as_encoded_bytes())? {
                content.push(p);
            }
        }
        // A sole directory is the archive's wrapper; a sole symlink is not, so the package root is a directory.
        let from = match content.as_slice() {
            [only] if std::fs::symlink_metadata(only).is_ok_and(|m| m.is_dir()) => (*only).clone(),
            _ => tmp.clone(),
        };
        // Links are checked against the package as installed, below the wrapper.
        for link in links.iter().filter(|l| l.starts_with(&from)) {
            if escapes(&from, link) {
                let _ = std::fs::remove_file(link);
            }
        }
        remove(target)?;
        std::fs::rename(&from, target)?;
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&tmp);
    result
}

/// Whether the symlink `link` below `root` leads outside it, following each link on the way as the kernel would: an
/// absolute target, or a `..` that climbs past `root`.
fn escapes(root: &Path, link: &Path) -> bool {
    use std::ffi::OsString;
    use std::path::Component;
    let Ok(rel) = link.strip_prefix(root) else {
        return true;
    };
    let path = |at: &[OsString]| root.join(at.iter().collect::<PathBuf>());
    let mut at: Vec<OsString> = rel.iter().map(Into::into).collect();
    // The components still to walk, the next one last; `None` is a `..`.
    let mut todo: Vec<Option<OsString>> = vec![];
    // `at` is a symlink here; 40 is Linux's limit on the links one lookup follows.
    for _ in 0..40 {
        let Ok(target) = std::fs::read_link(path(&at)) else {
            return true;
        };
        at.pop();
        let mut parts = vec![];
        for c in target.components() {
            match c {
                Component::Normal(n) => parts.push(Some(n.to_os_string())),
                Component::ParentDir => parts.push(None),
                Component::CurDir => {}
                Component::RootDir | Component::Prefix(_) => return true,
            }
        }
        todo.extend(parts.into_iter().rev());
        loop {
            match todo.pop() {
                None => return false,
                Some(None) => {
                    if at.pop().is_none() {
                        return true;
                    }
                }
                Some(Some(name)) => {
                    at.push(name);
                    if std::fs::symlink_metadata(path(&at)).is_ok_and(|m| m.is_symlink()) {
                        break;
                    }
                }
            }
        }
    }
    true
}

/// Unpacks a tar into `dir` as `tar::Archive::unpack` does (directories last, so their modes don't stop their
/// files); the symlinks it made.
fn untar(bytes: &[u8], dir: &Path, used: &AtomicU64) -> Result<Vec<PathBuf>> {
    let reader: Box<dyn Read> = if bytes.starts_with(&[0x1f, 0x8b]) {
        Box::new(flate2::read::GzDecoder::new(bytes))
    } else {
        Box::new(bytes)
    };
    let mut archive = tar::Archive::new(Budget {
        inner: reader,
        used,
    });
    let mut directories = vec![];
    let mut links = vec![];
    for (n, entry) in archive.entries()?.enumerate() {
        if n == MAX_ENTRIES {
            bail!("the archive holds more than {MAX_ENTRIES} entries");
        }
        let mut entry = entry?;
        let kind = entry.header().entry_type();
        if kind == tar::EntryType::Directory {
            directories.push(entry);
            continue;
        }
        let path: PathBuf = (entry.path()?.components())
            .filter(|c| matches!(c, std::path::Component::Normal(_)))
            .collect();
        if entry.unpack_in(dir)? && kind == tar::EntryType::Symlink {
            links.push(dir.join(path));
        }
    }
    directories.sort_by(|a, b| b.path_bytes().cmp(&a.path_bytes()));
    for mut d in directories {
        d.unpack_in(dir)?;
    }
    Ok(links)
}

/// Unpacks a zip into `dir`; the symlinks it made.
fn unzip(bytes: &[u8], dir: &Path, used: &AtomicU64) -> Result<Vec<PathBuf>> {
    use rayon::prelude::*;
    use std::os::unix::fs::PermissionsExt;
    let archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
    if archive.len() > MAX_ENTRIES {
        bail!("the archive holds more than {MAX_ENTRIES} entries");
    }
    let links = std::sync::Mutex::new(vec![]);
    (0..archive.len()).into_par_iter().try_for_each_init(
        || archive.clone(),
        |archive, i| -> Result<()> {
            let mut entry = archive.by_index(i)?;
            let Some(name) = entry.enclosed_name() else {
                return Ok(());
            };
            let path = dir.join(name);
            if entry.is_dir() {
                std::fs::create_dir_all(&path)?;
                return Ok(());
            }
            std::fs::create_dir_all(path.parent().unwrap_or(dir))?;
            let mode = entry.unix_mode();
            if entry.is_symlink() {
                // PATH_MAX: a longer target can't be followed.
                let mut target = String::new();
                (&mut entry).take(4097).read_to_string(&mut target)?;
                if target.len() <= 4096 {
                    (links
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner))
                    .push((path, target));
                }
                return Ok(());
            }
            let mut out = std::fs::File::create(&path)?;
            std::io::copy(
                &mut Budget {
                    inner: &mut entry,
                    used,
                },
                &mut out,
            )?;
            if let Some(mode) = mode.filter(|m| m & 0o111 != 0) {
                std::fs::set_permissions(
                    &path,
                    std::fs::Permissions::from_mode((mode & 0o777) | 0o600),
                )?;
            }
            Ok(())
        },
    )?;
    // Last, so no file is written through a link.
    Ok((links
        .into_inner()
        .unwrap_or_else(std::sync::PoisonError::into_inner))
    .into_iter()
    .filter(|(path, target)| {
        !target.starts_with('/') && std::os::unix::fs::symlink(target, path).is_ok()
    })
    .map(|(path, _)| path)
    .collect())
}

/// Links the path repository `url` into `target`, or with `mirror` (its `symlink: false`) copies it there, as
/// Composer's `PathDownloader` does.
pub fn link_path(url: &str, base: &Path, target: &Path, mirror: bool) -> Result<()> {
    let source = if Path::new(url).is_absolute() {
        PathBuf::from(url)
    } else {
        base.join(url)
    };
    let source = std::fs::canonicalize(&source)
        .with_context(|| format!("path repository {url} does not exist"))?;
    remove(target)?;
    std::fs::create_dir_all(target.parent().context("target without a parent")?)?;
    if mirror {
        return Ok(copy_tree(&source, &source, target)?);
    }
    let parent = std::fs::canonicalize(target.parent().unwrap_or(target))?;
    let relative =
        crate::paths::shortest(&parent.to_string_lossy(), &source.to_string_lossy(), true);
    std::os::unix::fs::symlink(format!("{}/", relative.trim_end_matches('/')), target)?;
    Ok(())
}

/// Copies `from`, below the path repository `root`, to `to` as Composer's mirror does (Symfony's `mirror` over an
/// `ArchivableFilesFinder`): without version control directories, and without a symlink that leads outside `root`.
fn copy_tree(root: &Path, from: &Path, to: &Path) -> std::io::Result<()> {
    const VCS: [&str; 9] = [
        ".svn",
        "_svn",
        "CVS",
        "_darcs",
        ".arch-params",
        ".monotone",
        ".bzr",
        ".git",
        ".hg",
    ];
    std::fs::create_dir(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let name = e.file_name();
        if VCS.iter().any(|v| name == *v) {
            continue;
        }
        let (src, dst) = (e.path(), to.join(&name));
        let kind = e.file_type()?;
        if kind.is_symlink() {
            if std::fs::canonicalize(&src).is_ok_and(|real| real.starts_with(root)) {
                std::os::unix::fs::symlink(std::fs::read_link(&src)?, &dst)?;
            }
        } else if kind.is_dir() {
            copy_tree(root, &src, &dst)?;
        } else {
            std::fs::copy(&src, &dst)?;
        }
    }
    Ok(())
}

pub fn remove(path: &Path) -> std::io::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(m) if m.is_dir() => std::fs::remove_dir_all(path),
        Ok(_) => std::fs::remove_file(path),
        Err(_) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn composer_dirs_match_composer() {
        let t = scratch("dirs");
        for d in [
            "a",
            "b/.composer/cache",
            "c/cfg/composer",
            "d/.config/composer",
            "d/.composer",
        ] {
            std::fs::create_dir_all(t.join(d)).unwrap();
        }
        let t = t.to_string_lossy();
        let dirs = |vars: &[(&str, String)], xdg: bool, macos: bool| {
            let env = |k: &str| vars.iter().find(|v| v.0 == k).map(|v| v.1.clone());
            dirs_in(&env, xdg, macos).map(|(h, c)| {
                (
                    h.to_string_lossy().into_owned(),
                    c.to_string_lossy().into_owned(),
                )
            })
        };
        let pair = |h: &str, c: &str| Some((h.to_string(), c.to_string()));
        let home = |d: &str| ("HOME", format!("{t}/{d}"));
        let set = |k: &'static str, v: &str| (k, v.to_string());
        assert_eq!(
            dirs(
                &[
                    set("COMPOSER_HOME", "/x"),
                    set("COMPOSER_CACHE_DIR", "/c"),
                    home("a")
                ],
                true,
                false
            ),
            pair("/x", "/c")
        );
        assert_eq!(
            dirs(&[set("COMPOSER_HOME", "/x"), home("a")], true, true),
            pair("/x", "/x/cache")
        );
        assert_eq!(
            dirs(&[home("a/")], true, false),
            pair(
                &format!("{t}/a/.config/composer"),
                &format!("{t}/a/.cache/composer")
            )
        );
        assert_eq!(
            dirs(&[home("b")], true, false),
            pair(
                &format!("{t}/b/.composer"),
                &format!("{t}/b/.composer/cache")
            )
        );
        let xdg = [
            home("c"),
            ("XDG_CONFIG_HOME", format!("{t}/c/cfg")),
            ("XDG_CACHE_HOME", format!("{t}/c/cch")),
        ];
        assert_eq!(
            dirs(&xdg, true, false),
            pair(
                &format!("{t}/c/cfg/composer"),
                &format!("{t}/c/cch/composer")
            )
        );
        assert_eq!(
            dirs(&[home("d")], true, false),
            pair(
                &format!("{t}/d/.config/composer"),
                &format!("{t}/d/.cache/composer")
            )
        );
        assert_eq!(
            dirs(&[home("a")], false, false),
            pair(
                &format!("{t}/a/.composer"),
                &format!("{t}/a/.composer/cache")
            )
        );
        assert_eq!(
            dirs(&[home("b")], false, true),
            pair(
                &format!("{t}/b/.composer"),
                &format!("{t}/b/Library/Caches/composer")
            )
        );
        assert_eq!(dirs(&[set("HOME", "")], true, false), None);
        assert_eq!(dirs(&[set("COMPOSER_CACHE_DIR", "/c")], true, false), None);
        assert!(composer_dirs().is_some(), "HOME is set in tests");
    }

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pun-fetch-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// Links planted at the temporary names a write uses (the process id is no secret) are neither written
    /// through nor removed; the file still lands.
    #[test]
    fn store_never_writes_through_a_planted_temporary_name() {
        let d = scratch("store-planted");
        let (sentinel, path) = (d.join("sentinel"), d.join("composer.lock"));
        std::fs::write(&sentinel, "kept").unwrap();
        let pid = std::process::id();
        let planted = [
            d.join(format!("composer.lock.{pid}.tmp")),
            d.join(format!("composer.lock.{pid}-0.tmp")),
        ];
        for link in &planted {
            std::os::unix::fs::symlink(&sentinel, link).unwrap();
        }
        store(&path, b"new").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
        assert_eq!(std::fs::read_to_string(&sentinel).unwrap(), "kept");
        assert!(planted.iter().all(|l| l.is_symlink()));
        std::fs::remove_dir_all(&d).unwrap();
    }

    /// A black hole (an attempt that never connects within the race) costs one attempt delay, not a connect
    /// timeout per address; a refused address costs nothing; IPv4 and IPv6 take turns.
    #[test]
    fn the_first_reachable_address_goes_first() {
        // 2001:db8::1 is the black hole, 10.0.0.9 refuses, the rest connect.
        fn connect(addr: SocketAddr, _: Duration) -> bool {
            match addr.ip().to_string().as_str() {
                "2001:db8::1" => {
                    std::thread::sleep(Duration::from_secs(2));
                    false
                }
                "10.0.0.9" => false,
                // Slow: fails after the next attempt has started, which connects later still.
                "2001:db8::3" => {
                    std::thread::sleep(Duration::from_millis(300));
                    false
                }
                "10.0.0.2" => {
                    std::thread::sleep(Duration::from_millis(500));
                    true
                }
                _ => true,
            }
        }
        let v4 = |n: u8| SocketAddr::from(([10, 0, 0, n], 443));
        let v6 = |n: u16| SocketAddr::from(([0x2001, 0xdb8, 0, 0, 0, 0, 0, n], 443));
        let delay = Duration::from_millis(200);
        let give_up = Duration::from_secs(5);
        let mut addrs = [v6(1), v6(2), v4(9), v4(1)];
        let t = std::time::Instant::now();
        prefer_reachable(&mut addrs, delay, give_up, connect);
        assert!(t.elapsed() < Duration::from_secs(1), "{:?}", t.elapsed());
        // Tried 2001:db8::1, then 10.0.0.9 (refused at once), then 2001:db8::2.
        assert_eq!(addrs, [v6(2), v6(1), v4(9), v4(1)]);
        let mut addrs = [v4(9), v4(1)];
        prefer_reachable(&mut addrs, delay, give_up, connect);
        assert_eq!(addrs, [v4(1), v4(9)]);
        // Every attempt started and one failed: the race waits on for the last.
        let mut addrs = [v6(3), v4(2)];
        prefer_reachable(&mut addrs, delay, give_up, connect);
        assert_eq!(addrs, [v4(2), v6(3)]);

        // Nothing connects: the order stays, after every attempt failed or the race gave up.
        let mut addrs = [v4(9), v6(1)];
        prefer_reachable(&mut addrs, delay, Duration::from_millis(200), connect);
        assert_eq!(addrs, [v4(9), v6(1)]);
        let mut addrs = [v4(9), v4(9)];
        prefer_reachable(&mut addrs, delay, give_up, connect);
        assert_eq!(addrs, [v4(9), v4(9)]);
        let mut one = [v6(1)];
        prefer_reachable(&mut one, delay, give_up, |_, _| unreachable!());
        assert_eq!(one, [v6(1)]);
    }

    #[test]
    fn dist_urls_match_composer() {
        for (url, want) in [
            (
                "https://github.com/acme/lib/zipball/old",
                "https://api.github.com/repos/acme/lib/zipball/abc123",
            ),
            (
                "https://www.github.com/acme/lib/tarball/v1",
                "https://api.github.com/repos/acme/lib/tarball/abc123",
            ),
            (
                "https://github.com/acme/lib/archive/v1.2.zip",
                "https://api.github.com/repos/acme/lib/zipball/abc123",
            ),
            (
                "https://api.github.com/repos/acme/lib/zipball",
                "https://api.github.com/repos/acme/lib/zipball/abc123",
            ),
            (
                "https://api.github.com/repos/Acme/Lib/zipball/abc",
                "https://api.github.com/repos/Acme/Lib/zipball/abc123",
            ),
            (
                "https://bitbucket.org/acme/lib/get/v1.tar.gz",
                "https://bitbucket.org/acme/lib/get/abc123.tar.gz",
            ),
            (
                "https://gitlab.com/api/v4/projects/acme%2Flib/repository/archive.zip?sha=old",
                "https://gitlab.com/api/v4/projects/acme%2Flib/repository/archive.zip?sha=abc123",
            ),
            (
                "https://repo.example.com/dist/acme-lib-1.0.zip",
                "https://repo.example.com/dist/acme-lib-1.0.zip",
            ),
        ] {
            assert_eq!(dist_url(url, Some("abc123")), want, "{url}");
        }
        assert_eq!(
            dist_url("https://github.com/acme/lib/zipball/old", None),
            "https://github.com/acme/lib/zipball/old"
        );
    }

    fn serve(
        body: &'static [u8],
        requests: usize,
    ) -> (String, std::sync::mpsc::Receiver<Option<String>>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for stream in listener.incoming().take(requests) {
                let mut stream = stream.unwrap();
                let mut head = vec![];
                let mut byte = [0];
                while !head.ends_with(b"\r\n\r\n") && stream.read(&mut byte).unwrap() == 1 {
                    head.push(byte[0]);
                }
                let head = String::from_utf8_lossy(&head).into_owned();
                let auth = head.lines().find_map(|l| {
                    l.strip_prefix("authorization: ")
                        .or_else(|| l.strip_prefix("Authorization: "))
                });
                tx.send(auth.map(String::from)).unwrap();
                let response = if head.starts_with("GET /busy") {
                    b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                        .to_vec()
                } else if head.starts_with("GET /missing") {
                    b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                        .to_vec()
                } else {
                    [
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        )
                        .as_bytes(),
                        body,
                    ]
                    .concat()
                };
                stream.write_all(&response).unwrap();
            }
        });
        (base, rx)
    }

    fn package(url: &str, shasum: &str) -> Map<String, Value> {
        json!({"name": "Acme/Lib", "dist": {"type": "zip", "url": url, "reference": "", "shasum": shasum}}).as_object().unwrap().clone()
    }

    #[test]
    #[expect(
        clippy::too_many_lines,
        reason = "one test walks every case of the behaviour it checks"
    )]
    fn archives_are_checked_cached_and_authorized() {
        const BODY: &[u8] = b"archive bytes";
        let dir = scratch("archive");
        let (base, seen) = serve(BODY, 3);
        let origin = base.trim_start_matches("http://");
        std::fs::write(
            dir.join("auth.json"),
            format!(
                r#"{{"http-basic": {{"{origin}": {{"username": "user", "password": "pass"}}}}}}"#
            ),
        )
        .unwrap();
        let auth = Auth::load(&dir, None, None);
        let url = format!("{base}/a.zip");
        let fetcher = Fetcher::new(Some(dir.join("cache")), auth.clone(), false);

        assert_eq!(
            fetcher.archive(&package(&url, &sha1(BODY))).unwrap(),
            (BODY.to_vec(), false)
        );
        assert_eq!(seen.recv().unwrap().as_deref(), Some("Basic dXNlcjpwYXNz"));
        let cached = dir
            .join("cache/files/acme/lib")
            .join(format!("{}.zip", sha1(url.as_bytes())));
        assert_eq!(std::fs::read(&cached).unwrap(), BODY);
        assert_eq!(
            fetcher.archive(&package(&url, &sha1(BODY))).unwrap(),
            (BODY.to_vec(), true)
        );

        let error = format!("{:#}", fetcher.archive(&package(&url, "0000")).unwrap_err());
        assert!(
            error.contains("has sha1")
                && error.contains("composer.lock expects 0000; it may have been tampered with"),
            "{error}"
        );
        let error = format!(
            "{:#}",
            fetcher
                .archive(&package(&format!("{base}/missing"), ""))
                .unwrap_err()
        );
        assert!(error.contains("HTTP 404"), "{error}");

        let (base, seen) = serve(BODY, 7);
        let uncached = Fetcher::new(None, Auth::default(), false);
        let url = format!("{base}/a.zip");
        assert_eq!(
            uncached.archive(&package(&url, "")).unwrap(),
            (BODY.to_vec(), false)
        );
        let readonly = dir.join("readonly");
        std::fs::create_dir_all(readonly.join("files/acme/lib")).unwrap();
        let mode = std::os::unix::fs::PermissionsExt::from_mode(0o555);
        std::fs::set_permissions(readonly.join("files/acme/lib"), mode).unwrap();
        let fetcher = Fetcher::new(Some(readonly.clone()), Auth::default(), false);
        assert_eq!(
            fetcher.archive(&package(&url, "")).unwrap(),
            (BODY.to_vec(), false)
        );
        let mode = std::os::unix::fs::PermissionsExt::from_mode(0o755);
        std::fs::set_permissions(readonly.join("files/acme/lib"), mode).unwrap();
        std::fs::write(dir.join("not-a-dir"), "").unwrap();
        let unwritable = Fetcher::new(Some(dir.join("not-a-dir")), Auth::default(), false);
        assert_eq!(
            unwritable.archive(&package(&url, "")).unwrap(),
            (BODY.to_vec(), false)
        );
        let taken = dir.join("taken");
        let slot = taken
            .join("files/acme/lib")
            .join(format!("{}.zip", sha1(url.as_bytes())));
        std::fs::create_dir_all(slot.join("x")).unwrap();
        let fetcher = Fetcher::new(Some(taken), Auth::default(), false);
        assert_eq!(
            fetcher.archive(&package(&url, "")).unwrap(),
            (BODY.to_vec(), false)
        );
        assert_eq!(
            std::fs::read_dir(slot.parent().unwrap()).unwrap().count(),
            1
        );
        let error = format!(
            "{:#}",
            uncached
                .archive(&package(&format!("{base}/busy"), ""))
                .unwrap_err()
        );
        assert!(error.ends_with("/busy: HTTP 503"), "{error}");
        assert_eq!(seen.iter().take(7).count(), 7);
        let closed = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap();
        let url = format!("http://{closed}/a.zip");
        let error = format!("{:#}", uncached.archive(&package(&url, "")).unwrap_err());
        assert!(
            error.starts_with(&format!("downloading acme/lib from {url}: ")),
            "{error}"
        );

        let secure = Fetcher::new(None, auth, true);
        let error = format!("{:#}", secure.archive(&package(&url, "")).unwrap_err());
        assert_eq!(
            error,
            format!(
                "Your configuration does not allow connections to {url}. See https://getcomposer.org/doc/06-config.md#secure-http for details."
            )
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn redacts_urls_as_composer_hides_secrets() {
        for (url, want) in [
            (
                "https://a.test/p.zip?X-Amz-Signature=s3cr3t&x=1",
                "https://a.test/p.zip?***",
            ),
            ("https://u:s3cr3t@a.test/p.zip", "https://***@a.test/p.zip"),
            (
                "https://a.test:8443/p.zip#frag",
                "https://a.test:8443/p.zip?***",
            ),
            ("https://a.test?t=s3cr3t", "https://a.test?***"),
            ("https://a.test/p.zip", "https://a.test/p.zip"),
            ("../local", "../local"),
        ] {
            assert_eq!(redact(url), want, "{url}");
        }
        let closed = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap();
        let url = format!("http://u:s3cr3t@{closed}/a.zip?sig=s3cr3t");
        let fetcher = Fetcher::new(None, Auth::default(), false);
        let error = format!("{:#}", fetcher.archive(&package(&url, "")).unwrap_err());
        assert!(
            !error.contains("s3cr3t") && error.contains(&format!("***@{closed}/a.zip?***")),
            "{error}"
        );
        let error = format!(
            "{:#}",
            Fetcher::new(None, Auth::default(), true)
                .archive(&package(&url, ""))
                .unwrap_err()
        );
        assert!(!error.contains("s3cr3t"), "{error}");
    }

    #[test]
    fn credentials_by_host() {
        let dir = scratch("auth");
        std::fs::write(
            dir.join("auth.json"),
            r#"{"github-oauth": {"github.com": "gh", "github.acme.test": "ghe"}, "gitlab-token": {"gitlab.com": "gl", "git.acme.test": {"username": "u", "token": "t"}}, "bearer": {"repo.acme.test": "br"}}"#,
        )
        .unwrap();
        let home = scratch("auth-home");
        std::fs::write(
            home.join("auth.json"),
            r#"{"bearer": {"repo.acme.test": "home", "other.test": "o", "port.test:8443": "p"}}"#,
        )
        .unwrap();
        let env = r#"{"http-basic": {"basic.test": {"username": "ab", "password": "c"}, "skip.test": "x"}, "github-oauth": {"skip.test": 1}, "gitlab-token": {"skip.test": 1}, "bearer": {"skip.test": 1}}"#;
        let auth = Auth::load(&dir, Some(&home), Some(env.into()));
        assert_eq!(auth.header("https://basic.test/x"), Some("Basic YWI6Yw=="));
        assert_eq!(auth.header("https://skip.test/x"), None);
        assert_eq!(
            Auth::load(&dir, None, Some("not json".into())).header("https://other.test/x"),
            None
        );
        assert_eq!(
            auth.header("https://api.github.com/repos/a/b/zipball/c"),
            Some("token gh")
        );
        assert_eq!(auth.header("https://github.com/a/b"), Some("token gh"));
        assert_eq!(auth.header("https://github.acme.test/x"), Some("token ghe"));
        assert_eq!(
            auth.header("https://gitlab.com/api/v4/x"),
            Some("Bearer gl")
        );
        assert_eq!(auth.header("https://git.acme.test/x"), Some("Basic dTp0"));
        assert_eq!(
            auth.header("https://me@repo.acme.test/p.zip"),
            Some("Bearer br")
        );
        assert_eq!(auth.header("https://repo.acme.test:8443/p.zip"), None);
        assert_eq!(
            auth.header("https://port.test:8443/p.zip"),
            Some("Bearer p")
        );
        assert_eq!(auth.header("https://port.test/p.zip"), None);
        assert_eq!(auth.header("https://other.test/x"), Some("Bearer o"));
        assert_eq!(auth.header("https://packagist.org/x"), None);
        std::fs::remove_dir_all(&dir).unwrap();
        std::fs::remove_dir_all(&home).unwrap();
    }

    fn zip_archive() -> Vec<u8> {
        use zip::write::SimpleFileOptions;
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(vec![]));
        let file = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        w.add_directory("acme-lib-1a2b/", file).unwrap();
        w.start_file("acme-lib-1a2b/src/A.php", file).unwrap();
        w.write_all(b"<?php class A {}\n").unwrap();
        w.start_file("acme-lib-1a2b/bin/run", file.unix_permissions(0o755))
            .unwrap();
        w.write_all(b"#!/bin/sh\n").unwrap();
        w.add_symlink("acme-lib-1a2b/link.php", "src/A.php", file)
            .unwrap();
        w.add_symlink("acme-lib-1a2b/escape", "../../outside", file)
            .unwrap();
        w.start_file("../evil.php", file).unwrap();
        w.write_all(b"evil").unwrap();
        w.add_directory("__MACOSX/", file).unwrap();
        w.finish().unwrap().into_inner()
    }

    #[test]
    fn unpack_unwraps_and_stays_inside() {
        let dir = scratch("unpack");
        let target = dir.join("vendor/acme/lib");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("stale.php"), "old").unwrap();
        unpack("zip", &zip_archive(), &target).unwrap();
        assert_eq!(
            std::fs::read_to_string(target.join("src/A.php")).unwrap(),
            "<?php class A {}\n"
        );
        assert!(!target.join("stale.php").exists());
        assert_eq!(
            std::fs::metadata(target.join("bin/run"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o755
        );
        assert_eq!(
            std::fs::read_link(target.join("link.php")).unwrap(),
            Path::new("src/A.php")
        );
        assert!(std::fs::symlink_metadata(target.join("escape")).is_err());
        assert!(
            !dir.join("vendor/acme/evil.php").exists() && !dir.join("vendor/evil.php").exists()
        );
        assert_eq!(
            std::fs::read_dir(dir.join("vendor/acme")).unwrap().count(),
            1,
            "no temporary directory is left"
        );

        let mut tar = tar::Builder::new(flate2::write::GzEncoder::new(
            vec![],
            flate2::Compression::fast(),
        ));
        let mut h = tar::Header::new_gnu();
        h.set_size(5);
        h.set_mode(0o644);
        h.set_cksum();
        tar.append_data(&mut h, "pkg-v1/README", &b"hello"[..])
            .unwrap();
        let tgz = tar.into_inner().unwrap().finish().unwrap();
        unpack("tar", &tgz, &target).unwrap();
        assert_eq!(
            std::fs::read_to_string(target.join("README")).unwrap(),
            "hello"
        );
        assert!(!target.join("src").exists());
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(vec![]));
        let file = zip::write::SimpleFileOptions::default();
        w.start_file("a", file).unwrap();
        w.start_file("a/b", file).unwrap();
        let clash = w.finish().unwrap().into_inner();
        assert!(unpack("zip", &clash, &dir.join("vendor/acme/clash")).is_err());

        let mut tar = tar::Builder::new(vec![]);
        for name in ["README", "LICENSE"] {
            let mut h = tar::Header::new_gnu();
            h.set_size(5);
            h.set_mode(0o644);
            h.set_cksum();
            tar.append_data(&mut h, name, &b"hello"[..]).unwrap();
        }
        unpack("tar", &tar.into_inner().unwrap(), &target).unwrap();
        assert!(target.join("README").exists() && target.join("LICENSE").exists());
        for kind in ["rar", "xz"] {
            let error = unpack(kind, b"", &target).unwrap_err().to_string();
            assert_eq!(error, format!("dist type `{kind}` is not supported yet"));
        }

        let package_dir = dir.join("packages/x");
        std::fs::create_dir_all(&package_dir).unwrap();
        let link = dir.join("vendor/acme/x");
        link_path("packages/x", &dir, &link, false).unwrap();
        assert_eq!(
            std::fs::read_link(&link).unwrap(),
            Path::new("../../packages/x/")
        );
        link_path(&package_dir.to_string_lossy(), Path::new("/"), &link, false).unwrap();
        assert_eq!(
            std::fs::read_link(&link).unwrap(),
            Path::new("../../packages/x/")
        );
        let error = link_path("packages/gone", &dir, &link, false)
            .unwrap_err()
            .to_string();
        assert_eq!(error, "path repository packages/gone does not exist");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_path_repository_without_symlinks_is_copied() {
        let dir = scratch("mirror");
        let package_dir = dir.join("packages/x");
        let link = dir.join("vendor/acme/x");
        std::fs::create_dir_all(&package_dir).unwrap();
        link_path("packages/x", &dir, &link, false).unwrap();
        // `symlink: false` copies the package, without its .git or a link out of it, and replaces the old link.
        std::fs::create_dir_all(package_dir.join(".git")).unwrap();
        std::fs::create_dir_all(package_dir.join("src")).unwrap();
        std::fs::write(package_dir.join("src/A.php"), "a").unwrap();
        std::os::unix::fs::symlink("src/A.php", package_dir.join("in")).unwrap();
        std::os::unix::fs::symlink("../../vendor", package_dir.join("out")).unwrap();
        link_path("packages/x", &dir, &link, true).unwrap();
        assert!(std::fs::symlink_metadata(&link).unwrap().is_dir());
        std::fs::write(package_dir.join("src/A.php"), "changed").unwrap();
        assert_eq!(
            std::fs::read_to_string(link.join("src/A.php")).unwrap(),
            "a"
        );
        assert_eq!(
            std::fs::read_link(link.join("in")).unwrap(),
            Path::new("src/A.php")
        );
        assert!(
            !link.join(".git").exists() && std::fs::symlink_metadata(link.join("out")).is_err()
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Every link left in the package resolves inside it as installed: a root-level `..`, a link that only escapes
    /// once the wrapper is stripped, and a chain whose parts each look inside are dropped; links within stay.
    #[test]
    fn links_stay_inside_the_installed_package() {
        let dir = scratch("links");
        let target = dir.join("vendor/acme/lib");
        let zip = |links: &[(&str, &str)]| {
            let mut w = zip::ZipWriter::new(std::io::Cursor::new(vec![]));
            let file = zip::write::SimpleFileOptions::default();
            w.start_file("w/src/A.php", file).unwrap();
            w.write_all(b"a").unwrap();
            w.add_directory("w/d/e/", file).unwrap();
            for (name, to) in links {
                w.add_symlink(*name, *to, file).unwrap();
            }
            w.finish().unwrap().into_inner()
        };
        let bytes = zip(&[
            ("w/up", "../secret"),
            ("w/ok", "src/A.php"),
            ("w/d/e/ok", "../../src"),
            ("w/d/hop", "e/.."),
            ("w/d/chain", "hop/../../.."),
            ("w/d/abs", "/etc/passwd"),
        ]);
        unpack("zip", &bytes, &target).unwrap();
        let link = |n: &str| std::fs::symlink_metadata(target.join(n)).is_ok();
        assert!(link("ok") && link("d/e/ok") && link("d/hop"));
        assert!(!link("up") && !link("d/chain") && !link("d/abs"));
        assert_eq!(
            std::fs::read_to_string(target.join("d/e/ok/A.php")).unwrap(),
            "a"
        );

        // Without a wrapper, a root-level `..` leaves the package.
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(vec![]));
        let file = zip::write::SimpleFileOptions::default();
        w.start_file("README", file).unwrap();
        w.add_symlink("up", "../lib2", file).unwrap();
        unpack("zip", &w.finish().unwrap().into_inner(), &target).unwrap();
        assert!(target.join("README").exists() && !link("up"));

        // A sole symlink is not a wrapper: the package root stays a directory.
        std::fs::create_dir_all(dir.join("host")).unwrap();
        std::fs::write(dir.join("host/private"), "p").unwrap();
        let mut tar = tar::Builder::new(vec![]);
        let mut h = tar::Header::new_gnu();
        h.set_entry_type(tar::EntryType::Symlink);
        h.set_size(0);
        h.set_mode(0o777);
        tar.append_link(&mut h, "root", dir.join("host")).unwrap();
        unpack("tar", &tar.into_inner().unwrap(), &target).unwrap();
        assert!(std::fs::symlink_metadata(&target).unwrap().is_dir());
        assert!(!link("root") && !target.join("private").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A dist type `unpack` can't extract never reaches the cache, whose file name it would form.
    #[test]
    fn an_unsupported_dist_type_is_refused_before_the_cache() {
        let dir = scratch("kind");
        let fetcher = Fetcher::new(Some(dir.join("cache")), Auth::default(), true);
        let package = json!({"name": "a/b", "dist": {"type": "zip/../../../evil", "url": "https://a.test/x"}});
        let error = fetcher.archive(package.as_object().unwrap()).unwrap_err();
        assert_eq!(
            error.to_string(),
            "dist type `zip/../../../evil` is not supported yet"
        );
        assert!(!dir.join("cache").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
