//! `infamous-installer update`: bring an installed copy up to date with the GitHub repository that
//! holds the port's published tree (the output of `export`).
//!
//! 1. Resolve the selected release tag or branch to a commit and fetch its full file list. GitHub gives
//!    every file's git blob hash, so comparing it with the hash of the local file
//!    (`sha1("blob <len>\0" + bytes)`) says exactly which files differ, without downloading them.
//! 2. Download only those files (raw.githubusercontent.com), or the commit's zip if many changed,
//!    and verify each one against its blob hash before anything on disk is touched.
//! 3. Move them into place and delete files that an earlier update installed but the repository
//!    no longer has. `.installer/update.state` remembers the commit and the file list.
//!
//! No extra crates: HTTP goes through `curl.exe` and zips through `tar.exe`, both part of
//! Windows 10/11 (and of every Linux/macOS). A private repository works with GITHUB_TOKEN set.

use crate::util::{self, Ctx, Error, Result};
use crate::{bail, hash, manifest::Manifest};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// More changed files than this -> one zip download instead of one request per file.
const ZIP_THRESHOLD: usize = 150;
const INSTALL_LOCK_PATH: &str = "installer/.update.lock";

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Channel {
    #[default]
    Stable,
    BleedingEdge,
}

impl Channel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::BleedingEdge => "bleeding-edge",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Stable => "Stable",
            Self::BleedingEdge => "Bleeding Edge",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "stable" => Some(Self::Stable),
            "bleeding-edge" => Some(Self::BleedingEdge),
            _ => None,
        }
    }
}

pub const BUILD_CHANNEL_PATH: &str = "installer/build-channel";
const BLEEDING_EDGE_TAG_PREFIX: &str = "bleeding-edge-";
const INSTALLER_RELEASE_PATH: &str = "installer/bin/infamous-installer.exe";
const LAUNCHER_RELEASE_PATH: &str = "installer/bin/infamous-launch.exe";
const SETUP_ASSET_NAME: &str = "inFamous-Setup.exe";
const JSON_ACCEPT: Option<&str> = Some("application/vnd.github+json");

enum Source {
    Branch(String),
    Release(Channel),
}

pub struct Remote {
    pub repo: String,
    source: Source,
    /// Folder inside the repository that holds the tree ("" = repository root).
    pub path: String,
    token: Option<String>,
}

impl Remote {
    pub fn new(
        m: &Manifest,
        repo: Option<&str>,
        branch: Option<&str>,
        channel: Option<Channel>,
    ) -> Result<Remote> {
        let source = match (branch, channel) {
            (Some(_), Some(_)) => bail!("--branch and --channel cannot be used together"),
            (Some(branch), None) => Source::Branch(branch.into()),
            (None, channel) if m.update_releases => Source::Release(channel.unwrap_or_default()),
            (None, Some(Channel::BleedingEdge)) => {
                bail!("Bleeding Edge updates require [update] releases = true in installer/infamous.toml")
            }
            (None, _) => Source::Branch(m.update_branch.clone()),
        };
        let repo = repo
            .map(|r| r.to_string())
            .or_else(|| m.update_repo.clone())
            .ctx(|| "updating is not configured: installer/infamous.toml has no [update] repo (or pass --repo owner/name)")?;
        let repo = normalize_repo(&repo)?;
        let token = std::env::var("GITHUB_TOKEN")
            .ok()
            .filter(|t| !t.trim().is_empty());
        Ok(Remote {
            repo,
            source,
            path: m.update_path.clone(),
            token,
        })
    }

    pub fn source_label(&self) -> String {
        match &self.source {
            Source::Branch(branch) => format!("branch {branch}"),
            Source::Release(channel) => format!("{} channel", channel.label()),
        }
    }
}

/// "owner/name", also accepting a pasted https://github.com/owner/name(.git) URL.
fn normalize_repo(r: &str) -> Result<String> {
    let mut s = r.trim().trim_end_matches('/').to_string();
    for p in [
        "https://github.com/",
        "http://github.com/",
        "github.com/",
        "git@github.com:",
    ] {
        if let Some(x) = s.strip_prefix(p) {
            s = x.to_string();
        }
    }
    let s = s.trim_end_matches(".git").to_string();
    let parts: Vec<&str> = s.split('/').collect();
    if parts.len() != 2
        || parts.iter().any(|p| {
            p.is_empty()
                || !p
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
        })
    {
        bail!("'{r}' is not a GitHub repository (expected owner/name)");
    }
    Ok(s)
}

// ---------------------------------------------------------------------------------------------
// HTTP (curl)
// ---------------------------------------------------------------------------------------------

pub(crate) fn curl_program() -> Command {
    let prog = if cfg!(windows) {
        // The real curl.exe, never a PowerShell alias; it ships in System32 since Windows 10 1803.
        let sys = std::env::var("SystemRoot")
            .map(|r| PathBuf::from(r).join("System32").join("curl.exe"))
            .ok();
        match sys {
            Some(p) if p.is_file() => p,
            _ => PathBuf::from("curl.exe"),
        }
    } else {
        PathBuf::from("curl")
    };
    let mut c = Command::new(prog);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    c.stdin(Stdio::null());
    c
}

fn curl_cmd(remote: &Remote, url: &str, accept: Option<&str>, trusted_redirect: bool) -> Command {
    let mut c = curl_program();
    c.args([
        "-fsSL",
        "--retry",
        "3",
        "--retry-delay",
        "2",
        "--connect-timeout",
        "20",
        "-A",
        "infamous-installer",
    ]);
    if let Some(a) = accept {
        c.arg("-H").arg(format!("Accept: {a}"));
    }
    if let Some(t) = &remote.token {
        c.arg("-H")
            .arg(format!("Authorization: Bearer {}", t.trim()));
        if trusted_redirect {
            c.arg("--location-trusted");
        }
    }
    c.arg(url);
    c
}

fn http_err(url: &str, stderr: &[u8]) -> Error {
    let msg = String::from_utf8_lossy(stderr).trim().to_string();
    let hint = if msg.contains("404") {
        " (repository, branch or file not found -- a private repository needs GITHUB_TOKEN)"
    } else if msg.contains("403") || msg.contains("429") {
        " (GitHub refused or rate-limited the request; try again later)"
    } else {
        ""
    };
    Error(format!(
        "download failed: {url}: {}{hint}",
        if msg.is_empty() {
            "no response".into()
        } else {
            msg
        }
    ))
}

enum Metadata {
    Found(Vec<u8>),
    Missing(Error),
}

