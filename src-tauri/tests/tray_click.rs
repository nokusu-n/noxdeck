mod common;
use common::*;
use noxdeck_lib::{tray, shell::ShellRequest};
use serde_json::json;

fn seed(f: &Fake, show_game: bool) {
    f.seed_accounts(&accounts_file(&[account_json("aaaa0001", Some("userA@example.com"), Some("Alpha#1111"), "ready")]));
    std::fs::write(f.paths.data.join("settings.json"), json!({"schema":1,"sort":"last_login","dismissed":{"remember_login":false,"logout_warning":false},"show_game_start":show_game,"after_start":"stay"}).to_string()).unwrap();
}

fn actions(f: &Fake) -> Vec<ShellRequest> {
    f.shell.requests().into_iter().filter(|r| *r != ShellRequest::TrayChanged).collect()
}

#[test]
fn account_click_calls_the_game_or_launcher_operation_without_a_logout_warning() {
    for show_game in [true, false] {
        let f = Fake::new(); seed(&f, show_game);
        let sim = f.sim.clone();
        f.procs.on_start(move || { let run = sim.new_run(); run.login("aaaa0001"); run.line("InstallState (prometheus): installed=1 playable=1"); });
        let procs = f.procs.clone(); f.procs.on_launch(move || procs.set_game(true));
        let core = f.core(); f.shell.clear();
        tray::click(&core, "acc-aaaa0001");
        wait_idle(&core);
        assert!(f.procs.calls().contains(&"start_launcher".to_string()));
        assert_eq!(f.procs.calls().contains(&"send_launch_command".to_string()), show_game);
        assert!(actions(&f).is_empty());
    }
}

#[test]
fn running_game_sets_ask_shows_the_window_and_clear_ask_changes_no_process() {
    let f = Fake::new(); seed(&f, true); f.procs.set_game(true);
    let core = f.core(); f.shell.clear();
    tray::click(&core, "acc-aaaa0001");
    let state = serde_json::to_value(core.state()).unwrap();
    assert_eq!(state["ask"], json!({"kind":"close_game","accountId":"aaaa0001"}));
    assert_eq!(actions(&f), [ShellRequest::ShowWindow]);
    assert!(f.procs.calls().is_empty());
    core.clear_ask();
    assert_eq!(serde_json::to_value(core.state()).unwrap()["ask"], serde_json::Value::Null);
    assert!(f.procs.calls().is_empty());
}

#[test]
fn any_other_refusal_shows_the_window_and_open_and_exit_have_their_shell_requests() {
    let f = Fake::new(); seed(&f, true); let core = f.core(); f.shell.clear();
    tray::click(&core, "acc-missing");
    assert_eq!(actions(&f), [ShellRequest::ShowWindow]);
    assert_eq!(serde_json::to_value(core.state()).unwrap()["ask"], serde_json::Value::Null);
    f.shell.clear(); tray::click(&core, "open"); tray::click(&core, "exit");
    assert_eq!(actions(&f), [ShellRequest::ShowWindow, ShellRequest::Exit]);
}
