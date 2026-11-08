//! Discord: webhook alerts (opt-in, your own webhook) and Rich Presence over the local Discord IPC pipe.
use std::fs::File;
use std::io::{Read, Write};
use std::time::{SystemTime, UNIX_EPOCH};

/// Built-in Discord application used for Rich Presence when the user hasn't set their own.
/// Application IDs are public identifiers (not secrets), so shipping one is fine.
pub const DEFAULT_APP_ID: &str = "1568963569779875942";

pub fn valid_webhook(url: &str) -> bool {
    let u = url.trim();
    ["https://discord.com/api/webhooks/", "https://discordapp.com/api/webhooks/", "https://canary.discord.com/api/webhooks/", "https://ptb.discord.com/api/webhooks/"]
        .iter()
        .any(|p| u.starts_with(p))
}

/// Post a message to a Discord webhook.
pub fn send_webhook(url: &str, text: &str) -> Result<(), String> {
    if !valid_webhook(url) {
        return Err("That doesn't look like a Discord webhook URL.".into());
    }
    let body = serde_json::json!({ "username": "RBX Account Manager", "content": text }).to_string();
    let agent = crate::roblox::agent()?;
    match agent.post(url.trim()).set("Content-Type", "application/json").send_string(&body) {
        Ok(_) => Ok(()),
        Err(ureq::Error::Status(c, _)) => Err(format!("Discord returned HTTP {c}.")),
        Err(e) => Err(format!("Couldn't reach Discord: {e}")),
    }
}

/// Fire-and-forget webhook alert on a background thread.
pub fn alert(url: &str, text: String) {
    if url.trim().is_empty() {
        return;
    }
    let url = url.to_string();
    std::thread::spawn(move || {
        let _ = send_webhook(&url, &text);
    });
}

// ------------------------------------------------------------------ Rich Presence (IPC)
pub struct Rpc {
    pipe: Option<File>,
    app_id: String,
    started: u64,
}

impl Rpc {
    pub fn new() -> Self {
        let started = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        Rpc { pipe: None, app_id: String::new(), started }
    }

    fn frame(pipe: &mut File, op: u32, json: &str) -> std::io::Result<()> {
        let mut buf = Vec::with_capacity(8 + json.len());
        buf.extend_from_slice(&op.to_le_bytes());
        buf.extend_from_slice(&(json.len() as u32).to_le_bytes());
        buf.extend_from_slice(json.as_bytes());
        pipe.write_all(&buf)
    }

    fn read_frame(pipe: &mut File) -> std::io::Result<String> {
        let mut head = [0u8; 8];
        pipe.read_exact(&mut head)?;
        let len = u32::from_le_bytes([head[4], head[5], head[6], head[7]]) as usize;
        let mut body = vec![0u8; len.min(64 * 1024)];
        pipe.read_exact(&mut body)?;
        Ok(String::from_utf8_lossy(&body).to_string())
    }

    fn connect(&mut self, app_id: &str) -> Result<(), String> {
        for i in 0..10 {
            let path = format!(r"\\.\pipe\discord-ipc-{i}");
            if let Ok(mut f) = std::fs::OpenOptions::new().read(true).write(true).open(&path) {
                let hs = serde_json::json!({ "v": 1, "client_id": app_id }).to_string();
                Self::frame(&mut f, 0, &hs).map_err(|e| e.to_string())?;
                let reply = Self::read_frame(&mut f).map_err(|e| e.to_string())?;
                if !reply.contains("READY") {
                    return Err("Discord rejected the Application ID.".into());
                }
                self.pipe = Some(f);
                self.app_id = app_id.to_string();
                return Ok(());
            }
        }
        Err("Discord isn't running.".into())
    }

    /// Show / update the activity. Empty `app_id` clears it.
    pub fn set(&mut self, app_id: &str, details: &str, state: &str) -> Result<(), String> {
        let app_id = app_id.trim();
        if app_id.is_empty() {
            self.clear();
            return Ok(());
        }
        if self.pipe.is_none() || self.app_id != app_id {
            self.pipe = None;
            self.connect(app_id)?;
        }
        let msg = serde_json::json!({
            "cmd": "SET_ACTIVITY",
            "args": {
                "pid": std::process::id(),
                "activity": {
                    "details": details,
                    "state": state,
                    "timestamps": { "start": self.started },
                    // shown when an Art Asset named "logo" exists in your Discord application
                    "assets": { "large_image": "logo", "large_text": "RBX Account Manager" }
                }
            },
            "nonce": format!("{}", SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0))
        })
        .to_string();
        let pipe = self.pipe.as_mut().unwrap();
        let r = Self::frame(pipe, 1, &msg).and_then(|_| Self::read_frame(pipe).map(|_| ()));
        if let Err(e) = r {
            self.pipe = None;
            return Err(e.to_string());
        }
        Ok(())
    }

    pub fn clear(&mut self) {
        if let Some(mut p) = self.pipe.take() {
            let msg = serde_json::json!({ "cmd": "SET_ACTIVITY", "args": { "pid": std::process::id() }, "nonce": "clear" }).to_string();
            let _ = Self::frame(&mut p, 1, &msg);
        }
    }
}
