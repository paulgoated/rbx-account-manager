//! Background monitor (runs even while the window is hidden in the tray):
//! - maps new Roblox processes to the account that launched them
//! - watches each client's Roblox log for disconnects / kicks and auto-rejoins
//! - Anti-AFK nudges
//! - handles tray commands
use crate::roblox::{self, GameTarget};
use crate::{discord, win};
use regex::Regex;
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

/// Mirrors the Anti-AFK setting for the tray menu check mark.
pub static AFK_ON: AtomicBool = AtomicBool::new(false);

const REJOIN_LIMIT: usize = 3;
const REJOIN_WINDOW: Duration = Duration::from_secs(600);

pub enum Event {
    Toast(String, bool),
    AntiAfk(bool),
}

pub enum TrayCmd {
    StopAll,
    ToggleAfk,
}

#[derive(Clone, Default)]
pub struct Config {
    pub anti_afk: bool,
    pub afk_minutes: u32,
    pub auto_rejoin: bool,
    pub alerts: bool,
    pub webhook: String,
    pub rpc: bool,
    pub rpc_app_id: String,
    pub accounts: usize,
    pub fps_cap: u32,
}

struct Pending {
    user_id: String,
    username: String,
    cookie: String,
    target: Option<GameTarget>,
    at: Instant,
}

pub struct Client {
    pub pid: u32,
    pub user_id: String,
    pub username: String,
    cookie: String,
    target: Option<GameTarget>,
    started: SystemTime,
    started_at: Instant,
    log: Option<PathBuf>,
    log_pos: u64,
    handled: bool,
}

#[derive(Default)]
pub struct Shared {
    pub cfg: Config,
    pending: VecDeque<Pending>,
    pub clients: Vec<Client>,
    rejoins: HashMap<String, Vec<Instant>>,
    pub last_nudge: Option<String>,
    /// Rich Presence status for the Settings tab ("OK: ..." when connected).
    pub rpc_status: String,
}

pub type SharedRef = Arc<Mutex<Shared>>;

impl Shared {
    /// Called right after a client was launched, so its process can be matched to the account.
    pub fn expect_launch(&mut self, user_id: &str, username: &str, cookie: &str, target: Option<GameTarget>) {
        self.pending.push_back(Pending {
            user_id: user_id.into(),
            username: username.into(),
            cookie: cookie.into(),
            target,
            at: Instant::now(),
        });
    }

    pub fn username_for_pid(&self, pid: u32) -> Option<String> {
        self.clients.iter().find(|c| c.pid == pid).map(|c| c.username.clone())
    }

    pub fn forget_all(&mut self) {
        self.clients.clear();
        self.pending.clear();
    }
}

fn logs_dir() -> Option<PathBuf> {
    std::env::var("LOCALAPPDATA").ok().map(|l| PathBuf::from(l).join("Roblox").join("logs"))
}

/// Newest player log created after `since` that no other client has claimed.
fn find_log(since: SystemTime, claimed: &HashSet<PathBuf>) -> Option<PathBuf> {
    let dir = logs_dir()?;
    let since = since.checked_sub(Duration::from_secs(10)).unwrap_or(since);
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            let name = p.file_name()?.to_string_lossy().to_lowercase();
            if !name.ends_with(".log") || !name.contains("player") || claimed.contains(&p) {
                return None;
            }
            let meta = e.metadata().ok()?;
            let created = meta.created().or_else(|_| meta.modified()).ok()?;
            (created >= since).then_some((created, p))
        })
        .min_by_key(|(t, _)| *t)
        .map(|(_, p)| p)
}

fn read_new(path: &PathBuf, pos: &mut u64) -> String {
    let Ok(mut f) = std::fs::File::open(path) else { return String::new() };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    if len < *pos {
        *pos = 0;
    }
    if f.seek(SeekFrom::Start(*pos)).is_err() {
        return String::new();
    }
    let mut buf = Vec::new();
    let _ = f.take(2 * 1024 * 1024).read_to_end(&mut buf);
    *pos += buf.len() as u64;
    String::from_utf8_lossy(&buf).to_string()
}

/// Lines in a Roblox client log that mean the client lost its game (disconnect, kick, shutdown).
pub fn disconnect_regex() -> Regex {
    Regex::new(
        r"(?i)(lost connection|disconnection notification|you were kicked|kicked by|has been kicked|server has shut ?down|server is shutting down|connection attempt failed|disconnected for being idle|error code: ?(26[0-9]|27[0-9]|28[0-9]))",
    )
    .unwrap()
}