fn get_metadata(remote: &Remote, url: &str, accept: Option<&str>) -> Result<Metadata> {
    let out = curl_cmd(remote, url, accept, false)
        // Metadata also runs during launcher startup. Leave retries to the next check.
        .args([
            "--retry",
            "0",
            "--max-time",
            "15",
            "--write-out",
            "%{http_code}",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .ctx(|| "could not run curl (it is part of Windows 10 and 11)")?;
    if !out.status.success() {
        let err = http_err(url, &out.stderr);
        return if out.status.code() == Some(22) && out.stdout.ends_with(b"404") {
            Ok(Metadata::Missing(err))
        } else {
            Err(err)
        };
    }
    let mut body = out.stdout;
    body.truncate(body.len().saturating_sub(3));
    Ok(Metadata::Found(body))
}

fn get_bytes(remote: &Remote, url: &str, accept: Option<&str>) -> Result<Vec<u8>> {
    match get_metadata(remote, url, accept)? {
        Metadata::Found(body) => Ok(body),
        Metadata::Missing(err) => Err(err),
    }
}

fn get_file(remote: &Remote, url: &str, dest: &Path, trusted_redirect: bool) -> Result<()> {
    util::ensure_parent(dest)?;
    let out = curl_cmd(remote, url, None, trusted_redirect)
        .arg("-o")
        .arg(dest)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .ctx(|| "could not run curl (it is part of Windows 10 and 11)")?;
    if !out.status.success() {
        return Err(http_err(url, &out.stderr));
    }
    Ok(())
}

/// API / raw-file hosts; INFAMOUS_UPDATE_API / INFAMOUS_UPDATE_RAW override them (testing, mirrors).
fn api_base() -> String {
    std::env::var("INFAMOUS_UPDATE_API")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "https://api.github.com".into())
        .trim_end_matches('/')
        .to_string()
}
fn raw_base() -> String {
    std::env::var("INFAMOUS_UPDATE_RAW")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "https://raw.githubusercontent.com".into())
        .trim_end_matches('/')
        .to_string()
}

/// Percent-encode a repository path for a URL (keeps '/').
fn url_path(p: &str) -> String {
    let mut s = String::new();
    for b in p.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~/".contains(&b) {
            s.push(b as char);
        } else {
            s.push_str(&format!("%{b:02X}"));
        }
    }
    s
}

// ---------------------------------------------------------------------------------------------
// Minimal JSON (GitHub's tree listing)
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    pub fn get(&self, k: &str) -> Option<&Json> {
        match self {
            Json::Obj(v) => v.iter().find(|(a, _)| a == k).map(|(_, b)| b),
            _ => None,
        }
    }
    pub fn str(&self) -> Option<&str> {
        match self {
            Json::Str(s) => Some(s),
            _ => None,
        }
    }
}

pub fn parse_json(src: &[u8]) -> Result<Json> {
    let mut p = Parser { s: src, i: 0 };
    let v = p.value()?;
    p.ws();
    if p.i != p.s.len() {
        bail!("bad JSON: trailing data at byte {}", p.i);
    }
    Ok(v)
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn ws(&mut self) {
        while self.i < self.s.len() && matches!(self.s[self.i], b' ' | b'\t' | b'\n' | b'\r') {
            self.i += 1;
        }
    }
    fn err<T>(&self, what: &str) -> Result<T> {
        Err(Error(format!("bad JSON: {what} at byte {}", self.i)))
    }
    fn lit(&mut self, w: &str, v: Json) -> Result<Json> {
        if self.s[self.i..].starts_with(w.as_bytes()) {
            self.i += w.len();
            Ok(v)
        } else {
            self.err("unexpected word")
        }
    }
    fn value(&mut self) -> Result<Json> {
        self.ws();
        let Some(&c) = self.s.get(self.i) else {
            return self.err("unexpected end");
        };
        match c {
            b'{' => {
                self.i += 1;
                let mut v = Vec::new();
                self.ws();
                if self.s.get(self.i) == Some(&b'}') {
                    self.i += 1;
                    return Ok(Json::Obj(v));
                }
                loop {
                    self.ws();
                    if self.s.get(self.i) != Some(&b'"') {
                        return self.err("expected a key");
                    }
                    let k = self.string()?;
                    self.ws();
                    if self.s.get(self.i) != Some(&b':') {
                        return self.err("expected ':'");
                    }
                    self.i += 1;
                    let x = self.value()?;
                    v.push((k, x));
                    self.ws();
                    match self.s.get(self.i) {
                        Some(b',') => self.i += 1,
                        Some(b'}') => {
                            self.i += 1;
                            return Ok(Json::Obj(v));
                        }
                        _ => return self.err("expected ',' or '}'"),
                    }
                }
            }
            b'[' => {
                self.i += 1;
                let mut v = Vec::new();
                self.ws();
                if self.s.get(self.i) == Some(&b']') {
                    self.i += 1;
                    return Ok(Json::Arr(v));
                }
                loop {
                    v.push(self.value()?);
                    self.ws();
                    match self.s.get(self.i) {
                        Some(b',') => self.i += 1,
                        Some(b']') => {
                            self.i += 1;
                            return Ok(Json::Arr(v));
                        }
                        _ => return self.err("expected ',' or ']'"),
                    }
                }
            }
            b'"' => Ok(Json::Str(self.string()?)),
            b't' => self.lit("true", Json::Bool(true)),
            b'f' => self.lit("false", Json::Bool(false)),
            b'n' => self.lit("null", Json::Null),
            _ => {
                let st = self.i;
                while self.i < self.s.len()
                    && matches!(
                        self.s[self.i],
                        b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9'
                    )
                {
                    self.i += 1;
                }
                let t = std::str::from_utf8(&self.s[st..self.i]).unwrap_or("");
                match t.parse::<f64>() {
                    Ok(n) if !t.is_empty() => Ok(Json::Num(n)),
                    _ => self.err("unexpected character"),
                }
            }
        }
    }
    fn hex4(&mut self) -> Result<u32> {
        let Some(h) = self.s.get(self.i..self.i + 4) else {
            return self.err("short \\u escape");
        };
        let v = u32::from_str_radix(std::str::from_utf8(h).unwrap_or("x"), 16)
            .map_err(|_| Error(format!("bad JSON: bad \\u escape at byte {}", self.i)))?;
        self.i += 4;
        Ok(v)
    }
    fn string(&mut self) -> Result<String> {
        self.i += 1; // opening quote
        let mut out: Vec<u8> = Vec::new();
        loop {
            let Some(&c) = self.s.get(self.i) else {
                return self.err("unterminated string");
            };
            self.i += 1;
            match c {
                b'"' => break,
                b'\\' => {
                    let Some(&e) = self.s.get(self.i) else {
                        return self.err("bad escape");
                    };
                    self.i += 1;
                    match e {
                        b'"' => out.push(b'"'),
                        b'\\' => out.push(b'\\'),
                        b'/' => out.push(b'/'),
                        b'b' => out.push(8),
                        b'f' => out.push(12),
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        b'u' => {
                            let mut cp = self.hex4()?;
                            if (0xD800..0xDC00).contains(&cp)
                                && self.s.get(self.i..self.i + 2) == Some(b"\\u")
                            {
                                self.i += 2;
                                let lo = self.hex4()?;
                                cp = 0x10000
                                    + ((cp - 0xD800) << 10)
                                    + (lo.wrapping_sub(0xDC00) & 0x3FF);
                            }
                            let ch = char::from_u32(cp).unwrap_or('\u{FFFD}');
                            let mut b = [0u8; 4];
                            out.extend_from_slice(ch.encode_utf8(&mut b).as_bytes());
                        }
                        _ => return self.err("bad escape"),
                    }
                }
                _ => out.push(c),
            }
        }
        String::from_utf8(out).map_err(|_| Error("bad JSON: invalid UTF-8 in a string".into()))
    }
}

// ---------------------------------------------------------------------------------------------
// Plan
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Entry {
    /// Path relative to the install folder (forward slashes).
    pub path: String,
    /// Path inside the repository.
    pub repo_path: String,
    /// git blob hash (hex).
    pub sha: String,
    pub size: u64,
}

