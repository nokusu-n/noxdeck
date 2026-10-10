mod common;
use common::*;
use noxdeck_lib::{Core, Phase, Timing};
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
use std::time::{Duration, Instant};

const ID: &str = "aaaa0001";
fn setup() -> (Fake, Arc<Core>, Run) {
    let f = Fake::new();
    f.seed_accounts(&accounts_file(&[account_json(ID, Some("placeholder@example.com"), None, "ready")]));
    let run = f.sim.new_run();
    run.login(ID);
    run.line("InstallState (prometheus): installed=1 playable=1");
    f.procs.set_running(true);
    let core = Core::new(f.paths.clone(), Timing { game_timeout: Duration::from_millis(600), ..fast() }, f.procs.clone(), Box::new(|_| {}), "test", f.shell.clone());
    (f, core, run)
}
fn commands(f: &Fake) -> usize { f.procs.calls().iter().filter(|c| *c == "send_launch_command").count() }

#[test]
fn a_dropped_command_is_resent_once_and_both_commands_are_journalled() {
    let (f, core, _) = setup();
    let count = Arc::new(AtomicUsize::new(0));
    let procs = f.procs.clone();
    f.procs.on_launch(move || { if count.fetch_add(1, Ordering::SeqCst) == 1 { procs.set_game(true); } });
    core.start_game(ID, false).unwrap();
    wait_idle(&core);
    assert_eq!(commands(&f), 2);
    assert_eq!(account(&core.state(), ID).last_outcome, Some("game_started"));
    assert_eq!(core.journal.text().matches("Command(LaunchGame)").count(), 2);
}
#[test]
fn silence_never_sends_a_third_command() {
    let (f, core, _) = setup();
    core.start_game(ID, false).unwrap();
    wait_idle(&core);
    assert_eq!(commands(&f), 2);
    assert_eq!(account(&core.state(), ID).last_outcome, Some("game_not_started"));
}
#[test]
fn the_launchers_new_launch_line_suppresses_a_resend_without_a_process() {
    let (f, core, run) = setup();
    f.procs.on_launch(move || run.line("Launched <game path> -uid prometheus"));
    core.start_game(ID, false).unwrap();
    wait_idle(&core);
    assert_eq!(commands(&f), 1);
}
#[test]
fn an_old_launch_line_does_not_suppress_the_new_operations_retry() {
    let (f, core, run) = setup();
    run.line("Launched <game path> -uid prometheus");
    core.tick();
    core.start_game(ID, false).unwrap();
    wait_idle(&core);
    assert_eq!(commands(&f), 2);
}
#[test]
fn leaving_the_wanted_account_suppresses_a_resend() {
    let (f, core, run) = setup();
    f.procs.on_launch(move || run.login("bbbb0002"));
    core.start_game(ID, false).unwrap();
    wait_idle(&core);
    assert_eq!(commands(&f), 1);
}
#[test]
fn a_gone_launcher_ends_after_the_grace_instead_of_the_game_bound() {
    let (f, core, _) = setup();
    let procs = f.procs.clone();
    f.procs.on_launch(move || procs.set_running(false));
    let started = Instant::now();
    core.start_game(ID, false).unwrap();
    wait_idle(&core);
    assert!(started.elapsed() < Duration::from_millis(400));
    assert_eq!(commands(&f), 1);
    assert_eq!(account(&core.state(), ID).last_outcome, Some("game_not_started"));
    assert!(core.journal.text().contains("end: Gone"));
}
#[test]
fn a_new_game_process_prevents_both_the_retry_and_the_gone_failure() {
    let (f, core, _) = setup();
    let procs = f.procs.clone();
    f.procs.on_launch(move || { procs.set_game(true); procs.set_running(false); });
    core.start_game(ID, false).unwrap();
    wait_idle(&core);
    assert_eq!(commands(&f), 1);
    assert_eq!(account(&core.state(), ID).last_outcome, Some("game_started"));
}
#[test]
fn cancel_prevents_the_retry() {
    let (f, core, _) = setup();
    core.start_game(ID, false).unwrap();
    wait_for(&core, "command sent", |s| s.operation.as_ref().is_some_and(|op| op.phase == Phase::LaunchingGame));
    std::thread::sleep(core.timing.cancel_guard);
    core.cancel_operation();
    wait_idle(&core);
    assert_eq!(commands(&f), 1);
    assert_eq!(account(&core.state(), ID).last_outcome, None);
}
