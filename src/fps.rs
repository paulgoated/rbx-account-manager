//! FPS limit. Roblox reads its frame rate cap from two places:
//! 1. `FramerateCap` in `%LOCALAPPDATA%\Roblox\GlobalBasicSettings_*.xml` (the in-game
//!    "Maximum Frame Rate" setting). This is the one current clients actually use.
//! 2. `DFIntTaskSchedulerTargetFps` in each install's `ClientSettings/ClientAppSettings.json`
//!    (the older fast flag; kept as a fallback). Other flags in that file are preserved.
//!
//! Roblox rewrites GlobalBasicSettings when a client closes, so this is applied before every launch.
use regex::Regex;
use serde_json::{Map, Value};
use std::path::PathBuf;

const FLAG: &str = "DFIntTaskSchedulerTargetFps";

fn local_appdata() -> Option<PathBuf> {
    std::env::var("LOCALAPPDATA").ok().map(PathBuf::from)
}

/// Version folders that contain RobloxPlayerBeta.exe.
pub fn version_dirs() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(local) = local_appdata() {
        roots.push(local.join("Roblox").join("Versions"));
    }
    roots.push(PathBuf::from(r"C:\Program Files (x86)\Roblox\Versions"));
    roots.push(PathBuf::from(r"C:\Program Files\Roblox\Versions"));
    let mut out = Vec::new();
    for root in roots {
        let Ok(rd) = std::fs::read_dir(&root) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.join("RobloxPlayerBeta.exe").is_file() {
                out.push(p);
            }
        }
    }
    out
}

/// Roblox's per-user settings files (GlobalBasicSettings_13.xml etc.).
pub fn basic_settings_files() -> Vec<PathBuf> {
    let Some(dir) = local_appdata().map(|l| l.join("Roblox")) else { return Vec::new() };
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    rd.flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .map(|n| {
                    let n = n.to_string_lossy();
                    n.starts_with("GlobalBasicSettings") && n.ends_with(".xml")
                })
                .unwrap_or(false)
        })
        .collect()
}

/// Set `FramerateCap` in one GlobalBasicSettings file. Returns true if the file was changed.
fn set_framerate_cap(file: &PathBuf, cap: u32) -> Result<bool, String> {
    let text = std::fs::read_to_string(file).map_err(|e| e.to_string())?;
    let re = Regex::new(r#"<int name="FramerateCap">-?\d+</int>"#).unwrap();
    // 0 = Roblox default (-1). Roblox's own options top out at 240.
    let value: i64 = if cap == 0 { -1 } else { cap.min(240) as i64 };
    let new = format!(r#"<int name="FramerateCap">{value}</int>"#);
    let updated = if re.is_match(&text) {
        re.replace(&text, new.as_str()).to_string()
    } else if let Some(i) = text.find("<Properties>") {
        // older files without the entry: add it
        let at = i + "<Properties>".len();
        format!("{}\n\t\t\t{}{}", &text[..at], new, &text[at..])
    } else {
        return Ok(false);
    };
    if updated == text {
        return Ok(false);
    }
    std::fs::write(file, updated).map_err(|e| e.to_string())?;
    Ok(true)
}

fn set_fast_flag(dir: &PathBuf, cap: u32) -> Result<(), String> {
    let folder = dir.join("ClientSettings");
    let file = folder.join("ClientAppSettings.json");
    let mut map: Map<String, Value> = std::fs::read_to_string(&file)
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(t.trim_start_matches('\u{feff}')).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();
    if cap == 0 {
        if map.remove(FLAG).is_none() {
            return Ok(());
        }
    } else {
        map.insert(FLAG.into(), Value::from(cap));
    }
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    std::fs::write(&file, serde_json::to_string_pretty(&Value::Object(map)).unwrap_or_default()).map_err(|e| e.to_string())
}

/// Apply `cap` (0 = Roblox default). Returns how many Roblox installs / settings files were found.
pub fn apply(cap: u32) -> Result<usize, String> {
    let dirs = version_dirs();
    let files = basic_settings_files();
    if dirs.is_empty() && files.is_empty() {
        return Err("Roblox Player installation not found.".into());
    }
    let mut last_err = None;
    let mut ok = 0;
    for f in &files {
        match set_framerate_cap(f, cap) {
            Ok(_) => ok += 1,
            Err(e) => last_err = Some(e),
        }
    }
    for d in &dirs {
        match set_fast_flag(d, cap) {
            Ok(()) => ok += 1,
            Err(e) => last_err = Some(e),
        }
    }
    match (ok, last_err) {
        (0, Some(e)) => Err(format!("Couldn't write the FPS setting: {e}")),
        (_, _) => Ok(dirs.len().max(1)),
    }
}
