mod common;
use common::*;
use noxdeck_lib::{Core, LauncherKind, Refusal};
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
fn already_in_game_refuses_with_or_without_permission_to_close() {
    let f = seeded();
    let (core, _) = confirmed(&f);
    f.procs.clear_calls();
    for close in [false, true] {
        assert_eq!(core.start_game(A, close), Err(Refusal::AlreadyInGame));
    }
    assert!(f.procs.calls().is_empty());
    assert!(core.state().operation.is_none());
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
