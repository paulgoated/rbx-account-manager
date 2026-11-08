//! Account storage (DPAPI-encrypted) and settings. Same format as earlier versions.
use serde::{Deserialize, Deserializer, Serialize};
use std::path::PathBuf;

use crate::win;

const ENTROPY: &[u8] = b"LocalRbxManager-v1";

/// Accept string / number / null from older JSON and store it as a String.
fn de_str<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let v = serde_json::Value::deserialize(d)?;
    Ok(match v {
        serde_json::Value::Null => String::new(),
        serde_json::Value::String(s) => s,
        other => other.to_string(),
    })
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "PascalCase", default)]
pub struct Account {
    #[serde(deserialize_with = "de_str")]
    pub user_id: String,
    #[serde(deserialize_with = "de_str")]
    pub username: String,
    #[serde(deserialize_with = "de_str")]
    pub display_name: String,
    #[serde(deserialize_with = "de_str")]
    pub alias: String,
    #[serde(deserialize_with = "de_str")]
    pub cookie: String,
    #[serde(deserialize_with = "de_str")]
    pub added: String,
    #[serde(deserialize_with = "de_str")]
    pub status: String,
    #[serde(deserialize_with = "de_str")]
    pub last_played: String,
    #[serde(deserialize_with = "de_str")]
    pub group: String,
    #[serde(deserialize_with = "de_str")]
    pub note: String,
    pub favorite: bool,
    /// Main account: skipped by "select all".
    pub main: bool,
}

impl Account {
    pub fn is_ok(&self) -> bool {
        self.status.is_empty() || self.status == "OK"
    }
    pub fn title(&self) -> &str {
        if self.alias.is_empty() { &self.username } else { &self.alias }
    }
}

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
pub struct Settings {
    #[serde(deserialize_with = "de_str")]
    pub last_game: String,
    #[serde(deserialize_with = "de_str")]
    pub last_job: String,
    pub multi_roblox: bool,
    pub recent_games: Vec<String>,
    pub favorite_games: Vec<String>,
    /// Delay between accounts in a batch launch (seconds). 0 = default (6).
    pub stagger_secs: u32,
    /// true = launch to the Roblox home screen (don't join a game).
    pub launch_home: bool,
    /// Game name cache: Place ID -> name.
    pub game_names: std::collections::HashMap<String, String>,
    /// FPS limit written to ClientAppSettings.json (0 = Roblox default).
    pub fps_cap: u32,
    pub anti_afk: bool,
    /// Minutes between Anti-AFK nudges (0 = default 15).
    pub afk_minutes: u32,
    pub auto_rejoin: bool,
    pub close_to_tray: bool,
    pub discord_alerts: bool,
    pub discord_webhook: String,
    pub discord_rpc: bool,
    pub discord_app_id: String,
}

pub fn data_dir() -> PathBuf {
    if let Ok(d) = std::env::var("RBXMGR_DATADIR") {
        if !d.is_empty() {
            return PathBuf::from(d);
        }
    }
    let base = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".into());
    PathBuf::from(base).join("LocalRbxManager")
}

fn strip_bom(s: &str) -> &str {
    s.trim().trim_start_matches('\u{feff}')
}

pub fn load_accounts() -> Result<Vec<Account>, String> {
    let file = data_dir().join("accounts.dat");
    if !file.exists() {
        return Ok(Vec::new());
    }
    let raw = std::fs::read(&file).map_err(|e| e.to_string())?;
    let plain = win::unprotect(&raw, ENTROPY)?;
    let text = String::from_utf8_lossy(&plain).to_string();
    let json = strip_bom(&text);
    if json.is_empty() {
        return Ok(Vec::new());
    }
    let json = if json.starts_with('{') { format!("[{json}]") } else { json.to_string() };
    serde_json::from_str(&json).map_err(|e| format!("Account data is corrupted: {e}"))
}

pub fn save_accounts(list: &[Account]) -> Result<(), String> {
    let dir = data_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_vec(list).map_err(|e| e.to_string())?;
    let enc = win::protect(&json, ENTROPY)?;
    let file = dir.join("accounts.dat");
    let tmp = dir.join("accounts.dat.tmp");
    std::fs::write(&tmp, enc).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &file).map_err(|e| e.to_string())
}

/// DPAPI entropy that Roblox Account Manager (ic3w0lf) uses for AccountData.json.
const RAM_ENTROPY: &[u8] = b"ROBLOX ACCOUNT MANAGER | :) | BROUGHT TO YOU BUY ic3w0lf";

/// Read AccountData.json from Roblox Account Manager. The original file is never modified.
pub fn import_ram(path: &str) -> Result<Vec<Account>, String> {
    let path = path.trim().trim_matches('"');
    if path.is_empty() {
        return Err("Enter the AccountData.json location first.".into());
    }
    let raw = std::fs::read(path).map_err(|e| format!("Couldn't read the file: {e}"))?;
    let head = String::from_utf8_lossy(&raw[..raw.len().min(16)]).to_string();
    let text = if strip_bom(&head).starts_with('[') || strip_bom(&head).starts_with('{') {
        strip_bom(&String::from_utf8_lossy(&raw)).to_string()
    } else {
        let plain = win::unprotect(&raw, RAM_ENTROPY).or_else(|_| win::unprotect(&raw, &[])).map_err(|_| {
            "Couldn't open the file. It's probably password-protected in Roblox Account Manager \
             (turn the password off first), or it was created by a different Windows user."
                .to_string()
        })?;
        strip_bom(&String::from_utf8_lossy(&plain)).to_string()
    };
    let v: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("This isn't Roblox Account Manager data: {e}"))?;
    let items = match v {
        serde_json::Value::Array(a) => a,
        other @ serde_json::Value::Object(_) => vec![other],
        _ => vec![],
    };
    let mut out = Vec::new();
    for it in &items {
        let s = |k: &str| match &it[k] {
            serde_json::Value::String(s) => s.trim().to_string(),
            serde_json::Value::Number(n) => n.to_string(),
            _ => String::new(),
        };
        let (cookie, user_id, username, alias) = (s("SecurityToken"), s("UserID"), s("Username"), s("Alias"));
        if cookie.is_empty() || user_id.is_empty() {
            continue;
        }
        let valid = !matches!(it["Valid"], serde_json::Value::Bool(false));
        out.push(Account {
            alias: if alias.is_empty() { username.clone() } else { alias },
            user_id,
            username,
            cookie,
            status: if valid { "OK" } else { "EXPIRED" }.into(),
            ..Default::default()
        });
    }
    if out.is_empty() {
        return Err("No accounts could be imported from this file.".into());
    }
    Ok(out)
}

pub fn load_settings() -> Settings {
    let file = data_dir().join("settings.json");
    std::fs::read_to_string(file)
        .ok()
        .and_then(|t| serde_json::from_str(strip_bom(&t)).ok())
        .unwrap_or_default()
}

pub fn save_settings(s: &Settings) {
    let dir = data_dir();
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(j) = serde_json::to_string_pretty(s) {
        let _ = std::fs::write(dir.join("settings.json"), j);
    }
}
