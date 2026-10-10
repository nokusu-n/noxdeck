use noxdeck_lib::settings::Settings;
use serde_json::{json, Value};

#[test]
fn defaults_include_the_stage_three_settings() {
    let value = serde_json::to_value(Settings::default()).unwrap();
    assert_eq!(value["after_start"], "tray");
    assert!(value.get("show_game_start").is_none());
    assert!(value.get("language").is_some());
    assert_eq!(value["language"], Value::Null);
}

#[test]
fn a_stage_two_file_keeps_old_preferences_and_adds_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    std::fs::write(&path, r#"{"schema":1,"sort":"battletag","dismissed":{"remember_login":true,"logout_warning":true}}"#).unwrap();
    let (settings, reset) = Settings::load(&path);
    assert!(!reset);
    let value = serde_json::to_value(settings).unwrap();
    assert_eq!(value["sort"], "battletag");
    assert_eq!(value["dismissed"]["logout_warning"], true);
    assert_eq!(value["after_start"], "tray");
    assert!(value.get("show_game_start").is_none());
    assert!(value.get("language").is_some());
}

fn round_trip(field: &str, values: &[Value]) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    for expected in values {
        let mut input = json!({"schema":1,"sort":"last_login","dismissed":{"remember_login":false,"logout_warning":false}});
        input[field] = expected.clone();
        let settings: Settings = serde_json::from_value(input).unwrap();
        settings.save(&path).unwrap();
        let (loaded, reset) = Settings::load(&path);
        assert!(!reset);
        let actual = serde_json::to_value(loaded).unwrap();
        assert!(actual.get(field).is_some(), "{field} was lost");
        assert_eq!(&actual[field], expected);
    }
}

#[test]
fn after_start_round_trips_each_choice() {
    round_trip("after_start", &[json!("stay"), json!("tray"), json!("close")]);
}

#[test]
fn an_old_game_start_preference_is_retained_only_for_account_migration() {
    let settings: Settings = serde_json::from_value(json!({"schema":1,"sort":"last_login","dismissed":{"remember_login":false,"logout_warning":false},"show_game_start":false})).unwrap();
    assert_eq!(settings.legacy_show_game_start, Some(false));
}

#[test]
fn language_round_trips_follow_windows_and_all_languages() {
    round_trip("language", &[Value::Null, json!("ru"), json!("en"), json!("it")]);
}
