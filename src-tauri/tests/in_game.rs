mod common;
use common::*;
use noxdeck_lib::{Core, LauncherKind, Phase, Refusal, tray, launcher::Processes, shell::ShellRequest};
use std::fs;
use std::path::Path;

const A: &str = "aaaa0001";
const B: &str = "aaaa0002";

fn seeded() -> Fake {
    let f = Fake::new();
    f.seed_accounts(&accounts_file(&[
        account_json(A, Some("userA@example.com"), None, "ready"),
        account_json(B, Some("userB@example.com"), None, "ready"),
    ]));
    f
}

fn launch(run: &Run) {
    run.line("Launched C:\\StandIn\\FakeGame.exe with args: -uid prometheus, pid: 9999");
}

fn marked(core: &Core, id: &str) {
    let s = core.state();
    assert!(account(&s, id).is_in_game, "{id} must be in game");
    assert_eq!(s.accounts.iter().filter(|a| a.is_in_game).count(), 1);
    let json = serde_json::to_value(&s).unwrap();
    assert_eq!(
        json["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["id"] == id)
            .unwrap()["isInGame"],
        true
    );
}

fn saved(path: &Path) -> serde_json::Value {
    serde_json::from_slice(&fs::read(path.join("state.json")).unwrap()).unwrap()
}

fn confirmed(f: &Fake) -> (std::sync::Arc<Core>, Run) {
    let core = f.core();
    let run = f.sim.new_run();
    run.login(A);
    launch(&run);
    f.procs.set_running(true);
    f.procs.set_game_pids(vec![7001]);
    core.tick();
    marked(&core, A);
    (core, run)
}

#[test]
fn command_launch_fixture_confirms_from_the_same_process_snapshot() {
    let f = seeded();
    let core = f.core();
    f.sim
        .new_run()
        .raw(&fixture("launch-command-launcher-stays.log"));
    f.procs.set_running(true);
    f.procs.set_game_pids(vec![7002]);
    f.procs.clear_queries();
    core.tick();
    marked(&core, B);
    assert_eq!(f.procs.queries(), ["snapshot"]);
    assert_eq!(
        saved(&f.paths.data),
        serde_json::json!({"schema":1,"game":{"account_id":B,"pid":7002}})
    );
    assert!(!f.paths.data.join("state.json.tmp").exists());
}

#[test]
fn manual_launch_fixture_survives_launcher_exit_before_the_tick() {
    let f = seeded();
    let core = f.core();
    f.sim
        .new_run()
        .raw(&fixture("launch-by-hand-launcher-exits.log"));
    f.procs.set_game_pids(vec![7003]);
    core.tick();
    marked(&core, B);
    assert_eq!(core.state().launcher, LauncherKind::Closed);
    assert!(!account(&core.state(), B).is_current);
}

#[test]
fn launch_uses_the_session_at_the_event_before_a_later_login() {
    let f = seeded();
    let core = f.core();
    let run = f.sim.new_run();
    run.login(A);
    launch(&run);
    run.login(B);
    f.procs.set_running(true);
    f.procs.set_game(true);
    core.tick();
    marked(&core, A);
    assert!(account(&core.state(), B).is_current);
}

#[test]
fn confirmed_account_survives_launcher_logout_switch_and_restart() {
    let f = seeded();
    let (core, run) = confirmed(&f);
    run.logout();
    run.login(B);
    core.tick();
    marked(&core, A);
    assert!(account(&core.state(), B).is_current);
    f.procs.set_running(false);
    core.tick();
    f.sim.new_run().login(B);
    f.procs.set_running(true);
    core.tick();
    marked(&core, A);
}

#[test]
fn candidate_waits_for_a_process_and_survives_launcher_exit() {
    let f = seeded();
    let core = f.core();
    let run = f.sim.new_run();
    run.login(A);
    launch(&run);
    core.tick();
    assert!(!account(&core.state(), A).is_in_game);
    f.procs.set_game_pids(vec![7004]);
    core.tick();
    marked(&core, A);
}

#[test]
fn later_launch_replaces_an_unconfirmed_candidate() {
    let f = seeded();
    let core = f.core();
    let run = f.sim.new_run();
    run.login(A);
    launch(&run);
    core.tick();
    run.login(B);
    launch(&run);
    f.procs.set_game(true);
    core.tick();
    marked(&core, B);
}

#[test]
fn remembered_pid_disappearing_clears_the_mark_even_with_another_game_running() {
    let f = seeded();
    let (core, _) = confirmed(&f);
    f.procs.set_game_pids(vec![8001]);
    core.tick();
    assert!(core.state().game_running);
    assert!(!account(&core.state(), A).is_in_game);
    assert_eq!(saved(&f.paths.data)["game"], serde_json::Value::Null);
}

#[test]
fn game_ending_clears_the_mark_and_persisted_pair() {
    let f = seeded();
    let (core, _) = confirmed(&f);
    f.procs.set_game(false);
    core.tick();
    assert!(!account(&core.state(), A).is_in_game);
    assert_eq!(saved(&f.paths.data)["game"], serde_json::Value::Null);
}

#[test]
fn historic_launch_counts_when_a_game_runs_in_the_first_snapshot() {
    let f = seeded();
    f.sim
        .new_run()
        .raw(&fixture("launch-by-hand-launcher-exits.log"));
    f.procs.set_game_pids(vec![7005]);
    marked(&f.core(), B);
}

#[test]
fn historic_launch_without_a_game_is_not_a_candidate_for_a_later_process() {
    let f = seeded();
    let run = f.sim.new_run();
    run.login(A);
    launch(&run);
    let core = f.core();
    f.procs.set_game(true);
    core.tick();
    assert!(!account(&core.state(), A).is_in_game);
    run.login(B);
    launch(&run);
    core.tick();
    marked(&core, B);
}

#[test]
fn restart_restores_the_pair_even_without_launcher_logs() {
    let f = seeded();
    let (core, _) = confirmed(&f);
    drop(core);
    fs::remove_dir_all(f.paths.logs_dir()).unwrap();
    fs::create_dir_all(f.paths.logs_dir()).unwrap();
    marked(&f.core(), A);
}

#[test]
fn restart_drops_a_stale_pid_without_guessing_from_old_logs() {
    let f = seeded();
    let (core, _) = confirmed(&f);
    drop(core);
    f.procs.set_game_pids(vec![8002]);
    let restarted = f.core();
    assert!(restarted.state().game_running);
    assert!(!account(&restarted.state(), A).is_in_game);
    assert_eq!(saved(&f.paths.data)["game"], serde_json::Value::Null);
}

#[test]
fn deleting_the_record_drops_the_pair_and_cannot_resurrect_it() {
    let f = seeded();
    let (core, _) = confirmed(&f);
    core.delete_account(A).unwrap();
    assert_eq!(saved(&f.paths.data)["game"], serde_json::Value::Null);
    core.tick();
    assert!(core.state().accounts.iter().all(|a| !a.is_in_game));
    drop(core);
    assert!(f.core().state().accounts.iter().all(|a| !a.is_in_game));
}

#[test]
fn deleting_a_candidate_prevents_a_mark_if_the_record_is_added_again() {
    let f = seeded();
    let core = f.core();
    let run = f.sim.new_run();
    run.login(A);
    launch(&run);
    f.procs.set_running(true);
    core.tick();
    core.delete_account(A).unwrap();
    assert_eq!(core.save_current().unwrap(), A);
    f.procs.set_game(true);
    core.tick();
    assert!(!account(&core.state(), A).is_in_game);
    run.login(B);
    launch(&run);
    core.tick();
    marked(&core, B);
}

#[test]
fn playing_account_without_consent_gets_the_question_and_touches_nothing() {
    let f = seeded();
    let (core, _) = confirmed(&f);
    f.procs.clear_calls();
    let config = f.config_bytes();
    let registry = fs::read(f.paths.data.join("accounts.json")).unwrap();
    assert_eq!(core.start_game(A, false), Err(Refusal::GameRunning));
    assert!(f.procs.calls().is_empty());
    assert!(core.state().operation.is_none());
    assert_eq!(f.config_bytes(), config);
    assert_eq!(fs::read(f.paths.data.join("accounts.json")).unwrap(), registry);
    assert_eq!(saved(&f.paths.data)["game"]["pid"], 7001);
}

#[test]
fn restart_stops_the_old_process_before_command_and_marks_the_new_one() {
    for reuse in [true, false] {
        let f = seeded();
        let (core, run) = confirmed(&f);
        run.line("InstallState (prometheus): installed=1 playable=1");
        if !reuse { f.procs.set_running(false); }
        let config = f.config_bytes();
        let latest = std::sync::Arc::new(std::sync::Mutex::new(run));
        let kept = latest.clone();
        let sim = f.sim.clone();
        f.procs.on_start(move || {
            let run = sim.new_run();
            run.login(A);
            run.line("InstallState (prometheus): installed=1 playable=1");
            *kept.lock().unwrap() = run;
        });
        let procs = f.procs.clone();
        let weak = std::sync::Arc::downgrade(&core);
        f.procs.on_launch(move || {
            assert!(!procs.game_running("FakeGame.exe"), "old game must be gone before the command");
            assert!(weak.upgrade().unwrap().state().accounts.iter().all(|a| !a.is_in_game));
            launch(&latest.lock().unwrap());
            procs.set_game_pids(vec![8002]);
        });
        f.procs.clear_calls();
        core.start_game(A, true).unwrap();
        wait_idle(&core);
        marked(&core, A);
        assert_eq!(saved(&f.paths.data)["game"]["pid"], 8002);
        assert_eq!(account(&core.state(), A).last_outcome, Some("game_started"));
        let calls = f.procs.calls();
        assert_eq!(calls.first().map(String::as_str), Some("stop_game"));
        assert_eq!(calls.iter().filter(|c| c.as_str() == "send_launch_command").count(), 1);
        assert_eq!(calls.iter().any(|c| c == "start_launcher"), !reuse);
        if reuse { assert_eq!(f.config_bytes(), config); assert!(!f.backup_file().exists()); }
    }
}

#[test]
fn failed_stop_or_lingering_old_game_leaves_launcher_and_config_untouched() {
    for lingering in [false, true] {
        let f = seeded();
        let (core, _) = confirmed(&f);
        if lingering { f.procs.keep_game_on_stop(true); } else { f.procs.deny_game_stop(true); }
        let config = f.config_bytes();
        f.procs.clear_calls();
        assert_eq!(core.start_game(A, true), Err(Refusal::GameNotClosable));
        assert_eq!(f.procs.calls(), ["stop_game"]);
        assert_eq!(f.config_bytes(), config);
        assert!(!f.backup_file().exists());
        assert_eq!(core.state().launcher, LauncherKind::LoggedIn);
        assert!(core.state().operation.is_none());
        assert_eq!(saved(&f.paths.data)["game"]["pid"], 7001);
    }
}

#[test]
fn old_process_and_old_window_cannot_finish_or_attribute_the_new_start() {
    let f = seeded();
    let (core, run) = confirmed(&f);
    run.line("InstallState (prometheus): installed=1 playable=1");
    let procs = f.procs.clone();
    f.procs.set_game_windows(vec![7001]);
    f.procs.on_launch(move || { launch(&run); procs.set_game_pids(vec![7001]); });
    f.procs.clear_calls();
    core.start_game(A, true).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(60));
    core.tick();
    assert!(core.state().accounts.iter().all(|a| !a.is_in_game));
    assert_eq!(core.state().operation.unwrap().phase, Phase::LaunchingGame);
    f.procs.set_game_pids(vec![7001, 8002]);
    wait_for(&core, "new process enters handover", |s| s.operation.as_ref().is_some_and(|o| o.phase == Phase::ClosingLauncher));
    marked(&core, A);
    assert_eq!(saved(&f.paths.data)["game"]["pid"], 8002);
    std::thread::sleep(std::time::Duration::from_millis(130));
    assert!(!f.procs.calls().iter().any(|c| c == "stop_launcher"), "old window must not satisfy the new handover");
    f.procs.set_game_windows(vec![8002]);
    let window_at = std::time::Instant::now();
    wait_idle(&core);
    assert!(window_at.elapsed() >= core.timing.handover_pause);
    assert_eq!(core.state().launcher, LauncherKind::Closed);
    assert_eq!(saved(&f.paths.data)["game"]["pid"], 8002);
}

