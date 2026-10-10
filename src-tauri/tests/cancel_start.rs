mod common;
use common::*;
use noxdeck_lib::{Phase, tray};
use noxdeck_lib::launcher::Processes;
use std::time::Duration;

const ID: &str = "aaaa0001";
fn seeded() -> Fake {
    let f = Fake::new();
    f.seed_accounts(&accounts_file(&[account_json(ID, Some("placeholder@example.com"), Some("Placeholder#1111"), "ready")]));
    f
}
fn login_on_start(f: &Fake) {
    let sim = f.sim.clone();
    f.procs.on_start(move || { let run = sim.new_run(); run.login(ID); run.line("InstallState (prometheus): installed=1 playable=1"); });
}
fn guarded_cancel(core: &noxdeck_lib::Core) {
    std::thread::sleep(core.timing.cancel_guard);
    core.cancel_operation();
    wait_idle(core);
    assert_eq!(account(&core.state(), ID).last_outcome, None);
}

#[test]
fn double_click_is_ignored_then_launcher_wait_can_be_cancelled_and_started_again() {
    let f = seeded();
    let core = f.core();
    core.start_launcher(ID).unwrap();
    core.cancel_operation();
    std::thread::sleep(Duration::from_millis(20));
    assert!(core.state().operation.is_some());
    guarded_cancel(&core);
    assert!(f.procs.launcher_running(&f.install), "a visible launcher is left alone");
    login_on_start(&f);
    core.start_launcher(ID).unwrap();
    wait_idle(&core);
    assert!(account(&core.state(), ID).is_current);
}

#[test]
fn cancelling_a_hidden_launcher_during_login_stops_only_that_launcher() {
    let f = seeded();
    let core = f.core();
    core.start_game(ID, false).unwrap();
    wait_for(&core, "waiting login", |s| s.operation.as_ref().is_some_and(|o| o.phase == Phase::WaitingLogin));
    guarded_cancel(&core);
    assert!(!core.state().game_running);
    assert!(!f.procs.launcher_running(&f.install));
    assert!(!f.procs.calls().iter().any(|c| c == "send_launch_command" || c == "stop_game"));
    login_on_start(&f);
    core.start_game(ID, false).unwrap();
    wait_for(&core, "game wait", |s| s.operation.as_ref().is_some_and(|o| o.phase == Phase::LaunchingGame));
    guarded_cancel(&core);
    assert!(!f.procs.launcher_running(&f.install));
}

#[test]
fn cancelling_a_reused_launcher_through_the_tray_leaves_it_running() {
    let f = seeded();
    let run = f.sim.new_run();
    run.login(ID);
    run.line("InstallState (prometheus): installed=1 playable=1");
    f.procs.set_running(true);
    let core = f.core();
    core.start_game(ID, false).unwrap();
    wait_for(&core, "game wait", |s| s.operation.as_ref().is_some_and(|o| o.phase == Phase::LaunchingGame));
    let item = tray::menu(&core.state()).into_iter().find(|i| i.id == format!("acc-{ID}")).unwrap();
    assert!(item.enabled);
    assert_eq!(item.label, "Cancel");
    std::thread::sleep(core.timing.cancel_guard);
    tray::click(&core, &item.id);
    wait_idle(&core);
    assert!(f.procs.launcher_running(&f.install));
    assert_eq!(account(&core.state(), ID).last_outcome, None);
    assert!(!f.procs.calls().iter().any(|c| c == "stop_launcher"));
}

