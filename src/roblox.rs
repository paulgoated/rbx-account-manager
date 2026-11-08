//! All communication with Roblox. ONLY roblox.com domains (+ the rbxcdn.com image CDN).
use regex::Regex;
use serde_json::Value;
use std::collections::HashMap;
use std::io::Read;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub struct UserInfo {
    pub id: String,
    pub name: String,
    pub display_name: String,
}

#[derive(Clone, Debug, Default)]
pub struct GameTarget {
    pub place_id: String,
    pub job_id: Option<String>,
    pub link_code: Option<String>,
    pub follow_user_id: Option<String>,
    pub follow_username: Option<String>,
    pub share_code: Option<String>,
}

#[derive(Clone, Default)]
pub struct Details {
    pub robux: Option<i64>,
    pub premium: Option<bool>,
    pub friends: Option<i64>,
    pub followers: Option<i64>,
    pub following: Option<i64>,
    pub created: String,
    pub banned: bool,
}

/// kind: 0 offline, 1 online (web/app), 2 in game, 3 studio
#[derive(Clone, Default)]
pub struct Presence {
    pub kind: i64,
    pub location: String,
}

#[derive(Clone)]
pub struct ServerInfo {
    pub id: String,
    pub playing: i64,
    pub max: i64,
    pub ping: Option<i64>,
    #[allow(dead_code)]
    pub fps: Option<f64>,
}

// ------------------------------------------------------------------ http helpers
pub(crate) fn agent() -> Result<ureq::Agent, String> {
    let tls = native_tls::TlsConnector::new().map_err(|e| format!("TLS error: {e}"))?;
    Ok(ureq::AgentBuilder::new()
        .tls_connector(Arc::new(tls))
        .redirects(0)
        .timeout(Duration::from_secs(20))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) RBXAccountManager/2.1")
        .build())
}

fn with_headers(r: ureq::Request, cookie: Option<&str>) -> ureq::Request {
    let r = r
        .set("Referer", "https://www.roblox.com/")
        .set("Origin", "https://www.roblox.com")
        .set("Accept", "application/json");
    match cookie {
        Some(c) if !c.is_empty() => r.set("Cookie", &format!(".ROBLOSECURITY={c}")),
        _ => r,
    }
}

fn get_json(a: &ureq::Agent, url: &str, cookie: Option<&str>) -> Result<Value, String> {
    match with_headers(a.get(url), cookie).call() {
        Ok(r) => {
            let t = r.into_string().map_err(|e| e.to_string())?;
            serde_json::from_str(&t).map_err(|e| e.to_string())
        }
        Err(ureq::Error::Status(c, _)) => Err(format!("HTTP {c}")),
        Err(e) => Err(format!("Couldn't reach Roblox: {e}")),
    }
}

/// POST JSON with an X-CSRF-TOKEN retry (standard Roblox API behaviour).
fn post_json(a: &ureq::Agent, url: &str, cookie: Option<&str>, body: &str) -> Result<Value, String> {
    let mut csrf: Option<String> = None;
    for _ in 0..2 {
        let mut req = with_headers(a.post(url), cookie).set("Content-Type", "application/json");
        if let Some(t) = &csrf {
            req = req.set("X-CSRF-TOKEN", t);
        }
        match req.send_string(body) {
            Ok(r) => {
                let t = r.into_string().map_err(|e| e.to_string())?;
                return serde_json::from_str(&t).map_err(|e| e.to_string());
            }
            Err(ureq::Error::Status(403, r)) if csrf.is_none() && r.header("x-csrf-token").is_some() => {
                csrf = r.header("x-csrf-token").map(|s| s.to_string());
            }
            Err(ureq::Error::Status(c, _)) => return Err(format!("HTTP {c}")),
            Err(e) => return Err(format!("Couldn't reach Roblox: {e}")),
        }
    }
    Err("HTTP 403".into())
}

fn id_str(v: &Value) -> Option<String> {
    match v {
        Value::Number(n) => Some(n.to_string()),
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        _ => None,
    }
}

fn find_key(v: &Value, key: &str) -> Option<Value> {
    match v {
        Value::Object(m) => {
            if let Some(x) = m.get(key) {
                if !x.is_null() {
                    return Some(x.clone());
                }
            }
            m.values().find_map(|x| find_key(x, key))
        }
        Value::Array(a) => a.iter().find_map(|x| find_key(x, key)),
        _ => None,
    }
}