#[derive(Debug)]
pub struct Plan {
    pub commit: String,
    pub release_tag: Option<String>,
    pub fetch: Vec<Entry>,
    pub delete: Vec<String>,
    /// Every file the repository has (local paths), for the state file.
    pub all: Vec<String>,
    pub previous_commit: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Availability {
    Update,
    Repair,
    PendingBuild,
    OptionalUpscalers,
    Current,
}

impl Availability {
    pub fn action_label(self) -> Option<&'static str> {
        match self {
            Self::Update => Some("Install update"),
            Self::Repair => Some("Repair installation"),
            Self::PendingBuild => Some("Resume build"),
            Self::OptionalUpscalers => Some("Set up DLSS and FSR"),
            Self::Current => None,
        }
    }

    pub fn maintenance_note(self) -> Option<&'static str> {
        match self {
            Self::Repair => Some("This version is already installed. Repair restores changed or missing release files and removes obsolete files."),
            Self::PendingBuild => Some("The files are current. Resume the unfinished build."),
            Self::OptionalUpscalers => Some("The files are current. You can build optional DLSS and FSR support."),
            Self::Update | Self::Current => None,
        }
    }
}

impl Plan {
    pub fn is_empty(&self) -> bool {
        self.fetch.is_empty() && self.delete.is_empty()
    }
    pub fn download_bytes(&self) -> u64 {
        self.fetch.iter().map(|e| e.size).sum()
    }

    pub fn availability(&self, root: &Path) -> Availability {
        if !self.is_empty() {
            if self.previous_commit.as_ref() == Some(&self.commit) {
                Availability::Repair
            } else {
                Availability::Update
            }
        } else if pending_marker(root).is_file() {
            Availability::PendingBuild
        } else if crate::upscalers::needs_provisioning(root) {
            Availability::OptionalUpscalers
        } else {
            Availability::Current
        }
    }
}

pub fn pending_marker(root: &Path) -> PathBuf {
    root.join(".installer").join("update.pending")
}

/// Keep Setup and the command-line updater from changing the same installation concurrently.
pub fn install_lock(root: &Path) -> Result<Option<fs::File>> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_DELETE_ON_CLOSE;

        // The manifest directory already exists even when --force targets a fresh checkout.
        fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .share_mode(0)
            .custom_flags(FILE_FLAG_DELETE_ON_CLOSE)
            .open(root.join(INSTALL_LOCK_PATH))
            .map(Some)
            .ctx(|| "could not lock this installation for updating (another update may be running)")
    }
    #[cfg(not(windows))]
    {
        let _ = root;
        Ok(None)
    }
}

/// The rebuild is handed to the newly installed CLI without checking the release catalog again.
pub fn verify_pending(root: &Path, commit: &str) -> Result<()> {
    let marker = fs::read_to_string(pending_marker(root))
        .ctx(|| "the staged update has no pending rebuild")?;
    let state = UpdateState::load(root);
    if marker.trim() != commit || state.commit.as_deref() != Some(commit) {
        bail!("the staged update does not match version {commit}; check for updates again");
    }
    Ok(())
}

pub fn installed_cli(root: &Path) -> PathBuf {
    let cli = root.join("installer").join("infamous-installer");
    if cfg!(windows) {
        cli.with_extension("exe")
    } else {
        cli
    }
}

pub fn launcher_name(game_title: &str) -> String {
    format!("{game_title}.exe")
}

pub fn finish_command(root: &Path, commit: &str) -> Command {
    let mut command = Command::new(installed_cli(root));
    command.args(["update", "--finish-pending", "--expect-commit", commit]);
    command.arg("--root").arg(root).current_dir(root);
    command
}

/// `.installer/update.state`: the commit this folder was last updated to and the files it got.
struct UpdateState {
    commit: Option<String>,
    files: Vec<String>,
}

fn state_path(root: &Path) -> PathBuf {
    root.join(".installer").join("update.state")
}

impl UpdateState {
    fn load(root: &Path) -> UpdateState {
        let mut st = UpdateState {
            commit: None,
            files: Vec::new(),
        };
        if let Ok(t) = fs::read_to_string(state_path(root)) {
            for l in t.lines() {
                if let Some(c) = l.strip_prefix("commit ") {
                    st.commit = Some(c.trim().to_string());
                } else if let Some(f) = l.strip_prefix("file ") {
                    st.files.push(f.to_string());
                }
            }
        }
        st
    }
}

/// The setup program's record of the files it placed: merged into `update.state` so the next
/// update removes those the repository no longer has (an update never deletes anything else).
pub fn record_installed(root: &Path, files: &[String]) -> Result<()> {
    let st = UpdateState::load(root);
    let mut all = st.files;
    for f in files {
        if !all.contains(f) {
            all.push(f.clone());
        }
    }
    let commit = st.commit.unwrap_or_else(|| "setup".to_string());
    save_state(root, &commit, &all)
}

fn save_state(root: &Path, commit: &str, files: &[String]) -> Result<()> {
    let mut s = format!("# Written by `infamous-installer update`.\ncommit {commit}\n");
    for f in files {
        s.push_str(&format!("file {f}\n"));
    }
    util::write_file(&state_path(root), s.as_bytes())
}

fn blob_sha(data: &[u8]) -> String {
    let mut buf = format!("blob {}\0", data.len()).into_bytes();
    buf.extend_from_slice(data);
    hash::hex(&hash::sha1(&buf))
}

/// The git blob hash of a local file, or None if it does not exist / can't be read.
pub fn git_blob_sha(p: &Path) -> Option<String> {
    Some(blob_sha(&fs::read(p).ok()?))
}

fn crlf_to_lf(d: &[u8]) -> Vec<u8> {
    let mut o = Vec::with_capacity(d.len());
    let mut i = 0;
    while i < d.len() {
        if d[i] == b'\r' && d.get(i + 1) == Some(&b'\n') {
            i += 1;
            continue;
        }
        o.push(d[i]);
        i += 1;
    }
    o
}

/// Does the local file already hold this blob? A repository with `text=auto` stores text files
/// with LF line ends, while a Windows copy may have CRLF; both count as the same file.
pub fn local_matches(p: &Path, sha: &str) -> bool {
    let Ok(d) = fs::read(p) else { return false };
    if blob_sha(&d) == sha {
        return true;
    }
    !is_executable(p) && d.windows(2).any(|w| w == b"\r\n") && blob_sha(&crlf_to_lf(&d)) == sha
}

fn is_executable(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
}

/// Windows batch/PowerShell scripts are written with CRLF (cmd.exe misreads labels in LF files).
fn wants_crlf(path: &str) -> bool {
    let l = path.to_ascii_lowercase();
    cfg!(windows) && (l.ends_with(".cmd") || l.ends_with(".bat") || l.ends_with(".ps1"))
}

fn lf_to_crlf(d: &[u8]) -> Vec<u8> {
    let mut o = Vec::with_capacity(d.len() + d.len() / 16);
    for (i, &c) in d.iter().enumerate() {
        if c == b'\n' && (i == 0 || d[i - 1] != b'\r') {
            o.push(b'\r');
        }
        o.push(c);
    }
    o
}