#[test]
fn cancelling_after_a_game_appears_is_ignored_and_finishes_normally() {
    for close_launcher in [true, false] {
        let f = seeded();
        std::fs::write(f.paths.data.join("settings.json"), serde_json::to_string(&noxdeck_lib::settings::Settings {
            close_launcher, ..Default::default()
        }).unwrap()).unwrap();
        login_on_start(&f);
        f.procs.set_game_window(false);
        let procs = f.procs.clone();
        f.procs.on_launch(move || procs.set_game(true));
        let mut timing = fast();
        timing.game_settle = Duration::from_millis(300);
        let core = noxdeck_lib::Core::new(f.paths.clone(), timing, f.procs.clone(), Box::new(|_| {}), "test", f.shell.clone());
        core.start_game(ID, false).unwrap();
        wait_for(&core, "game process", |s| s.game_running && s.operation.is_some());
        assert!(!core.state().operation.unwrap().cancellable);
        let item = tray::menu(&core.state()).into_iter().find(|i| i.id == format!("acc-{ID}")).unwrap();
        assert!(!item.enabled);
        assert_ne!(item.label, "Cancel");
        std::thread::sleep(core.timing.cancel_guard);
        core.cancel_operation();
        f.procs.set_game_window(true);
        wait_idle(&core);
        assert!(core.state().game_running);
        assert_eq!(account(&core.state(), ID).last_outcome, Some("game_started"));
        assert_eq!(f.procs.launcher_running(&f.install), !close_launcher);
        assert!(!f.procs.calls().iter().any(|c| c == "stop_game"));
        if !close_launcher {
            assert!(!f.procs.is_hidden());
            assert!(f.procs.calls().iter().any(|c| c == "show_windows_no_activate"));
            assert!(!f.procs.calls().iter().any(|c| c == "show_windows"));
        }
    }
}

#[test]
fn a_cancel_winning_the_process_race_reveals_the_kept_launcher() {
    let f = seeded();
    login_on_start(&f);
    let (ready, waiting) = std::sync::mpsc::sync_channel(1);
    let (resume, resumed) = std::sync::mpsc::sync_channel(1);
    let resumed = std::sync::Mutex::new(resumed);
    f.procs.on_launch(move || {
        ready.send(()).unwrap();
        resumed.lock().unwrap().recv().unwrap();
    });
    let core = f.core();
    core.start_game(ID, false).unwrap();
    waiting.recv_timeout(Duration::from_secs(2)).unwrap();
    std::thread::sleep(core.timing.cancel_guard);
    core.cancel_operation();
    f.procs.set_game(true);
    resume.send(()).unwrap();
    wait_idle(&core);
    assert_eq!(account(&core.state(), ID).last_outcome, None);
    assert!(f.procs.launcher_running(&f.install));
    assert!(core.state().game_running);
    assert!(!f.procs.is_hidden());
    assert!(f.procs.calls().iter().any(|c| c == "show_windows_no_activate"));
    assert!(!f.procs.calls().iter().any(|c| c == "stop_launcher" || c == "stop_game"));
}

#[test]
fn a_game_closed_for_a_restart_stays_closed_after_cancel() {
    let f = seeded();
    f.procs.set_game(true);
    let core = f.core();
    core.start_game(ID, true).unwrap();
    guarded_cancel(&core);
    assert!(!core.state().game_running);
    assert_eq!(f.procs.calls().iter().filter(|c| *c == "stop_game").count(), 1);
}

#[test]
fn tray_cancel_remains_available_when_the_starting_account_is_outside_the_five_recent_accounts() {
    let f = seeded();
    let mut accounts = vec![account_json(ID, Some("placeholder@example.com"), None, "ready")];
    for index in 2..=6 {
        let mut value: serde_json::Value = serde_json::from_str(&account_json(&format!("aaaa{index:04}"), Some("other@example.com"), None, "ready")).unwrap();
        value["last_login_at"] = serde_json::json!("2026-10-11T10:00:00Z");
        accounts.push(value.to_string());
    }
    f.seed_accounts(&accounts_file(&accounts));
    let core = f.core();
    core.start_launcher(ID).unwrap();
    let menu = tray::menu(&core.state());
    assert_eq!(menu.iter().filter(|item| item.id.starts_with("acc-")).count(), 5);
    assert!(!menu.iter().any(|item| item.id == format!("acc-{ID}")));
    assert!(menu.iter().any(|item| item.id == "cancel" && item.enabled));
    std::thread::sleep(core.timing.cancel_guard);
    tray::click(&core, "cancel");
    wait_idle(&core);
    assert_eq!(account(&core.state(), ID).last_outcome, None);
}
