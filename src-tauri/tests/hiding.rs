mod common;
use common::*;
use noxdeck_lib::{Core, Timing};
use std::sync::Arc;
use std::time::Duration;

fn cold_start(settle: Duration, refresh: Duration) -> Fake {
    let f = Fake::new();
    f.seed_accounts(&accounts_file(&[account_json(
        "aaaa0001",
        Some("userA@example.com"),
        None,
        "ready",
    )]));
    let sim = f.sim.clone();
    f.procs.on_start(move || {
        let run = sim.new_run();
        run.login("aaaa0001");
        run.line("InstallState (prometheus): installed=1 playable=1");
    });
    let procs = Arc::clone(&f.procs);
    f.procs.on_launch(move || procs.set_game(true));
    let timing = noxdeck_lib::Timing {
        game_settle: settle,
        pid_refresh: refresh,
        ..fast()
    };
    let core = Core::new(
        f.paths.clone(),
        timing,
        f.procs.clone(),
        Box::new(|_| {}),
        "test",
        f.shell.clone(),
    );
    core.start_game("aaaa0001", false).unwrap();
    wait_idle(&core);
    f
}

#[test]
fn launcher_start_forces_refresh_before_the_launch_command() {
    let f = cold_start(Duration::from_millis(30), Duration::from_secs(2));
    let trace = f.procs.trace();
    let at = trace.iter().position(|v| *v == "start_launcher").unwrap();
    assert_eq!(trace.get(at + 1), Some(&"refresh_launcher_pids"));
}

#[test]
fn launch_command_forces_another_refresh_immediately_after_sending() {
    let f = cold_start(Duration::from_millis(30), Duration::from_secs(2));
    let trace = f.procs.trace();
    let at = trace
        .iter()
        .position(|v| *v == "send_launch_command")
        .unwrap();
    assert_eq!(trace.get(at + 1), Some(&"refresh_launcher_pids"));
}

#[test]
fn process_refresh_continues_independently_of_the_faster_window_passes() {
    let f = cold_start(Duration::from_millis(160), Duration::from_millis(50));
    let refreshes = f
        .procs
        .queries()
        .iter()
        .filter(|q| **q == "refresh_launcher_pids")
        .count();
    let passes = f
        .procs
        .calls()
        .iter()
        .filter(|q| *q == "hide_windows")
        .count();
    assert!(refreshes >= 4, "periodic refresh did not run: {refreshes}");
    assert!(
        passes > refreshes * 2,
        "window passes must run independently: {passes}/{refreshes}"
    );
}

#[test]
fn production_windows_are_checked_every_10_ms_and_processes_every_250_ms() {
    let timing = Timing::default();
    assert_eq!(timing.hide_interval, Duration::from_millis(10));
    assert_eq!(timing.pid_refresh, Duration::from_millis(250));
}