/// Files an update must never write or delete, whatever the repository contains.
fn protected(m: &Manifest, p: &str) -> bool {
    let l = p.to_ascii_lowercase();
    let top = l.split('/').next().unwrap_or("");
    if l == INSTALL_LOCK_PATH || [".installer", ".git", ".github"].contains(&top) {
        return true;
    }
    if let Some(link) = &m.link_name {
        if top == link.to_ascii_lowercase() {
            return true;
        }
    }
    let port = m.port.to_ascii_lowercase();
    if let Some(local) = l
        .strip_prefix(&port)
        .and_then(|rest| rest.strip_prefix('/'))
    {
        if matches!(local, "settings.ini" | "controls.ini")
            || local.starts_with("gamedata/")
            || local.starts_with("user-backups/")
        {
            return true;
        }
    }
    // The binaries the setup program places next to the tree (not in the repository).
    l == "installer/infamous-installer.exe"
        || l == "installer/infamous-installer"
        || l == format!("{}.exe", m.game_title.to_ascii_lowercase())
        || l.starts_with("installer/target/")
        || m.is_generated(p)
}

pub fn is_commit_sha(commit: &str) -> bool {
    commit.len() == 40 && commit.bytes().all(|b| b.is_ascii_hexdigit())
}

fn published_release_tag(release: &Json, channel: Channel) -> Option<&str> {
    let prerelease = channel == Channel::BleedingEdge;
    if !matches!(release.get("draft"), Some(Json::Bool(false)))
        || !matches!(release.get("prerelease"), Some(Json::Bool(flag)) if *flag == prerelease)
    {
        return None;
    }
    let tag = release.get("tag_name")?.str()?;
    let published = release.get("published_at")?.str()?;
    if tag.trim().is_empty()
        || published.trim().is_empty()
        || tag.starts_with(BLEEDING_EDGE_TAG_PREFIX) != prerelease
    {
        return None;
    }
    Some(tag)
}

fn has_setup_asset(release: &Json) -> bool {
    matches!(release.get("assets"), Some(Json::Arr(assets)) if assets.iter().any(|asset| {
        asset.get("name").and_then(Json::str) == Some(SETUP_ASSET_NAME)
            && asset.get("state").and_then(Json::str) == Some("uploaded")
            && matches!(asset.get("size"), Some(Json::Num(size)) if *size > 0.0)
    }))
}

fn bleeding_edge_build_number(tag: &str) -> Option<(u64, u64)> {
    let (run, attempt) = tag
        .strip_prefix(BLEEDING_EDGE_TAG_PREFIX)?
        .split_once('-')?;
    let ordinal = |text: &str| {
        text.parse::<u64>()
            .ok()
            .filter(|n| *n > 0 && n.to_string() == text)
    };
    Some((ordinal(run)?, ordinal(attempt)?))
}

fn release_tag(r: &Remote, api: &str, channel: Channel) -> Result<String> {
    let missing = || format!("no {} release is published for {}", channel.label(), r.repo);
    if channel == Channel::Stable {
        let url = format!("{api}/releases/latest");
        let data = match get_metadata(r, &url, JSON_ACCEPT)? {
            Metadata::Found(data) => data,
            Metadata::Missing(err) => return Err(Error(format!("{}: {err}", missing()))),
        };
        let release = parse_json(&data)?;
        let tag = published_release_tag(&release, channel).ctx(missing)?;
        if !has_setup_asset(&release) {
            bail!(
                "no channel-enabled Stable build is published for {} yet (release {tag} is missing {SETUP_ASSET_NAME})",
                r.repo
            );
        }
        return Ok(tag.to_string());
    }

    const PER_PAGE: usize = 100;
    let mut newest: Option<((u64, u64), String)> = None;
    for page in 1.. {
        let url = format!("{api}/releases?per_page={PER_PAGE}&page={page}");
        let Json::Arr(releases) = parse_json(&get_bytes(r, &url, JSON_ACCEPT)?)? else {
            bail!("unexpected answer from GitHub (no list of releases)");
        };
        // An older run may finish later than a newer one. Order by the run number and attempt.
        if let Some((build, tag)) = releases
            .iter()
            .filter(|release| has_setup_asset(release))
            .filter_map(|release| {
                let tag = published_release_tag(release, channel)?;
                Some((bleeding_edge_build_number(tag)?, tag))
            })
            .max()
        {
            if newest
                .as_ref()
                .is_none_or(|(old_build, _)| build > *old_build)
            {
                newest = Some((build, tag.to_string()));
            }
        }
        if releases.len() < PER_PAGE {
            break;
        }
    }
    newest.map(|(_, tag)| tag).ctx(missing)
}

