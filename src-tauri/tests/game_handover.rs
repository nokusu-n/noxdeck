mod common;
use common::*;
use noxdeck_lib::{Core, LauncherKind, Phase, settings::{Settings, AfterStart}};
use std::time::{Duration, Instant};

const ID: &str = "aaaa0001";

fn start(reuse: bool, close: bool, visible: bool, bound: Duration, pause: Duration) -> (Fake, std::sync::Arc<Core>) {
    let f = Fake::new();
    f.seed_accounts(&accounts_file(&[account_json(ID, Some("placeholder@example.com"), None, "ready")]));
    Settings { close_launcher: close, after_start: AfterStart::Stay, ..Settings::default() }
        .save(&f.paths.data.join("settings.json")).unwrap();
    if reuse {
        let run = f.sim.new_run();
        run.login(ID);
        run.line("InstallState (prometheus): installed=1 playable=1");
    } else {
        let sim = f.sim.clone();
        f.procs.on_start(move || {
            let run = sim.new_run();
            run.login(ID);
            run.line("InstallState (prometheus): installed=1 playable=1");
        });
    }
    f.procs.set_running(reuse);
    f.procs.set_game_window(visible);
    let procs = f.procs.clone();
    f.procs.on_launch(move || procs.set_game(true));
    let core = Core::new(f.paths.clone(), noxdeck_lib::Timing {
        game_handover: bound, handover_pause: pause, ..fast()
    }, f.procs.clone(), Box::new(|_| {}), "test", f.shell.clone());
    core.start_game(ID, false).unwrap();
    (f, core)
}

fn no_stop(f: &Fake) { assert!(!f.procs.calls().iter().any(|c| c == "stop_launcher")); }
fn success(core: &Core) { assert_eq!(account(&core.state(), ID).last_outcome, Some("game_started")); }

#[test]
fn a_long_lived_process_does_not_close_the_launcher_before_its_window_and_pause() {
    let (f, core) = start(true, true, false, Duration::from_secs(1), Duration::from_millis(150));
    wait_for(&core, "handover phase", |s| s.operation.as_ref().is_some_and(|o| o.phase == Phase::ClosingLauncher));
    std::thread::sleep(Duration::from_millis(350));
    no_stop(&f);
    assert!(core.state().operation.is_some(), "the old five-second settling rule must not finish this wait");
    let visible_at = Instant::now();
    f.procs.set_game_window(true);
    std::thread::sleep(Duration::from_millis(70));
    no_stop(&f);
    wait_idle(&core);
    assert!(visible_at.elapsed() >= core.timing.handover_pause);
    assert_eq!(f.procs.calls(), ["send_launch_command", "stop_launcher"]);
    success(&core);
    let text = core.diagnostics_text();
    assert!(text.contains("GameHandover"));
    assert!(text.contains("HandoverPause"));
    assert!(text.contains("end: Event"));
    assert!(text.contains("end: Elapsed"));
}

#[test]
fn missing_window_at_the_bound_leaves_the_launcher_and_still_succeeds() {
    for reuse in [true, false] {
        let (f, core) = start(reuse, true, false, Duration::from_millis(180), Duration::from_millis(80));
        wait_for(&core, "handover phase", |s| s.operation.as_ref().is_some_and(|o| o.phase == Phase::ClosingLauncher));
        assert_eq!(f.procs.is_hidden(), !reuse);
        wait_idle(&core);
        no_stop(&f);
        success(&core);
        assert_eq!(core.state().launcher, LauncherKind::LoggedIn);
        assert!(core.diagnostics_text().contains("end: Bound"));
        assert!(!f.procs.is_hidden());
        assert!(!f.procs.calls().iter().any(|c| c == "show_windows"));
        assert_eq!(f.procs.calls().iter().filter(|c| *c == "show_windows_no_activate").count(), usize::from(!reuse));
    }
}

#[test]
fn the_bound_also_limits_the_pause_after_a_late_window() {
    let (f, core) = start(true, true, true, Duration::from_millis(100), Duration::from_millis(200));
    wait_idle(&core);
    no_stop(&f);
    success(&core);
    assert!(core.diagnostics_text().contains("what: HandoverPause"));
    assert!(core.diagnostics_text().contains("end: Bound"));
}

#[test]
fn disappearance_during_either_wait_never_stops_the_launcher() {
    for reuse in [true, false] {
        for visible in [false, true] {
            let (f, core) = start(reuse, true, visible, Duration::from_millis(300), Duration::from_millis(200));
            wait_for(&core, "game seen", |s| s.game_running);
            std::thread::sleep(Duration::from_millis(40));
            f.procs.set_game(false);
            wait_idle(&core);
            no_stop(&f);
            success(&core);
            assert_eq!(core.state().launcher, LauncherKind::LoggedIn);
            assert!(!core.state().game_running);
            assert!(core.diagnostics_text().contains("end: Gone"));
            assert!(!f.procs.is_hidden());
            assert!(!f.procs.calls().iter().any(|c| c == "show_windows"));
            assert_eq!(f.procs.calls().iter().filter(|c| *c == "show_windows_no_activate").count(), usize::from(!reuse));
        }
    }
}

#[test]
fn confirmed_handover_stops_a_hidden_launcher_and_shows_no_window() {
    let (f, core) = start(false, true, true, Duration::from_millis(300), Duration::from_millis(80));
    wait_idle(&core);
    success(&core);
    assert_eq!(core.state().launcher, LauncherKind::Closed);
    assert!(f.procs.calls().iter().any(|c| c == "stop_launcher"));
    assert!(!f.procs.calls().iter().any(|c| c == "show_windows" || c == "show_windows_no_activate"));
}

#[test]
fn launcher_self_exit_is_success_without_waiting_for_the_bound() {
    let (f, core) = start(true, true, false, Duration::from_secs(1), Duration::from_millis(200));
    wait_for(&core, "game seen", |s| s.game_running);
    let exited_at = Instant::now();
    f.procs.set_running(false);
    wait_idle(&core);
    no_stop(&f);
    success(&core);
    assert!(exited_at.elapsed() < Duration::from_millis(300));
    assert_eq!(core.state().launcher, LauncherKind::Closed);
}

#[test]
fn close_off_retains_the_existing_settling_time_without_window_or_handover_wait() {
    let (f, core) = start(true, false, false, Duration::from_secs(1), Duration::from_secs(1));
    let begun = Instant::now();
    wait_idle(&core);
    no_stop(&f);
    success(&core);
    assert!(begun.elapsed() >= core.timing.game_settle);
    assert!(begun.elapsed() < Duration::from_millis(400));
    let text = core.diagnostics_text();
    assert!(text.contains("what: GameSettle"));
    assert!(!text.contains("what: GameHandover"));
    assert!(!text.contains("Phase(ClosingLauncher)"));
}
