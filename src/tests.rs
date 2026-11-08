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