pub fn start(shared: SharedRef, events: Sender<Event>, tray_rx: Receiver<TrayCmd>, main_hwnd: isize, repaint: Arc<dyn Fn() + Send + Sync>) {
    std::thread::spawn(move || {
        unsafe {
            // ShellExecute (launching roblox-player: links) wants COM on this thread.
            windows_sys::Win32::System::Com::CoInitializeEx(std::ptr::null(), 2);
        }
        let disconnect = disconnect_regex();
        let send = |e: Event| {
            let _ = events.send(e);
            repaint();
        };
        let mut known: HashSet<u32> = win::roblox_pids().into_iter().collect();
        let mut next_scan = Instant::now();
        let mut next_logs = Instant::now();
        let mut next_trim = Instant::now();
        let mut next_afk: Option<Instant> = None;
        let mut afk_due_since: Option<Instant> = None;

        loop {
            std::thread::sleep(Duration::from_millis(1000));
            let now = Instant::now();
            let cfg = shared.lock().unwrap().cfg.clone();
            AFK_ON.store(cfg.anti_afk, Ordering::Relaxed);

            // ---- tray commands
            while let Ok(cmd) = tray_rx.try_recv() {
                match cmd {
                    TrayCmd::StopAll => {
                        shared.lock().unwrap().forget_all();
                        let n = win::kill_pids(&win::roblox_pids());
                        send(Event::Toast(format!("Closed {n} Roblox window(s)."), false));
                    }
                    TrayCmd::ToggleAfk => {
                        let v = {
                            let mut sh = shared.lock().unwrap();
                            sh.cfg.anti_afk = !sh.cfg.anti_afk;
                            sh.cfg.anti_afk
                        };
                        AFK_ON.store(v, Ordering::Relaxed);
                        send(Event::AntiAfk(v));
                    }
                }
            }

            // ---- match new processes to pending launches
            if now >= next_scan {
                next_scan = now + Duration::from_secs(2);
                let pids = win::roblox_pids();
                let mut sh = shared.lock().unwrap();
                sh.clients.retain(|c| pids.contains(&c.pid));
                sh.pending.retain(|p| p.at.elapsed() < Duration::from_secs(90));
                for pid in &pids {
                    if known.insert(*pid) {
                        if let Some(p) = sh.pending.pop_front() {
                            sh.clients.push(Client {
                                pid: *pid,
                                user_id: p.user_id,
                                username: p.username,
                                cookie: p.cookie,
                                target: p.target,
                                started: SystemTime::now(),
                                started_at: Instant::now(),
                                log: None,
                                log_pos: 0,
                                handled: false,
                            });
                        }
                    }
                }
                known.retain(|p| pids.contains(p));
            }

            // ---- watch logs for disconnects
            let mut dropped: Vec<(u32, String, String, String, Option<GameTarget>)> = Vec::new();
            if now >= next_logs {
                next_logs = now + Duration::from_secs(3);
                let mut sh = shared.lock().unwrap();
                let mut claimed: HashSet<PathBuf> = sh.clients.iter().filter_map(|c| c.log.clone()).collect();
                for c in sh.clients.iter_mut() {
                    if c.log.is_none() && c.started_at.elapsed() < Duration::from_secs(120) {
                        if let Some(p) = find_log(c.started, &claimed) {
                            claimed.insert(p.clone());
                            c.log = Some(p);
                        }
                    }
                    if c.handled {
                        continue;
                    }
                    if let Some(path) = c.log.clone() {
                        let text = read_new(&path, &mut c.log_pos);
                        if !text.is_empty() && disconnect.is_match(&text) {
                            c.handled = true;
                            dropped.push((c.pid, c.user_id.clone(), c.username.clone(), c.cookie.clone(), c.target.clone()));
                        }
                    }
                }
            }
            for (pid, user_id, username, cookie, target) in dropped {
                let alert = |t: String| {
                    if cfg.alerts {
                        discord::alert(&cfg.webhook, t);
                    }
                };
                if !cfg.auto_rejoin {
                    send(Event::Toast(format!("{username} was disconnected."), true));
                    alert(format!(":warning: **{username}** was disconnected."));
                    continue;
                }
                let allowed = {
                    let mut sh = shared.lock().unwrap();
                    let hist = sh.rejoins.entry(user_id.clone()).or_default();
                    hist.retain(|t| t.elapsed() < REJOIN_WINDOW);
                    if hist.len() >= REJOIN_LIMIT {
                        false
                    } else {
                        hist.push(Instant::now());
                        true
                    }
                };
                if !allowed {
                    send(Event::Toast(format!("Stopped auto-rejoin for {username}: {REJOIN_LIMIT} rejoins in 10 minutes."), true));
                    alert(format!(":octagonal_sign: Stopped auto-rejoin for **{username}** ({REJOIN_LIMIT} rejoins in 10 minutes)."));
                    continue;
                }
                send(Event::Toast(format!("{username} disconnected. Rejoining..."), false));
                win::kill_pids(&[pid]);
                std::thread::sleep(Duration::from_secs(3));
                let mut target = target;
                if let Some(g) = target.as_mut() {
                    g.job_id = None; // the old server may be gone
                }
                if cfg.fps_cap > 0 {
                    let _ = crate::fps::apply(cfg.fps_cap);
                }
                match roblox::get_ticket(&cookie) {
                    Ok(ticket) => {
                        let uri = match &target {
                            Some(g) => roblox::launch_uri(&ticket, g),
                            None => roblox::home_uri(&ticket),
                        };
                        if win::open_url(&uri) {
                            shared.lock().unwrap().expect_launch(&user_id, &username, &cookie, target);
                            alert(format!(":arrows_counterclockwise: **{username}** was disconnected and is rejoining."));
                        }
                    }
                    Err(e) => {
                        send(Event::Toast(format!("Rejoin failed for {username}: {e}"), true));
                        alert(format!(":x: Rejoin failed for **{username}**: {e}"));
                    }
                }
            }

            // ---- Anti-AFK: due every N minutes; waits for a short pause in your input (max 4 min)
            if cfg.anti_afk {
                let every = Duration::from_secs(cfg.afk_minutes.clamp(1, 19) as u64 * 60);
                let due = *next_afk.get_or_insert(now + every);
                if now >= due {
                    let since = *afk_due_since.get_or_insert(now);
                    if win::user_idle_secs() >= 5 || since.elapsed() > Duration::from_secs(240) {
                        let wins = win::roblox_windows();
                        for w in &wins {
                            win::nudge_window(w.hwnd);
                            std::thread::sleep(Duration::from_millis(250));
                        }
                        if !wins.is_empty() {
                            let mut st: windows_sys::Win32::Foundation::SYSTEMTIME = unsafe { std::mem::zeroed() };
                            unsafe { windows_sys::Win32::System::SystemInformation::GetLocalTime(&mut st) };
                            shared.lock().unwrap().last_nudge = Some(format!("{:02}:{:02} ({} client(s))", st.wHour, st.wMinute, wins.len()));
                            repaint();
                        }
                        next_afk = Some(Instant::now() + every);
                        afk_due_since = None;
                    }
                }
            } else {
                next_afk = None;
                afk_due_since = None;
            }

            // ---- keep RAM low while the window is hidden in the tray
            if now >= next_trim {
                next_trim = now + Duration::from_secs(10);
                if main_hwnd != 0 && !win::is_window_visible(main_hwnd) {
                    win::trim_memory();
                }
            }
        }
    });
}