#[test]
fn old_pid_only_cannot_count_as_the_new_game_at_the_start_bound() {
    let f = seeded();
    let (core, run) = confirmed(&f);
    run.line("InstallState (prometheus): installed=1 playable=1");
    let procs = f.procs.clone();
    f.procs.on_launch(move || { launch(&run); procs.set_game_pids(vec![7001]); });
    f.procs.clear_calls();
    core.start_game(A, true).unwrap();
    wait_idle(&core);
    assert!(core.state().accounts.iter().all(|a| !a.is_in_game));
    assert_eq!(account(&core.state(), A).last_outcome, Some("game_not_started"));
    assert!(!f.procs.calls().iter().any(|c| c == "stop_launcher"));
}

#[test]
fn playing_account_tray_click_asks_and_cancelling_changes_nothing() {
    let f = seeded();
    let (core, _) = confirmed(&f);
    f.procs.clear_calls();
    f.shell.clear();
    assert!(tray::menu(&core.state()).iter().find(|item| item.id == format!("acc-{A}")).unwrap().enabled);
    tray::click(&core, &format!("acc-{A}"));
    assert_eq!(serde_json::to_value(core.state()).unwrap()["ask"], serde_json::json!({"kind":"close_game","accountId":A}));
    assert!(f.shell.requests().contains(&ShellRequest::ShowWindow));
    assert!(f.procs.calls().is_empty());
    core.clear_ask();
    assert!(core.state().ask.is_none());
    assert!(f.procs.calls().is_empty());
    assert_eq!(saved(&f.paths.data)["game"]["pid"], 7001);
}

#[test]
fn unknown_or_logged_out_session_does_not_guess_an_account() {
    let f = seeded();
    let core = f.core();
    let run = f.sim.new_run();
    run.login("cccc0003");
    launch(&run);
    f.procs.set_game(true);
    core.tick();
    assert!(core.state().accounts.iter().all(|a| !a.is_in_game));
    run.login(A);
    run.logout();
    launch(&run);
    core.tick();
    assert!(core.state().accounts.iter().all(|a| !a.is_in_game));
    run.login(B);
    launch(&run);
    core.tick();
    marked(&core, B);
}
