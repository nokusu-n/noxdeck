mod common;
use common::*;
use noxdeck_lib::{settings::Language, tray, LauncherKind, OpKind, OperationView, Phase, State};

fn state() -> State {
    let f = Fake::new();
    f.seed_accounts(&accounts_file(&[account_json("aaaa0001", Some("userA@example.com"), Some("Alpha#1111"), "ready")]));
    let mut state = f.core().state();
    state.lang = Language::En;
    state
}

#[test]
fn open_accounts_exit_with_at_most_five_most_recent_independent_of_list_sort() {
    let mut s = state();
    let template = s.accounts[0].clone();
    s.accounts = (0..8).map(|n| {
        let mut a = template.clone();
        a.id = format!("account-{n}");
        a.last_login_at = (n < 6).then(|| format!("2026-10-0{}T10:00:00Z", n + 1));
        a
    }).collect();
    let menu = tray::menu(&s);
    assert_eq!(menu.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(),
        ["open", "acc-account-5", "acc-account-4", "acc-account-3", "acc-account-2", "acc-account-1", "exit"]);
    assert_eq!(menu[0].label, "Open Noxdeck");
    assert_eq!(menu.last().unwrap().label, "Exit");
}

#[test]
fn fewer_accounts_keep_never_logged_in_last_and_an_empty_list_has_only_open_exit() {
    let mut s = state();
    let mut recent = s.accounts[0].clone();
    recent.id = "recent".into();
    recent.last_login_at = Some("2026-10-01T10:00:00Z".into());
    s.accounts.push(recent);
    assert_eq!(tray::menu(&s).iter().map(|i| i.id.as_str()).collect::<Vec<_>>(), ["open", "acc-recent", "acc-aaaa0001", "exit"]);
    s.accounts.clear();
    assert_eq!(tray::menu(&s).iter().map(|i| i.id.as_str()).collect::<Vec<_>>(), ["open", "exit"]);
}

#[test]
fn labels_use_battletag_or_email_and_both_languages_mark_game_and_launcher() {
    let mut s = state();
    assert_eq!(tray::menu(&s)[1].label, "Alpha#1111");
    s.accounts[0].battletag = None;
    assert_eq!(tray::menu(&s)[1].label, "userA@example.com");
    s.settings.show_game_start = false;
    s.accounts[0].is_current = true;
    assert_eq!(tray::menu(&s)[1].label, "userA@example.com — in Battle.net");
    s.accounts[0].is_in_game = true;
    assert_eq!(tray::menu(&s)[1].label, "userA@example.com — in game");
    s.lang = Language::Ru;
    let menu = tray::menu(&s);
    assert_eq!(menu[0].label, "Открыть Noxdeck");
    assert_eq!(menu[1].label, "userA@example.com — в игре");
    assert_eq!(menu.last().unwrap().label, "Выход");
    s.accounts[0].is_in_game = false;
    assert_eq!(tray::menu(&s)[1].label, "userA@example.com — в Battle.net");
}

#[test]
fn every_fr007_rule_disables_only_affected_items() {
    let base = state();
    for show_game in [true, false] {
        let mut s = base.clone(); s.settings.show_game_start = show_game;
        assert!(tray::menu(&s)[1].enabled);
        s.accounts[0].is_current = true;
        assert_eq!(tray::menu(&s)[1].enabled, show_game);
        s.accounts[0].is_current = false;
        s.accounts[0].is_in_game = true;
        assert!(!tray::menu(&s)[1].enabled);
        s.accounts[0].is_in_game = false;
        s.accounts[0].email = None;
        assert!(!tray::menu(&s)[1].enabled);
    }
    let mut s = base.clone(); s.launcher = LauncherKind::NotFound;
    assert!(!tray::menu(&s)[1].enabled);
    let mut s = base; s.operation = Some(OperationView { kind: OpKind::Add, account_id: None, phase: Phase::WaitingLogin, cancellable: true });
    let menu = tray::menu(&s);
    assert!(!menu[1].enabled);
    assert!(menu[0].enabled && menu.last().unwrap().enabled);
}