pub fn plan(root: &Path, m: &Manifest, r: &Remote) -> Result<Plan> {
    let api = format!("{}/repos/{}", api_base(), r.repo);
    let (reference, release) = match &r.source {
        Source::Branch(branch) => (branch.to_string(), None),
        Source::Release(channel) => {
            let tag = release_tag(r, &api, *channel)
                .ctx(|| format!("checking the {} channel", channel.label()))?;
            (format!("tags/{tag}"), Some((*channel, tag)))
        }
    };
    let source_label = match &release {
        Some((channel, tag)) => format!("{} release {tag}", channel.label()),
        None => r.source_label(),
    };
    let commit = get_bytes(
        r,
        &format!("{api}/commits/{}", url_path(&reference)),
        Some("application/vnd.github.sha"),
    )
    .ctx(|| format!("resolving {source_label}"))?;
    let commit = String::from_utf8_lossy(&commit).trim().to_string();
    if !is_commit_sha(&commit) {
        bail!(
            "GitHub did not return a commit for {} {source_label} (got '{}')",
            r.repo,
            commit.chars().take(60).collect::<String>()
        );
    }
    let tree = get_bytes(
        r,
        &format!("{api}/git/trees/{commit}?recursive=1"),
        JSON_ACCEPT,
    )?;
    let tree = parse_json(&tree)?;
    if matches!(tree.get("truncated"), Some(Json::Bool(true))) {
        bail!("the repository has too many files for GitHub's file list; updating needs a smaller tree");
    }
    let Some(Json::Arr(items)) = tree.get("tree") else {
        bail!("unexpected answer from GitHub (no file list)")
    };
    let prefix = if r.path.is_empty() {
        String::new()
    } else {
        format!("{}/", r.path)
    };

    let launcher = launcher_name(&m.game_title);
    let mut remote: BTreeMap<String, Entry> = BTreeMap::new();
    for it in items {
        if it.get("type").and_then(Json::str) != Some("blob")
            || it.get("mode").and_then(Json::str) == Some("120000")
        {
            continue; // folders, submodules, symlinks
        }
        let (Some(rp), Some(sha)) = (
            it.get("path").and_then(Json::str),
            it.get("sha").and_then(Json::str),
        ) else {
            continue;
        };
        let Some(local) = rp.strip_prefix(prefix.as_str()) else {
            continue;
        };
        // Prebuilt binaries the repository may carry: they replace the installer and the game's
        // launcher (which are not source files, so they are otherwise never touched).
        let local = match local.to_ascii_lowercase().as_str() {
            INSTALLER_RELEASE_PATH if cfg!(windows) => {
                "installer/infamous-installer.exe".to_string()
            }
            LAUNCHER_RELEASE_PATH if cfg!(windows) => launcher.clone(),
            l if l.starts_with("installer/bin/") => continue,
            _ => local.to_string(),
        };
        let binary = local.eq_ignore_ascii_case("installer/infamous-installer.exe")
            || local.eq_ignore_ascii_case(&launcher);
        let local = local.as_str();
        if local.is_empty()
            || local.split('/').any(|c| c == ".." || c.is_empty())
            || (!binary && protected(m, local))
        {
            continue;
        }
        let size = match it.get("size") {
            Some(Json::Num(n)) => *n as u64,
            _ => 0,
        };
        remote.insert(
            local.to_string(),
            Entry {
                path: local.to_string(),
                repo_path: rp.to_string(),
                sha: sha.to_ascii_lowercase(),
                size,
            },
        );
    }
    if remote.is_empty() {
        bail!(
            "{} ({source_label}{}) has no files to install",
            r.repo,
            if r.path.is_empty() {
                String::new()
            } else {
                format!(", folder {}", r.path)
            }
        );
    }
    if !remote.contains_key("installer/infamous.toml") {
        bail!(
            "{} does not look like this port's published tree (no installer/infamous.toml{}). Check [update] repo/path in installer/infamous.toml.",
            r.repo,
            if r.path.is_empty() { " at the repository root".to_string() } else { format!(" under {}", r.path) }
        );
    }
    if let Some((channel, _)) = &release {
        let marker = blob_sha(format!("{}\n", channel.as_str()).as_bytes());
        if remote
            .get(BUILD_CHANNEL_PATH)
            .is_none_or(|entry| entry.sha != marker)
        {
            bail!(
                "no channel-enabled {} build is published for {} yet ({source_label} is missing a matching {BUILD_CHANNEL_PATH})",
                channel.label(),
                r.repo
            );
        }
        if cfg!(windows) {
            for (local, path) in [
                ("installer/infamous-installer.exe", INSTALLER_RELEASE_PATH),
                (launcher.as_str(), LAUNCHER_RELEASE_PATH),
            ] {
                if remote.get(local).is_none_or(|entry| {
                    entry.repo_path != format!("{prefix}{path}")
                        || entry.size == 0
                        || !is_commit_sha(&entry.sha)
                }) {
                    bail!("{source_label} is missing the updater executable {path}");
                }
            }
        }
    }

    let fetch: Vec<Entry> = remote
        .values()
        .filter(|e| !local_matches(&util::join_slash(root, &e.path), &e.sha))
        .cloned()
        .collect();
    let st = UpdateState::load(root);
    // Files an earlier update or the setup program placed that the repository no longer has,
    // plus the manifest's [update] obsolete list (leftovers of older setups, e.g. the 1.0 setup's
    // private mod files, which made the first update's build fail).
    let mut delete: Vec<String> = Vec::new();
    for f in st.files.iter().chain(m.update_obsolete.iter()) {
        if !remote.contains_key(f)
            && !protected(m, f)
            && util::join_slash(root, f).is_file()
            && !delete.contains(f)
        {
            delete.push(f.clone());
        }
    }
    Ok(Plan {
        commit,
        release_tag: release.map(|(_, tag)| tag),
        fetch,
        delete,
        all: remote.into_keys().collect(),
        previous_commit: st.commit,
    })
}

/// The exact version and commit subjects the launcher presents before applying an update.
#[derive(Clone, Debug)]
pub struct Preview {
    pub commit: String,
    pub release_tag: Option<String>,
    pub availability: Availability,
    pub changes: Vec<String>,
    pub notes_status: Option<String>,
}

pub fn preview(root: &Path) -> Result<Preview> {
    preview_for_channel(root, crate::update_channel::preferred(root))
}

pub fn preview_for_channel(root: &Path, channel: Channel) -> Result<Preview> {
    let manifest = Manifest::load(&root.join("installer/infamous.toml"))?;
    let remote = Remote::new(&manifest, None, None, Some(channel))?;
    let plan = plan(root, &manifest, &remote)?;
    let availability = plan.availability(root);
    let (changes, notes_status) = if availability == Availability::Update {
        match change_notes(&remote, &plan) {
            Ok(notes) => notes,
            Err(_) => (
                Vec::new(),
                Some(
                    "GitHub change notes are unavailable. You can still install this update."
                        .into(),
                ),
            ),
        }
    } else {
        (
            Vec::new(),
            availability.maintenance_note().map(str::to_owned),
        )
    };
    Ok(Preview {
        commit: plan.commit,
        release_tag: plan.release_tag,
        availability,
        changes,
        notes_status,
    })
}

fn change_notes(r: &Remote, plan: &Plan) -> Result<(Vec<String>, Option<String>)> {
    let api = format!("{}/repos/{}", api_base(), r.repo);
    let target = &plan.commit;
    let base = plan.previous_commit.as_deref().filter(|c| is_commit_sha(c));
    let accept = JSON_ACCEPT;
    if let Some(base) = base {
        if base.eq_ignore_ascii_case(target) {
            return Ok((
                Vec::new(),
                Some("No new commits. The installer will repair files or finish preparing this version.".into()),
            ));
        }
        let url = format!("{api}/compare/{base}...{target}");
        if let Metadata::Found(comparison) = get_metadata(r, &url, accept)? {
            if let Some(notes) = incoming_notes(&parse_json(&comparison)?, base)? {
                return Ok(notes);
            }
        }
    }

    // Setup records a file list but does not always record the exact commit it packaged.
    let url = format!("{api}/commits?sha={target}&per_page=20");
    let Json::Arr(commits) = parse_json(&get_bytes(r, &url, accept)?)? else {
        bail!("unexpected answer from GitHub (no recent commits)");
    };
    let (changes, _) = commit_subjects(commits.iter());
    let baseline = if base.is_some() {
        "GitHub could not match the installed commit to the offered version's history."
    } else {
        "The installed commit was not recorded."
    };
    let description = if changes.is_empty() {
        "GitHub did not list any recent change notes for this version."
    } else {
        "Showing recent commits for the offered version."
    };
    Ok((changes, Some(format!("{baseline} {description}"))))
}

fn incoming_notes(comparison: &Json, base: &str) -> Result<Option<(Vec<String>, Option<String>)>> {
    match comparison.get("status").and_then(Json::str) {
        Some("ahead") => (),
        Some("behind" | "diverged") => return Ok(None),
        _ => bail!("unexpected answer from GitHub (no comparison status)"),
    }
    let ancestor = comparison
        .get("merge_base_commit")
        .and_then(|commit| commit.get("sha"))
        .and_then(Json::str);
    if !ancestor.is_some_and(|sha| sha.eq_ignore_ascii_case(base)) {
        bail!("unexpected answer from GitHub (the comparison base did not match)");
    }
    let Some(Json::Arr(commits)) = comparison.get("commits") else {
        bail!("unexpected answer from GitHub (no comparison commits)");
    };
    // The unpaginated comparison returns up to 250 of the most recent commits, oldest first.
    let (changes, partial) = commit_subjects(commits.iter().rev());
    let complete = matches!(
        comparison.get("total_commits"),
        Some(Json::Num(total)) if *total == commits.len() as f64
    );
    let status = if partial || !complete {
        Some("Showing a subset of the incoming commits.".into())
    } else if changes.is_empty() {
        Some(
            "The incoming commits did not list any changes beyond updater binary refreshes.".into(),
        )
    } else {
        None
    };
    Ok(Some((changes, status)))
}

