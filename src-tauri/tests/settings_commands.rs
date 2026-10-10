mod common;
use common::*;
use noxdeck_lib::{
    settings::{AfterStart, Language, Settings},
    shell::ShellRequest,
    tray,
};

#[test]
fn commands_publish_and_persist_settings_and_update_the_menu_without_a_restart() {
    let f = Fake::new();
    let core = f.core();
    f.shell.clear();
    core.set_after_start(AfterStart::Close);
    core.set_close_launcher(false);
    core.set_language(Some(Language::Ru));
    let state = core.state();
    assert_eq!(state.settings.after_start, AfterStart::Close);
    assert!(!state.settings.close_launcher);
    assert_eq!(state.settings.language, Some(Language::Ru));
    assert_eq!(state.lang, Language::Ru);
    assert_eq!(tray::menu(&state)[0].label, "Открыть Noxdeck");
    assert!(f
        .shell
        .requests()
        .iter()
        .all(|r| *r == ShellRequest::TrayChanged));
    assert!(f.shell.requests().len() >= 3);
    let (stored, reset) = Settings::load(&f.paths.data.join("settings.json"));
    assert!(!reset);
    assert_eq!(stored.after_start, AfterStart::Close);
    assert!(!stored.close_launcher);
    assert_eq!(stored.language, Some(Language::Ru));
    let restarted = f.core();
    assert_eq!(restarted.state().settings, state.settings);
    core.set_language(Some(Language::En));
    assert_eq!(tray::menu(&core.state())[0].label, "Open Noxdeck");
    core.set_language(Some(Language::It));
    assert_eq!(tray::menu(&core.state())[0].label, "Apri Noxdeck");
    assert_eq!(f.core().state().lang, Language::It);
    core.set_language(None);
    assert_eq!(core.state().settings.language, None);
}
