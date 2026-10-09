mod common;
use common::*;
use serde_json::json;

#[test]
fn language_probe() {
    let Ok(expected) = std::env::var("NOXDECK_LANGUAGE_EXPECTED") else { return };
    let f = Fake::new();
    let language = std::env::var("NOXDECK_LANGUAGE_SAVED").ok();
    let value = json!({"schema":1,"sort":"last_login","dismissed":{"remember_login":false,"logout_warning":false},"language":language});
    std::fs::write(f.paths.data.join("settings.json"), value.to_string()).unwrap();
    let state = serde_json::to_value(f.core().state()).unwrap();
    assert_eq!(state["lang"], expected);
    assert_eq!(state["settings"]["language"], value["language"]);
    assert_eq!(state["settings"]["afterStart"], "tray");
    assert_eq!(state["settings"]["showGameStart"], true);
    assert_eq!(state["settings"]["autostart"], false);
}

fn probe(locale: &str, saved: Option<&str>, expected: &str) {
    use std::os::windows::process::CommandExt;
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command.args(["--exact", "language_probe", "--nocapture"])
        .env("NOXDECK_LOCALE", locale)
        .env("NOXDECK_LANGUAGE_EXPECTED", expected)
        .env_remove("NOXDECK_LANGUAGE_SAVED")
        .creation_flags(0x0800_0000);
    if let Some(saved) = saved { command.env("NOXDECK_LANGUAGE_SAVED", saved); }
    let output = command.output().unwrap();
    assert!(output.status.success(), "locale={locale}, saved={saved:?}\n{}", String::from_utf8_lossy(&output.stdout));
}

#[test]
fn windows_locale_is_russian_only_for_ru() {
    for (locale, expected) in [("ru-RU", "ru"), ("ru-UA", "ru"), ("en-US", "en"), ("de-DE", "en"), ("", "en")] {
        probe(locale, None, expected);
    }
}

#[test]
fn a_saved_language_wins_over_the_windows_locale() {
    probe("ru-RU", Some("en"), "en");
    probe("en-US", Some("ru"), "ru");
}