fn commit_subjects<'a>(commits: impl Iterator<Item = &'a Json>) -> (Vec<String>, bool) {
    let mut subjects = Vec::new();
    let mut partial = false;
    for commit in commits {
        let subject = commit
            .get("commit")
            .and_then(|data| data.get("message"))
            .and_then(Json::str)
            .and_then(|message| message.lines().next())
            .map(str::trim);
        let Some(subject) = subject.filter(|s| !s.is_empty()) else {
            partial = true;
            continue;
        };
        if matches!(
            subject,
            "Refresh updater binaries from Setup app build"
                | "Keep updater binaries current after successful Windows builds"
        ) || subject.starts_with("Rebuilt installer and launcher binaries")
            || subject.starts_with("Distribute stable build ")
            || subject.starts_with("Distribute bleeding-edge build ")
        {
            continue;
        }
        if subjects.len() == 8 {
            return (subjects, true);
        }
        let mut chars = subject.chars();
        let mut display: String = chars.by_ref().take(160).collect();
        if chars.next().is_some() {
            display.push('\u{2026}');
        }
        subjects.push(display);
    }
    (subjects, partial)
}

// ---------------------------------------------------------------------------------------------
// Apply
// ---------------------------------------------------------------------------------------------

fn check_blob(p: &Path, e: &Entry) -> Result<()> {
    match git_blob_sha(p) {
        Some(h) if h == e.sha => Ok(()),
        Some(_) => bail!(
            "{} arrived damaged or changed on GitHub during the update; run the update again",
            e.path
        ),
        None => bail!("{} was not downloaded", e.path),
    }
}

fn find_zip_top(dir: &Path) -> Result<PathBuf> {
    let mut dirs = Vec::new();
    for d in fs::read_dir(dir).ctx(|| format!("reading {}", dir.display()))? {
        let d = d?;
        if d.file_type()?.is_dir() {
            dirs.push(d.path());
        }
    }
    if dirs.len() != 1 {
        bail!(
            "unexpected layout of the downloaded zip ({} top folders)",
            dirs.len()
        );
    }
    Ok(dirs.remove(0))
}

