//! Unit tests: `cargo test` (none of these touch your real Roblox install or accounts).
use crate::{discord, fps, monitor, roblox};

#[test]
fn fps_apply_writes_and_removes_flag_in_a_fake_install() {
    // Only run when there is no system-wide Roblox install that apply() would also touch.
    if std::path::Path::new(r"C:\Program Files (x86)\Roblox\Versions").exists() || std::path::Path::new(r"C:\Program Files\Roblox\Versions").exists() {
        return;
    }
    let tmp = std::env::temp_dir().join(format!("rbxmgr_fps_test_{}", std::process::id()));
    let ver = tmp.join("Roblox").join("Versions").join("version-test");
    std::fs::create_dir_all(ver.join("ClientSettings")).unwrap();
    std::fs::write(ver.join("RobloxPlayerBeta.exe"), b"").unwrap();
    std::fs::write(ver.join("ClientSettings").join("ClientAppSettings.json"), r#"{"FFlagKeepMe": true}"#).unwrap();
    let basic = tmp.join("Roblox").join("GlobalBasicSettings_13.xml");
    std::fs::write(&basic, "<roblox><Item class=\"UserGameSettings\"><Properties>\n<int name=\"FramerateCap\">-1</int>\n<int name=\"GraphicsQualityLevel\">21</int>\n</Properties></Item></roblox>").unwrap();
    let old = std::env::var("LOCALAPPDATA").ok();
    std::env::set_var("LOCALAPPDATA", &tmp);

    assert_eq!(fps::apply(30).unwrap(), 1);
    let xml = std::fs::read_to_string(&basic).unwrap();
    assert!(xml.contains(r#"<int name="FramerateCap">30</int>"#), "in-game FPS cap must be set: {xml}");
    assert!(xml.contains(r#"<int name="GraphicsQualityLevel">21</int>"#), "other settings must be kept");

    assert_eq!(fps::apply(144).unwrap(), 1);
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(ver.join("ClientSettings").join("ClientAppSettings.json")).unwrap()).unwrap();
    assert_eq!(v["DFIntTaskSchedulerTargetFps"], 144);
    assert_eq!(v["FFlagKeepMe"], true, "other flags must be kept");

    assert_eq!(fps::apply(0).unwrap(), 1);
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(ver.join("ClientSettings").join("ClientAppSettings.json")).unwrap()).unwrap();
    assert!(v.get("DFIntTaskSchedulerTargetFps").is_none());
    assert_eq!(v["FFlagKeepMe"], true);
    assert!(std::fs::read_to_string(&basic).unwrap().contains(r#"<int name="FramerateCap">-1</int>"#), "0 resets to Roblox default");

    if let Some(o) = old {
        std::env::set_var("LOCALAPPDATA", o);
    }
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn disconnect_lines_are_detected() {
    let re = monitor::disconnect_regex();
    for line in [
        "2025-11-08T10:00:00.000Z,1.0,abc,6 [FLog::Network] Lost connection with reason : Lost connection to the game server",
        "[FLog::Output] Disconnection Notification. Reason: 277",
        "You were kicked from this experience: Server is shutting down",
        "Disconnected (Error Code: 279)",
    ] {
        assert!(re.is_match(line), "should match: {line}");
    }
    for line in ["[FLog::Network] Connected to server", "Players: 12", "game joined"] {
        assert!(!re.is_match(line), "should not match: {line}");
    }
}

#[test]
fn rich_presence_defaults() {
    // new users (no settings file) and old settings files without the field get Rich Presence on
    let s: crate::store::Settings = serde_json::from_str("{}").unwrap();
    assert!(s.discord_rpc);
    // users who turned it off keep it off
    let s: crate::store::Settings = serde_json::from_str(r#"{"DiscordRpc": false}"#).unwrap();
    assert!(!s.discord_rpc);
    assert!(crate::discord::DEFAULT_APP_ID.bytes().all(|b| b.is_ascii_digit()));
}

#[test]
fn ram_import_reads_saved_passwords() {
    let file = std::env::temp_dir().join(format!("rbxmgr_ram_test_{}.json", std::process::id()));
    std::fs::write(
        &file,
        r#"[
            {"Valid": true, "SecurityToken": "_|WARNING:-fake-A", "Username": "alpha", "UserID": 1, "Alias": "", "Password": " p@ss:word "},
            {"Valid": false, "SecurityToken": "_|WARNING:-fake-B", "Username": "beta", "UserID": 2, "Password": "   "},
            {"Valid": true, "SecurityToken": "_|WARNING:-fake-C", "Username": "gamma", "UserID": 3, "Password": null},
            {"Valid": true, "SecurityToken": "_|WARNING:-fake-D", "Username": "delta", "UserID": 4}
        ]"#,
    )
    .unwrap();
    let list = crate::store::import_ram(file.to_str().unwrap());
    let _ = std::fs::remove_file(&file);
    let list = list.unwrap();
    assert_eq!(list.len(), 4);
    assert_eq!(list[0].password, "p@ss:word", "spaces around the password are trimmed");
    assert_eq!(list[0].status, "OK");
    assert_eq!(list[1].status, "EXPIRED");
    assert!(list[1].password.is_empty(), "a password of only spaces counts as not saved");
    // RAM leaves the password empty or out for accounts it got from a cookie
    assert!(list[2].password.is_empty() && list[3].password.is_empty());
}

#[test]
fn accounts_saved_before_passwords_still_load() {
    let old: Vec<crate::store::Account> = serde_json::from_str(r#"[{"UserId": "1", "Username": "alpha", "Cookie": "x"}]"#).unwrap();
    assert!(old[0].password.is_empty());
    let a = crate::store::Account { password: "secret".into(), ..Default::default() };
    let back: crate::store::Account = serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
    assert_eq!(back.password, "secret");
}

#[test]
fn webhook_urls_are_validated() {
    assert!(discord::valid_webhook("https://discord.com/api/webhooks/123/abc"));
    assert!(discord::valid_webhook("https://discordapp.com/api/webhooks/123/abc"));
    assert!(!discord::valid_webhook("https://example.com/api/webhooks/123/abc"));
    assert!(!discord::valid_webhook("http://discord.com/api/webhooks/123/abc"));
}

#[test]
fn game_inputs_parse() {
    let g = roblox::parse_game("https://www.roblox.com/games/79423426397666/Brawl-It-Out", "").unwrap();
    assert_eq!(g.place_id, "79423426397666");
    let g = roblox::parse_game("1730877806", "").unwrap();
    assert_eq!(g.place_id, "1730877806");
    let g = roblox::parse_game("@builderman", "").unwrap();
    assert_eq!(g.follow_username.as_deref(), Some("builderman"));
    assert!(roblox::home_uri("TICKET").starts_with("roblox-player:1+launchmode:app+gameinfo:TICKET"));
}

#[test]
fn listing_roblox_windows_does_not_crash() {
    let wins = crate::win::roblox_windows();
    let pids = crate::win::roblox_pids();
    assert!(wins.len() <= pids.len());
    println!("roblox processes: {}, windows: {}", pids.len(), wins.len());
}

#[test]
#[ignore] // needs the Discord desktop app running: cargo test -- --ignored rpc_
fn rpc_connects_to_local_discord() {
    let mut rpc = crate::discord::Rpc::new();
    let r = rpc.set("100000000000000001", "test", "test");
    println!("RPC result with a fake ID: {r:?}");
    // a fake ID must be rejected by Discord (proves the IPC pipe works), never "Discord isn't running"
    assert_ne!(r, Err("Discord isn't running.".to_string()));
}

#[test]
#[ignore] // RBX_RPC_TEST_ID=<your app id> cargo test -- --ignored rpc_real
fn rpc_real_application_id() {
    let Ok(id) = std::env::var("RBX_RPC_TEST_ID") else { return };
    let mut rpc = crate::discord::Rpc::new();
    let r = rpc.set(&id, "Managing accounts", "Connection test");
    println!("RPC result with the real ID: {r:?}");
    assert!(r.is_ok());
}
