//! In-app updates from this repo's GitHub Releases.
//! - check(): asks the GitHub API for the latest release and compares versions
//! - install(): downloads the exe, verifies the SHA256 written in the release notes,
//!   swaps it with the running exe (Windows allows renaming a running exe) and relaunches.
use regex::Regex;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

const REPO: &str = "rafaelvons/rbx-account-manager";
const ASSET: &str = "RBXAccountManager.exe";

#[derive(Clone, Debug)]
pub struct Release {
    pub tag: String,
    pub url: String,
    pub size: u64,
    pub sha256: Option<String>,
    pub page: String,
}

/// "v2.10.1" -> (2, 10, 1). Missing parts count as 0.
pub fn parse_version(s: &str) -> Option<(u32, u32, u32)> {
    let s = s.trim().trim_start_matches(['v', 'V']);
    let mut it = s.split(['.', '-', '+']).map(|p| p.parse::<u32>());
    let major = it.next()?.ok()?;
    let minor = it.next().and_then(|r| r.ok()).unwrap_or(0);
    let patch = it.next().and_then(|r| r.ok()).unwrap_or(0);
    Some((major, minor, patch))
}

pub fn current_version() -> (u32, u32, u32) {
    parse_version(env!("CARGO_PKG_VERSION")).unwrap_or((0, 0, 0))
}

pub fn is_newer(tag: &str) -> bool {
    parse_version(tag).map(|v| v > current_version()).unwrap_or(false)
}

/// SHA256 hex written in the release notes ("SHA256  <64 hex>").
pub fn sha_from_notes(body: &str) -> Option<String> {
    Regex::new(r"(?i)SHA-?256\W+([0-9a-f]{64})").unwrap().captures(body).map(|c| c[1].to_lowercase())
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

fn agent(read_timeout: u64) -> Result<ureq::Agent, String> {
    let tls = native_tls::TlsConnector::new().map_err(|e| format!("TLS error: {e}"))?;
    Ok(ureq::AgentBuilder::new()
        .tls_connector(Arc::new(tls))
        .redirects(5) // release downloads redirect to GitHub's file CDN
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(read_timeout))
        .user_agent(concat!("RBXAccountManager/", env!("CARGO_PKG_VERSION")))
        .build())
}

/// The latest release if it is newer than this build.
pub fn check() -> Result<Option<Release>, String> {
    let a = agent(20)?;
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let resp = match a.get(&url).set("Accept", "application/vnd.github+json").call() {
        Ok(r) => r,
        Err(ureq::Error::Status(404, _)) => return Ok(None),
        Err(ureq::Error::Status(c, _)) => return Err(format!("GitHub returned HTTP {c}.")),
        Err(e) => return Err(format!("Couldn't reach GitHub: {e}")),
    };
    let v: serde_json::Value = serde_json::from_str(&resp.into_string().map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let tag = v["tag_name"].as_str().unwrap_or_default().to_string();
    if tag.is_empty() || !is_newer(&tag) || v["draft"].as_bool().unwrap_or(false) || v["prerelease"].as_bool().unwrap_or(false) {
        return Ok(None);
    }
    let asset = v["assets"].as_array().and_then(|a| a.iter().find(|x| x["name"].as_str() == Some(ASSET)).cloned());
    let Some(asset) = asset else { return Ok(None) };
    Ok(Some(Release {
        tag,
        url: asset["browser_download_url"].as_str().unwrap_or_default().to_string(),
        size: asset["size"].as_u64().unwrap_or(0),
        sha256: sha_from_notes(v["body"].as_str().unwrap_or_default()),
        page: v["html_url"].as_str().unwrap_or_default().to_string(),
    }))
}

/// Download a release exe and check it. `progress` gets 0.0..=1.0.
pub fn download(rel: &Release, progress: &dyn Fn(f32)) -> Result<Vec<u8>, String> {
    if !rel.url.starts_with("https://github.com/") {
        return Err("Unexpected download address.".into());
    }
    let a = agent(60)?;
    let resp = a.get(&rel.url).call().map_err(|e| format!("Download failed: {e}"))?;
    let total = resp.header("Content-Length").and_then(|s| s.parse::<u64>().ok()).unwrap_or(rel.size).max(1);
    let mut reader = resp.into_reader().take(64 * 1024 * 1024);
    let mut bytes = Vec::with_capacity(total as usize);
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buf).map_err(|e| format!("Download failed: {e}"))?;
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&buf[..n]);
        progress((bytes.len() as f32 / total as f32).min(1.0));
    }
    if rel.size > 0 && bytes.len() as u64 != rel.size {
        return Err("The download is incomplete. Try again.".into());
    }
    if bytes.len() < 2 || &bytes[..2] != b"MZ" {
        return Err("The download isn't a Windows program.".into());
    }
    match &rel.sha256 {
        Some(expected) if sha256_hex(&bytes) != *expected => Err("The download doesn't match the SHA256 in the release notes, so it wasn't installed.".into()),
        _ => Ok(bytes),
    }
}

fn old_path(exe: &Path) -> PathBuf {
    exe.with_extension("exe.old")
}

/// Replace `target` with `bytes`: target -> target.exe.old, new bytes -> target.
/// Works while `target` is running. Rolls back if writing the new file fails.
pub fn swap_in(target: &Path, bytes: &[u8]) -> Result<(), String> {
    let tmp = target.with_extension("exe.new");
    std::fs::write(&tmp, bytes).map_err(|e| format!("Couldn't write the update: {e}"))?;
    let old = old_path(target);
    let _ = std::fs::remove_file(&old);
    std::fs::rename(target, &old).map_err(|e| format!("Couldn't replace the app file: {e}"))?;
    if let Err(e) = std::fs::rename(&tmp, target) {
        let _ = std::fs::rename(&old, target);
        return Err(format!("Couldn't replace the app file: {e}"));
    }
    Ok(())
}

/// Install the downloaded exe over the running one and start it ~2 s later,
/// after this process has closed (so even versions without the startup wait can open).
pub fn install_and_restart(bytes: &[u8]) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    swap_in(&exe, bytes)?;
    let line = format!(r#"/C ping -n 3 127.0.0.1 >nul & start "" "{}" --after-update"#, exe.display());
    std::process::Command::new("cmd")
        .raw_arg(line)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| format!("Updated, but couldn't restart: {e}"))?;
    Ok(())
}

/// Remove the previous exe left behind by an update (it can't be deleted while running).
pub fn cleanup_old() {
    if let Ok(exe) = std::env::current_exe() {
        let old = old_path(&exe);
        if old.exists() {
            for _ in 0..10 {
                if std::fs::remove_file(&old).is_ok() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(500));
            }
        }
    }
}