/// Download, verify and install the plan's files. `log` gets one line per step.
pub fn apply(
    root: &Path,
    r: &Remote,
    plan: &Plan,
    log: &mut dyn FnMut(String),
    stopped: &dyn Fn() -> bool,
) -> Result<()> {
    let work = root.join(".installer").join("update");
    let _ = fs::remove_dir_all(&work);
    for path in plan.all.iter().map(|file| util::join_slash(root, file)) {
        if is_executable(&path) && path.is_file() {
            cleanup_old_images(&path);
        }
    }
    fs::create_dir_all(&work).ctx(|| format!("creating {}", work.display()))?;

    // 1. Everything downloaded and verified before the install folder is touched.
    let mut staged: Vec<(PathBuf, &Entry)> = Vec::new();
    if plan.fetch.len() > ZIP_THRESHOLD {
        let zip = work.join("update.zip");
        log(format!(
            "Downloading the whole update as one zip ({} files changed)",
            plan.fetch.len()
        ));
        get_file(
            r,
            &format!("{}/repos/{}/zipball/{}", api_base(), r.repo, plan.commit),
            &zip,
            true,
        )?;
        let x = work.join("x");
        fs::create_dir_all(&x)?;
        log("Unpacking".into());
        let tar = if cfg!(windows) {
            std::env::var("SystemRoot")
                .map(|s| PathBuf::from(s).join("System32").join("tar.exe"))
                .ok()
                .filter(|p| p.is_file())
                .unwrap_or_else(|| PathBuf::from("tar.exe"))
        } else {
            PathBuf::from("tar")
        };
        let mut c = if cfg!(windows) {
            let mut c = Command::new(tar); // bsdtar: reads zips
            c.arg("-xf").arg(&zip).arg("-C").arg(&x);
            c
        } else {
            let _ = tar;
            let mut c = Command::new("unzip"); // GNU tar can't read zips
            c.arg("-q").arg("-o").arg(&zip).arg("-d").arg(&x);
            c
        };
        c.stdin(Stdio::null()).stdout(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            c.creation_flags(0x0800_0000);
        }
        let out = c
            .stderr(Stdio::piped())
            .output()
            .ctx(|| "could not run tar (it is part of Windows 10 and 11)")?;
        if !out.status.success() {
            bail!(
                "unpacking the update failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        let top = find_zip_top(&x)?;
        for e in &plan.fetch {
            let p = util::join_slash(&top, &e.repo_path);
            check_blob(&p, e)?;
            staged.push((p, e));
        }
    } else {
        let n = plan.fetch.len();
        for (i, e) in plan.fetch.iter().enumerate() {
            if stopped() {
                bail!("cancelled");
            }
            log(format!("[{}/{n}] {}", i + 1, e.path));
            let p = work.join("f").join(format!("{i}"));
            get_file(
                r,
                &format!(
                    "{}/{}/{}/{}",
                    raw_base(),
                    r.repo,
                    plan.commit,
                    url_path(&e.repo_path)
                ),
                &p,
                false,
            )?;
            check_blob(&p, e)?;
            staged.push((p, e));
        }
    }

    if stopped() {
        bail!("cancelled");
    }
    // 2. Into place. Keep the marker if installation is interrupted or the rebuild fails.
    util::write_file(&pending_marker(root), plan.commit.as_bytes())?;
    for (src, e) in &staged {
        let dst = util::join_slash(root, &e.path);
        util::ensure_parent(&dst)?;
        if wants_crlf(&e.path) {
            let d = fs::read(src)?;
            fs::write(src, lf_to_crlf(&d)).ctx(|| format!("staging {}", dst.display()))?;
        }
        replace_staged(src, &dst)?;
    }
    for f in &plan.delete {
        let p = util::join_slash(root, f);
        match fs::remove_file(&p) {
            Ok(()) => log(format!("removed {f}")),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e).ctx(|| format!("removing {}", p.display())),
        }
    }
    save_state(root, &plan.commit, &plan.all)?;
    let _ = fs::remove_dir_all(&work);
    Ok(())
}

fn cleanup_old_images(exe: &Path) {
    let first = exe.with_extension("old");
    let _ = fs::remove_file(&first);
    let Some(stem) = first.file_name().and_then(|name| name.to_str()) else {
        return;
    };
    let prefix = format!("{stem}-");
    if let Some(dir) = exe.parent() {
        for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
            let name = entry.file_name();
            let Some(suffix) = name.to_str().and_then(|name| name.strip_prefix(&prefix)) else {
                continue;
            };
            let Some((pid, index)) = suffix.split_once('-') else {
                continue;
            };
            if pid.parse::<u32>().is_ok() && index.parse::<u32>().is_ok() {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
}

fn replace_staged(src: &Path, dst: &Path) -> Result<()> {
    let Err(error) = fs::rename(src, dst) else {
        return Ok(());
    };
    if !cfg!(windows) || !is_executable(dst) || !dst.is_file() {
        return Err(error).ctx(|| format!("replacing {}", dst.display()));
    }
    // Windows allows renaming an executing image. Keep the old image until it exits and
    // restore its original name if the staged move fails.
    let mut old = dst.with_extension("old");
    let pid = std::process::id();
    let mut index = 0u32;
    while old.try_exists()? {
        index += 1;
        old = dst.with_extension(format!("old-{pid}-{index}"));
    }
    fs::rename(dst, &old).ctx(|| format!("backing up {}", dst.display()))?;
    if let Err(error) = fs::rename(src, dst) {
        fs::rename(&old, dst).ctx(|| {
            format!(
                "restoring {} after replacement failed: {error}",
                dst.display()
            )
        })?;
        return Err(error).ctx(|| format!("replacing {}", dst.display()));
    }
    Ok(())
}

/// Which of the plan's files are worth naming in a summary.
pub fn summary(plan: &Plan) -> String {
    let mut groups: BTreeMap<&str, usize> = BTreeMap::new();
    for e in &plan.fetch {
        *groups
            .entry(e.path.split('/').next().unwrap_or(""))
            .or_default() += 1;
    }
    let mut parts: Vec<String> = groups.iter().map(|(g, n)| format!("{g} {n}")).collect();
    if !plan.delete.is_empty() {
        parts.push(format!("{} removed", plan.delete.len()));
    }
    parts.join(", ")
}

/// Distinct top-level folders touched (for deciding what needs rebuilding).
pub fn touched(plan: &Plan) -> BTreeSet<String> {
    plan.fetch
        .iter()
        .map(|e| e.path.clone())
        .chain(plan.delete.iter().cloned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distinguishes_release_changes_from_repair_and_local_build_work() {
        let root = std::env::temp_dir().join(format!("infamous-check-{}", std::process::id()));
        let mut plan = Plan {
            commit: "a".repeat(40),
            previous_commit: Some("b".repeat(40)),
            release_tag: None,
            fetch: vec![],
            delete: vec![],
            all: vec![],
        };
        // A missing optional SDK is common on Windows, even for a current installation.
        assert_eq!(
            plan.availability(&root),
            if cfg!(windows) {
                Availability::OptionalUpscalers
            } else {
                Availability::Current
            }
        );
        let marker = pending_marker(&root);
        util::write_file(&marker, plan.commit.as_bytes()).unwrap();
        assert_eq!(plan.availability(&root), Availability::PendingBuild);

        // A deletion-only release still changes installed files. With the same commit
        // recorded locally, the same difference instead calls for a repair.
        plan.delete.push("obsolete-file".into());
        assert_eq!(plan.availability(&root), Availability::Update);
        plan.previous_commit = Some(plan.commit.clone());
        assert_eq!(plan.availability(&root), Availability::Repair);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn json_tree() {
        let j = parse_json(br#"{"sha":"ab","tree":[{"path":"a b/\u00e9.txt","mode":"100644","type":"blob","sha":"x","size":12},{"path":"d","type":"tree"}],"truncated":false}"#).unwrap();
        let Some(Json::Arr(t)) = j.get("tree") else {
            panic!()
        };
        assert_eq!(t[0].get("path").and_then(Json::str), Some("a b/\u{e9}.txt"));
        assert!(matches!(t[0].get("size"), Some(Json::Num(n)) if *n == 12.0));
        assert!(matches!(j.get("truncated"), Some(Json::Bool(false))));
    }

    #[test]
    fn blob_hash_matches_git() {
        // `printf 'hello\n' | git hash-object --stdin` = ce013625030ba8dba906f756967f9e9ca394464a
        let d = std::env::temp_dir().join("ifupd_blob_test.txt");
        fs::write(&d, b"hello\n").unwrap();
        assert_eq!(
            git_blob_sha(&d).as_deref(),
            Some("ce013625030ba8dba906f756967f9e9ca394464a")
        );
        let _ = fs::remove_file(&d);
    }

    #[test]
    fn line_ends() {
        let d = std::env::temp_dir().join("ifupd_eol_test.txt");
        let exe = d.with_extension("exe");
        fs::write(&d, b"hello\r\n").unwrap();
        assert!(local_matches(
            &d,
            "ce013625030ba8dba906f756967f9e9ca394464a"
        )); // blob of "hello\n"
        assert!(!local_matches(
            &d,
            "0000000000000000000000000000000000000000"
        ));
        fs::write(&exe, b"hello\r\n").unwrap();
        assert!(!local_matches(
            &exe,
            "ce013625030ba8dba906f756967f9e9ca394464a"
        ));
        let _ = fs::remove_file(&exe);
        let _ = fs::remove_file(&d);
        assert_eq!(lf_to_crlf(b"a\nb\r\nc\n"), b"a\r\nb\r\nc\r\n");
        assert_eq!(crlf_to_lf(b"a\r\nb\rc"), b"a\nb\rc");
    }

    #[cfg(windows)]
    #[test]
    fn repeated_replacements_keep_all_running_images() {
        use std::io::Read;

        const HOLD_IMAGE: &str = "INFAMOUS_TEST_HOLD_RUNNING_IMAGE";
        if std::env::var_os(HOLD_IMAGE).is_some() {
            let _ = std::io::stdin().read(&mut [0u8]);
            return;
        }

        let root =
            std::env::temp_dir().join(format!("infamous-running-images-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let exe = root.join("launcher.exe");
        let staged = root.join("staged");
        let launch = || {
            let mut child = Command::new(&exe)
                .args([
                    "--exact",
                    "update::tests::repeated_replacements_keep_all_running_images",
                ])
                .env(HOLD_IMAGE, "1")
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .spawn()
                .unwrap();
            let input = child.stdin.take().unwrap();
            (child, input)
        };

        let mut bytes = fs::read(std::env::current_exe().unwrap()).unwrap();
        fs::write(&exe, &bytes).unwrap();
        let v1 = blob_sha(&bytes);
        let (mut first, first_input) = launch();
        bytes.extend_from_slice(b"second version");
        fs::write(&staged, &bytes).unwrap();
        replace_staged(&staged, &exe).unwrap();

        let v2 = blob_sha(&bytes);
        let (mut second, second_input) = launch();
        bytes.extend_from_slice(b"third version");
        fs::write(&staged, &bytes).unwrap();
        cleanup_old_images(&exe);
        replace_staged(&staged, &exe).unwrap();
        let first_backup = exe.with_extension("old");
        let second_backup = exe.with_extension(format!("old-{}-1", std::process::id()));
        assert_eq!(git_blob_sha(&first_backup).as_deref(), Some(v1.as_str()));
        assert_eq!(git_blob_sha(&second_backup).as_deref(), Some(v2.as_str()));
        assert_eq!(
            git_blob_sha(&exe).as_deref(),
            Some(blob_sha(&bytes).as_str())
        );

        drop(first_input);
        assert!(first.wait().unwrap().success());
        cleanup_old_images(&exe);
        assert!(!first_backup.exists());
        assert!(second_backup.exists());

        drop(second_input);
        assert!(second.wait().unwrap().success());
        cleanup_old_images(&exe);
        assert!(!second_backup.exists());
        fs::remove_file(&exe).unwrap();
        fs::remove_dir(&root).unwrap();
    }

    #[test]
    fn repo_names() {
        assert_eq!(
            normalize_repo("https://github.com/whompay/inFAMOUSreborn").unwrap(),
            "whompay/inFAMOUSreborn"
        );
        assert_eq!(
            normalize_repo("whompay/inFAMOUSreborn.git").unwrap(),
            "whompay/inFAMOUSreborn"
        );
        assert!(normalize_repo("nope").is_err());
        assert_eq!(url_path("a b/c+d.txt"), "a%20b/c%2Bd.txt");
    }
}

#[cfg(test)]
mod manifest_tests {
    #[test]
    fn export_retains_required_build_inputs_but_excludes_private_files() {
        let m = shipped_manifest();
        let root =
            std::env::temp_dir().join(format!("infamous-export-policy-{}", std::process::id()));
        let keep = [
            "installer/pgo/infamous.profdata",
            "installer/pgo/infamous-us.profdata",
            "ps3recomp/libs/system/savedata_slots.h",
            "ps3recomp/libs/system/savedata_slot_dialog.h",
        ];
        let private = [
            "infamous-recomp/shadercache-before-test/cache.dat",
            "infamous-recomp/shadercache-after-test/cache.dat",
            "dlc/test.pkg",
            "dlc/test.rap",
            "dlc/test.edat.rifkey",
            "dlc/test.edat",
            "dlc/test.edat.dec",
            "disc/game.dkey",
            "disc/game.iso",
            "user/savedata-backup/BCUS98119/STATE",
            "infamous-recomp/recompiled/game.cpp",
        ];
        for path in keep.iter().chain(private.iter()) {
            crate::util::write_file(&root.join(path), b"test fixture").unwrap();
        }
        let files = crate::pipeline::export_files(&m, &root, &[]).unwrap();
        for path in keep {
            assert!(files.iter().any(|p| p == path), "missing {path}");
        }
        for path in private {
            assert!(!files.iter().any(|p| p == path), "leaked {path}");
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    fn shipped_manifest() -> crate::manifest::Manifest {
        crate::manifest::Manifest::load(std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/infamous.toml"
        )))
        .unwrap()
    }

    #[test]
    fn shipped_manifest_has_update_section() {
        let m = shipped_manifest();
        assert_eq!(
            m.update_repo.as_deref(),
            Some("WoofahRayetCode/inFAMOUS-US")
        );
        assert_eq!(m.update_branch, "main");
        assert_eq!(m.update_path, "");
        for binary in [
            "installer/bin/infamous-installer.exe",
            "installer/bin/infamous-launch.exe",
        ] {
            assert!(
                crate::util::any_match(&m.export_keep, binary),
                "{binary} must be published for self-update"
            );
        }
    }

    #[test]
    fn shipped_spu_lift_overwrites_patched_outputs() {
        let m = shipped_manifest();
        let step = m.steps.iter().find(|s| s.id == "spu-lift").unwrap();
        assert!(
            step.run.iter().any(|arg| arg == "--relift"),
            "spu-lift must overwrite previously patched sources during --regen/update"
        );
    }

    #[test]
    fn shipped_player_build_uses_pgo_with_shipped_profile() {
        // PGO=USE is safe on any clang-cl: infamous-recomp/CMakeLists.txt probes the profile
        // with the real compiler and falls back to a plain build if it can't read it.
        let m = shipped_manifest();
        assert!(m.build_configure.iter().any(|arg| arg == "-DPORT_PGO=USE"));
        assert!(m
            .build_configure
            .iter()
            .any(|arg| arg.starts_with("-DPORT_PGO_PROFILE=")
                && arg.ends_with("pgo/infamous.profdata")));
        for profile in [
            "installer/pgo/infamous.profdata",
            "installer/pgo/infamous-us.profdata",
        ] {
            assert!(
                crate::util::any_match(&m.export_keep, profile),
                "{profile} must be published"
            );
        }
    }
}

#[cfg(test)]
mod v246_tests {
    use super::*;

    #[test]
    fn setup_file_list_is_merged_into_the_update_state() {
        let d = std::env::temp_dir().join(format!("upd-state-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        // fresh install: the setup's list becomes the baseline
        record_installed(&d, &["a/x.c".to_string(), "b.txt".to_string()]).unwrap();
        let st = UpdateState::load(&d);
        assert_eq!(st.commit.as_deref(), Some("setup"));
        assert_eq!(st.files, vec!["a/x.c", "b.txt"]);
        // an update recorded its commit; re-running setup keeps it and adds new names once
        save_state(
            &d,
            "0123456789abcdef0123456789abcdef01234567",
            &["b.txt".to_string()],
        )
        .unwrap();
        record_installed(&d, &["b.txt".to_string(), "c/skate.cpp".to_string()]).unwrap();
        let st = UpdateState::load(&d);
        assert_eq!(
            st.commit.as_deref(),
            Some("0123456789abcdef0123456789abcdef01234567")
        );
        assert_eq!(st.files, vec!["b.txt", "c/skate.cpp"]);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn shipped_manifest_forbids_the_private_mod_and_lists_its_leftovers() {
        let m = crate::manifest::Manifest::load(std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/infamous.toml"
        )))
        .unwrap();
        for f in [
            "infamous-recomp/skate_mod.cpp",
            "infamous-recomp/skate_passthrough.h",
            "infamous-recomp/mods/skate/README.md",
            "ps3recomp/libs/video/rsx_skate_actor.c",
            "ps3recomp/libs/video/rsx_skate_actor.h",
            "infamous-recomp/tools/bench/skate_drive.ps1",
            "infamous-recomp/HANDOFF_v246.md",
            "infamous-recomp/handoff_private.txt",
            "infamous-recomp/AGENT_NOTES.rst",
            "infamous-recomp/coord_session.md",
            "infamous-recomp/thread_perf_notes.txt",
            "INFAMOUS_CODE_PROVENANCE_AUDIT.md",
            "infamous-recomp/gamedata-test/dev_hdd0/home/00000001/savedata/BLES00000_INFAMOUS_0/STATE",
        ] {
            assert_eq!(crate::pipeline::forbidden(&m, &[f.to_string()]).len(), 1, "{f} must be forbidden");
            assert!(crate::util::any_match(&m.export_exclude, f), "{f} must be excluded");
            assert!(!crate::util::any_match(&m.export_keep, f), "{f} must not be kept");
        }
        for f in [
            "infamous-recomp/trainer.cpp",
            "ps3recomp/libs/video/rsx_d3d12_abi.h",
            "infamous-recomp/CMakeLists.txt",
            "ps3recomp/CMakeLists.txt",
            "infamous-recomp/stubs.cpp",
        ] {
            assert!(
                crate::pipeline::forbidden(&m, &[f.to_string()]).is_empty(),
                "{f} is needed"
            );
            assert!(
                !crate::util::any_match(&m.export_exclude, f)
                    || crate::util::any_match(&m.export_keep, f),
                "{f} must be exported"
            );
        }
        for f in [
            "infamous-recomp/settings.ini",
            "infamous-recomp/play_pgo.cmd",
            "infamous-recomp/_bgtest/A_textures_off.cmd",
            "infamous-recomp/memcmd.txt.out",
            "infamous-recomp/tools/v246A_build.cmd",
        ] {
            assert!(
                crate::util::any_match(&m.export_exclude, f)
                    && !crate::util::any_match(&m.export_keep, f),
                "{f} must be excluded"
            );
        }
        assert!(
            !crate::util::any_match(&m.export_exclude, "infamous-recomp/build.cmd")
                || crate::util::any_match(&m.export_keep, "infamous-recomp/build.cmd")
        );
        assert!(m
            .update_obsolete
            .iter()
            .any(|f| f == "ps3recomp/libs/video/rsx_skate_actor.c"));
        assert!(m
            .update_obsolete
            .iter()
            .all(|f| crate::pipeline::forbidden(&m, &[f.clone()]).len() == 1));
    }
}