fn fmt_date(iso: &str) -> String {
    const M: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let p: Vec<&str> = iso.get(..10).unwrap_or("").split('-').collect();
    if p.len() == 3 {
        if let (Ok(y), Ok(m), Ok(d)) = (p[0].parse::<u32>(), p[1].parse::<usize>(), p[2].parse::<u32>()) {
            if (1..=12).contains(&m) {
                return format!("{d:02} {} {y}", M[m - 1]);
            }
        }
    }
    String::new()
}

// ------------------------------------------------------------------ accounts
/// `Ok(None)` = the cookie is invalid / expired.
pub fn get_user(cookie: &str) -> Result<Option<UserInfo>, String> {
    let a = agent()?;
    match with_headers(a.get("https://users.roblox.com/v1/users/authenticated"), Some(cookie)).call() {
        Ok(r) => {
            if r.status() != 200 {
                return Ok(None);
            }
            let txt = r.into_string().map_err(|e| e.to_string())?;
            let v: Value = serde_json::from_str(&txt).map_err(|e| format!("Unexpected Roblox response: {e}"))?;
            let Some(id) = id_str(&v["id"]) else { return Ok(None) };
            Ok(Some(UserInfo {
                id,
                name: v["name"].as_str().unwrap_or_default().to_string(),
                display_name: v["displayName"].as_str().unwrap_or_default().to_string(),
            }))
        }
        Err(ureq::Error::Status(401 | 403, _)) => Ok(None),
        Err(ureq::Error::Status(c, _)) => Err(format!("Roblox returned HTTP {c} (try again later).")),
        Err(e) => Err(format!("Couldn't reach Roblox: {e}")),
    }
}

pub fn get_details(id: &str, cookie: &str) -> Details {
    let mut d = Details::default();
    let Ok(a) = agent() else { return d };
    if let Ok(v) = get_json(&a, &format!("https://economy.roblox.com/v1/users/{id}/currency"), Some(cookie)) {
        d.robux = v["robux"].as_i64();
    }
    if let Ok(v) = get_json(&a, &format!("https://premiumfeatures.roblox.com/v1/users/{id}/validate-membership"), Some(cookie)) {
        d.premium = v.as_bool();
    }
    for (kind, slot) in [("friends", &mut d.friends), ("followers", &mut d.followers), ("followings", &mut d.following)] {
        if let Ok(v) = get_json(&a, &format!("https://friends.roblox.com/v1/users/{id}/{kind}/count"), None) {
            *slot = v["count"].as_i64();
        }
    }
    if let Ok(v) = get_json(&a, &format!("https://users.roblox.com/v1/users/{id}"), None) {
        d.created = fmt_date(v["created"].as_str().unwrap_or(""));
        d.banned = v["isBanned"].as_bool().unwrap_or(false);
    }
    d
}

pub fn get_presence(ids: &[String], cookie: &str) -> Result<HashMap<String, Presence>, String> {
    let nums: Vec<&str> = ids.iter().map(|s| s.as_str()).filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())).collect();
    if nums.is_empty() {
        return Ok(HashMap::new());
    }
    let a = agent()?;
    let body = format!("{{\"userIds\":[{}]}}", nums.join(","));
    let v = post_json(&a, "https://presence.roblox.com/v1/presence/users", Some(cookie), &body)?;
    let mut out = HashMap::new();
    for p in v["userPresences"].as_array().cloned().unwrap_or_default() {
        if let Some(id) = id_str(&p["userId"]) {
            out.insert(
                id,
                Presence {
                    kind: p["userPresenceType"].as_i64().unwrap_or(0),
                    location: p["lastLocation"].as_str().unwrap_or("").to_string(),
                },
            );
        }
    }
    Ok(out)
}

pub fn username_to_id(name: &str) -> Result<String, String> {
    let a = agent()?;
    let body = serde_json::json!({ "usernames": [name], "excludeBannedUsers": true }).to_string();
    let v = post_json(&a, "https://users.roblox.com/v1/usernames/users", None, &body)?;
    v["data"].get(0).and_then(|d| id_str(&d["id"])).ok_or_else(|| format!("User @{name} not found."))
}

fn resolve_share(code: &str, cookie: &str) -> Result<(String, Option<String>), String> {
    let a = agent()?;
    for kind in ["Server", "ExperienceInvite", "ExperienceDetails"] {
        let body = serde_json::json!({ "linkId": code, "linkType": kind }).to_string();
        if let Ok(v) = post_json(&a, "https://apis.roblox.com/sharelinks/v1/resolve-link", Some(cookie), &body) {
            if let Some(place) = find_key(&v, "placeId").and_then(|x| id_str(&x)) {
                let link = find_key(&v, "linkCode").and_then(|x| id_str(&x));
                return Ok((place, link));
            }
        }
    }
    Err("Couldn't open the share link (it may have expired).".into())
}