/// Discord Rich Presence updater (own thread: the IPC pipe can block).
pub fn start_rpc(shared: SharedRef) {
    std::thread::spawn(move || {
        let mut rpc = discord::Rpc::new();
        std::thread::sleep(Duration::from_secs(2));
        // update right away when the settings change, otherwise every 15 s
        let mut last_key = String::new();
        let mut next = Instant::now();
        loop {
            let (cfg, tracked) = {
                let sh = shared.lock().unwrap();
                (sh.cfg.clone(), sh.clients.len())
            };
            let id = cfg.rpc_app_id.trim().to_string();
            let key = format!("{}|{}|{}", cfg.rpc, id, cfg.accounts);
            if key != last_key || Instant::now() >= next {
                last_key = key;
                next = Instant::now() + Duration::from_secs(15);
                let status = if !cfg.rpc {
                    rpc.clear();
                    String::new()
                } else if id.is_empty() {
                    rpc.clear();
                    "Paste your Discord Application ID above.".to_string()
                } else if !id.bytes().all(|b| b.is_ascii_digit()) || !(17..=20).contains(&id.len()) {
                    rpc.clear();
                    "That isn't an Application ID (it's a 17–20 digit number).".to_string()
                } else {
                    let running = win::roblox_pids().len().max(tracked);
                    let details = format!("Managing {} account{}", cfg.accounts, if cfg.accounts == 1 { "" } else { "s" });
                    let state = if running == 0 { "No clients running".to_string() } else { format!("{running} Roblox client{} running", if running == 1 { "" } else { "s" }) };
                    match rpc.set(&id, &details, &state) {
                        Ok(()) => "OK: Connected. Showing on your Discord profile.".to_string(),
                        Err(e) => e,
                    }
                };
                shared.lock().unwrap().rpc_status = status;
            }
            std::thread::sleep(Duration::from_secs(2));
        }
    });
}
