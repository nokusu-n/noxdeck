mod common;
use common::*;
use noxdeck_lib::settings::Settings;

#[test]
fn automatic_ratings_default_on_for_new_settings() {
    let f = Fake::new();
    assert!(Settings::default().auto_ratings);
    assert!(f.core().state().settings.auto_ratings);
}

#[test]
fn missing_auto_field_loads_on_but_explicit_false_stays_false() {
    let f = Fake::new();
    for value in [None, Some(false), Some(true)] {
        let mut json = serde_json::json!({"schema":1,"sort":"last_login","dismissed":{"remember_login":false,"logout_warning":false}});
        if let Some(value) = value { json["auto_ratings"] = value.into(); }
        std::fs::write(f.paths.data.join("settings.json"), json.to_string()).unwrap();
        let (settings, reset) = Settings::load(&f.paths.data.join("settings.json"));
        assert!(!reset);
        assert_eq!(settings.auto_ratings, value.unwrap_or(true));
        assert_eq!(f.core().state().settings.auto_ratings, value.unwrap_or(true));
    }
}

#[test]
fn show_ratings_defaults_on_for_stage_three_files_and_round_trips() {
    let f = Fake::new();
    std::fs::write(f.paths.data.join("settings.json"), r#"{"schema":1,"sort":"last_login","dismissed":{"remember_login":false,"logout_warning":false},"after_start":"stay","show_game_start":true,"language":"en"}"#).unwrap();
    assert!(Settings::default().show_ratings);
    let core = f.core();
    assert!(core.state().settings.show_ratings);
    core.set_show_ratings(false);
    assert!(!core.state().settings.show_ratings);
    assert!(!Settings::load(&f.paths.data.join("settings.json")).0.show_ratings);
    assert!(!f.core().state().settings.show_ratings);
    core.set_show_ratings(true);
    assert!(f.core().state().settings.show_ratings);
}