pub fn list_servers(place: &str, cookie: Option<&str>) -> Result<Vec<ServerInfo>, String> {
    let a = agent()?;
    let mut out = Vec::new();
    let mut cursor = String::new();
    for _ in 0..3 {
        let mut url = format!("https://games.roblox.com/v1/games/{place}/servers/Public?sortOrder=Desc&excludeFullGames=false&limit=100");
        if !cursor.is_empty() {
            url.push_str("&cursor=");
            url.push_str(&cursor);
        }
        let v = get_json(&a, &url, cookie).map_err(|e| format!("Couldn't load servers: {e}"))?;
        for s in v["data"].as_array().cloned().unwrap_or_default() {
            let Some(id) = s["id"].as_str() else { continue };
            out.push(ServerInfo {
                id: id.to_string(),
                playing: s["playing"].as_i64().unwrap_or(0),
                max: s["maxPlayers"].as_i64().unwrap_or(0),
                ping: s["ping"].as_i64(),
                fps: s["fps"].as_f64(),
            });
        }
        match v["nextPageCursor"].as_str() {
            Some(c) if !c.is_empty() => cursor = c.to_string(),
            _ => break,
        }
    }
    Ok(out)
}

fn expired(code: u16) -> String {
    format!("Couldn't get an auth ticket (HTTP {code}). This account's cookie has probably expired.")
}

pub fn get_ticket(cookie: &str) -> Result<String, String> {
    let a = agent()?;
    let url = "https://auth.roblox.com/v1/authentication-ticket";
    let mut csrf: Option<String> = None;
    for _ in 0..2 {
        let mut req = with_headers(a.post(url), Some(cookie)).set("Content-Type", "application/json");
        if let Some(t) = &csrf {
            req = req.set("X-CSRF-TOKEN", t);
        }
        match req.send_string("{}") {
            Ok(r) => {
                return r.header("rbx-authentication-ticket").map(|s| s.to_string()).ok_or_else(|| expired(r.status()));
            }
            Err(ureq::Error::Status(403, r)) if csrf.is_none() => match r.header("x-csrf-token") {
                Some(t) => csrf = Some(t.to_string()),
                None => return Err(expired(403)),
            },
            Err(ureq::Error::Status(c, _)) => return Err(expired(c)),
            Err(e) => return Err(format!("Couldn't reach Roblox: {e}")),
        }
    }
    Err(expired(403))
}

// ------------------------------------------------------------------ launch
fn pct(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

impl GameTarget {
    /// Turn @username / share links into a directly launchable target (needs network).
    pub fn resolve(&mut self, cookie: &str) -> Result<(), String> {
        if let Some(name) = self.follow_username.take() {
            self.follow_user_id = Some(username_to_id(&name)?);
        }
        if let Some(code) = self.share_code.take() {
            let (place, link) = resolve_share(&code, cookie)?;
            self.place_id = place;
            self.link_code = link;
        }
        Ok(())
    }
}

pub fn launch_uri(ticket: &str, g: &GameTarget) -> String {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    let tracker = 100_000_000_000u64 + (now.as_nanos() % 899_999_999_999) as u64;
    let launch_time = now.as_millis();
    let base = "https://www.roblox.com/Game/PlaceLauncher.ashx?";
    let pl = if let Some(uid) = &g.follow_user_id {
        format!("{base}request=RequestFollowUser&browserTrackerId={tracker}&userId={uid}")
    } else if let Some(code) = &g.link_code {
        format!("{base}request=RequestPrivateGame&browserTrackerId={tracker}&placeId={}&accessCode=&linkCode={code}", g.place_id)
    } else if let Some(job) = &g.job_id {
        format!("{base}request=RequestGameJob&browserTrackerId={tracker}&placeId={}&gameId={job}&isPlayTogetherGame=false", g.place_id)
    } else {
        format!("{base}request=RequestGame&browserTrackerId={tracker}&placeId={}&isPlayTogetherGame=false", g.place_id)
    };
    format!(
        "roblox-player:1+launchmode:play+gameinfo:{ticket}+launchtime:{launch_time}+placelauncherurl:{}+browsertrackerid:{tracker}+robloxLocale:en_us+gameLocale:en_us",
        pct(&pl)
    )
}

/// Open Roblox on the home screen, signed in as the ticket's account, without joining a game.
pub fn home_uri(ticket: &str) -> String {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    let tracker = 100_000_000_000u64 + (now.as_nanos() % 899_999_999_999) as u64;
    format!(
        "roblox-player:1+launchmode:app+gameinfo:{ticket}+launchtime:{}+browsertrackerid:{tracker}+robloxLocale:en_us+gameLocale:en_us+LaunchExp:InApp",
        now.as_millis()
    )
}

/// Accepts: Place ID, game link, private server link, share link, roblox:// link,
/// profile link (users/123/profile) or @username (= follow that player into their game).
pub fn parse_game(text: &str, job: &str) -> Result<GameTarget, String> {
    let t = text.trim();
    let cap = |re: &str| Regex::new(re).unwrap().captures(t).map(|c| c[1].to_string());
    let mut g = GameTarget::default();
    if let Some(code) = cap(r"(?i)roblox\.com/share\?code=([A-Za-z0-9]+)") {
        g.share_code = Some(code);
        return Ok(g);
    }
    if let Some(u) = cap(r"^@([A-Za-z0-9_]{3,20})$") {
        g.follow_username = Some(u);
        return Ok(g);
    }
    if let Some(id) = cap(r"users/(\d+)/profile") {
        g.follow_user_id = Some(id);
        return Ok(g);
    }
    let place = if Regex::new(r"^\d+$").unwrap().is_match(t) {
        Some(t.to_string())
    } else {
        cap(r"games/(\d+)").or_else(|| cap(r"(?i)placeId=(\d+)"))
    };
    g.link_code = cap(r"privateServerLinkCode=([\w-]+)");
    g.job_id = cap(r"(?i)gameInstanceId=([0-9a-fA-F-]{36})");
    let j = job.trim();
    if !j.is_empty() {
        if !Regex::new(r"^[0-9a-fA-F-]{36}$").unwrap().is_match(j) {
            return Err("Job ID must be a GUID, e.g. 1a2b3c4d-1111-2222-3333-444455556666".into());
        }
        g.job_id = Some(j.to_string());
    }
    match place {
        Some(p) => {
            g.place_id = p;
            Ok(g)
        }
        None => Err("Enter a game link, Place ID, share link or @username first.".into()),
    }
}

// ------------------------------------------------------------------ images from the Roblox CDN
/// Download an image, only from *.rbxcdn.com hosts.
fn download_cdn(a: &ureq::Agent, url: &str) -> Option<Vec<u8>> {
    let host_ok = url
        .strip_prefix("https://")
        .and_then(|rest| rest.split('/').next())
        .map(|h| h.ends_with(".rbxcdn.com"))
        .unwrap_or(false);
    if !host_ok {
        return None;
    }
    let resp = a.get(url).call().ok()?;
    let mut buf = Vec::new();
    resp.into_reader().take(5 * 1024 * 1024).read_to_end(&mut buf).ok()?;
    Some(buf)
}

/// Game name + icon (PNG) for a Place ID. Public API, no cookie sent.
pub fn game_info(place: &str) -> (Option<String>, Option<Vec<u8>>) {
    let Ok(a) = agent() else { return (None, None) };
    let Some(uni) = get_json(&a, &format!("https://apis.roblox.com/universes/v1/places/{place}/universe"), None)
        .ok()
        .and_then(|v| id_str(&v["universeId"]))
    else {
        return (None, None);
    };
    let name = get_json(&a, &format!("https://games.roblox.com/v1/games?universeIds={uni}"), None)
        .ok()
        .and_then(|v| v["data"].get(0).and_then(|d| d["name"].as_str().map(|s| s.to_string())));
    let icon = get_json(
        &a,
        &format!("https://thumbnails.roblox.com/v1/games/icons?universeIds={uni}&returnPolicy=PlaceHolder&size=50x50&format=Png&isCircular=false"),
        None,
    )
    .ok()
    .and_then(|v| v["data"].get(0).and_then(|d| d["imageUrl"].as_str().map(|s| s.to_string())))
    .and_then(|u| download_cdn(&a, &u));
    (name, icon)
}

// ------------------------------------------------------------------ avatar
/// Fetch avatar headshots (PNG) from the public Roblox thumbnails API. No cookie sent.
pub fn fetch_headshots(ids: &[String]) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    let Ok(a) = agent() else { return out };
    for chunk in ids.chunks(50) {
        let url = format!(
            "https://thumbnails.roblox.com/v1/users/avatar-headshot?userIds={}&size=150x150&format=Png&isCircular=false",
            chunk.join(",")
        );
        let Ok(v) = get_json(&a, &url, None) else { continue };
        for it in v["data"].as_array().cloned().unwrap_or_default() {
            let Some(id) = id_str(&it["targetId"]) else { continue };
            let Some(img) = it["imageUrl"].as_str() else { continue };
            if let Some(buf) = download_cdn(&a, img) {
                out.push((id, buf));
            }
        }
    }
    out
}
