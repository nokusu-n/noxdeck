mod common;
use common::*;
use noxdeck_lib::registry::LoginState;
use noxdeck_lib::{Core, LauncherKind, OpKind, Phase, Refusal};
use std::sync::Arc;
use std::time::{Duration, Instant};

const A: &str = "aaaa0001";
const B: &str = "bbbb0002";

fn seeded() -> Fake {
    let f = Fake::new();
    f.seed_accounts(&accounts_file(&[
        account_json(A, Some("userA@example.com"), Some("Alpha#1001"), "ready"),
        account_json(B, Some("userB@example.com"), Some("Bravo#2002"), "ready"),
    ]));
    f
}

fn playable_login(run: &Run, id: &str) {
    run.login(id);
    run.line("InstallState (prometheus): installed=1 playable=1");
}

fn login_on_start(f: &Fake, id: &'static str) {
    let sim = f.sim.clone();
    f.procs.on_start(move || {
        let run = sim.new_run();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            playable_login(&run, id);
        });
    });
}

fn game_on_command(f: &Fake, exits: bool) {
    let procs = Arc::clone(&f.procs);
    f.procs.on_launch(move || {
        procs.set_game(true);
        if exits {
            procs.set_running(false);
        }
    });
}

fn assert_started(core: &Core, id: &str) {
    let s = core.state();
    assert_eq!(s.launcher, LauncherKind::Closed);
    assert!(s.game_running);
    assert_eq!(account(&s, id).login_state, LoginState::Ready);
    assert_eq!(account(&s, id).last_outcome, Some("game_started"));
    assert!(!account(&s, id).is_current);
}

#[test]
fn closed_launcher_starts_game_and_only_saved_names_change() {
    let f = seeded();
    let before = f.config_bytes();
    login_on_start(&f, A);
    game_on_command(&f, false);
    let core = f.core();
    core.start_game(A, false).unwrap();
    wait_idle(&core);
    assert_started(&core, A);
    assert!(account(&core.state(), A).last_login_at.is_some());
    let mut old: serde_json::Value = serde_json::from_slice(&before).unwrap();
    let mut new: serde_json::Value = serde_json::from_slice(&f.config_bytes()).unwrap();
    fn erase_list(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(map) => {
                map.remove("SavedAccountNames");
                for v in map.values_mut() {
                    erase_list(v);
                }
            }
            serde_json::Value::Array(list) => {
                for v in list {
                    erase_list(v);
                }
            }
            _ => {}
        }
    }
    erase_list(&mut old);
    erase_list(&mut new);
    assert_eq!(old, new);
    assert_eq!(std::fs::read(f.backup_file()).unwrap(), before);
    assert_eq!(f.sim.list().as_deref(), Some("userA@example.com"));
    let states = f.states.lock().unwrap();
    for phase in [
        Phase::WaitingLogin,
        Phase::LaunchingGame,
        Phase::ClosingLauncher,
    ] {
        assert!(states.iter().any(|s| s
            .operation
            .as_ref()
            .is_some_and(|op| op.kind == OpKind::Game && op.phase == phase)));
    }
    assert!(!f.procs.calls().iter().any(|c| c == "stop_game"));
}

#[test]
fn another_account_is_stopped_before_starting_the_wanted_account() {
    let f = seeded();
    playable_login(&f.sim.new_run(), A);
    f.procs.set_running(true);
    login_on_start(&f, B);
    game_on_command(&f, false);
    let core = f.core();
    core.start_game(B, false).unwrap();
    wait_idle(&core);
    assert_started(&core, B);
    let calls = f.procs.calls();
    let operations: Vec<_> = calls
        .iter()
        .filter(|c| c.as_str() != "hide_windows")
        .map(String::as_str)
        .collect();
    assert_eq!(
        operations,
        [
            "stop_launcher",
            "start_launcher",
            "send_launch_command",
            "stop_launcher"
        ]
    );
    assert_eq!(f.sim.list().as_deref(), Some("userB@example.com"));
}

#[test]
fn wanted_account_is_reused_without_restart_hiding_or_config_write() {
    let f = seeded();
    playable_login(&f.sim.new_run(), A);
    f.procs.set_running(true);
    game_on_command(&f, false);
    let before = f.config_bytes();
    let core = f.core();
    let since = Instant::now();
    core.start_game(A, false).unwrap();
    wait_idle(&core);
    assert_started(&core, A);
    assert!(since.elapsed() >= core.timing.game_settle);
    assert_eq!(f.procs.calls(), ["send_launch_command", "stop_launcher"]);
    assert_eq!(f.config_bytes(), before);
    assert!(!f.backup_file().exists());
    assert!(
        account(&core.state(), A).last_login_at.is_none(),
        "reuse is not a new login"
    );
}

#[test]
fn launcher_exiting_itself_after_game_start_is_success() {
    let f = seeded();
    login_on_start(&f, A);
    game_on_command(&f, true);
    let core = f.core();
    core.start_game(A, false).unwrap();
    wait_idle(&core);
    assert_started(&core, A);
    assert!(!f
        .procs
        .calls()
        .iter()
        .any(|c| c == "stop_launcher" || c == "show_windows"));
}

#[test]
fn command_is_sent_only_after_the_wanted_login_event() {
    let f = seeded();
    let sim = f.sim.clone();
    f.procs.on_start(move || {
        let run = sim.new_run();
        playable_login(&run, B);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            playable_login(&run, A);
        });
    });
    let core = f.core();
    let weak = Arc::downgrade(&core);
    let procs = Arc::clone(&f.procs);
    f.procs.on_launch(move || {
        assert!(
            account(&weak.upgrade().unwrap().state(), A).is_current,
            "must not launch on B"
        );
        procs.set_game(true);
    });
    core.start_game(A, false).unwrap();
    wait_idle(&core);
    assert_started(&core, A);
    assert_eq!(
        f.procs
            .calls()
            .iter()
            .filter(|c| c.as_str() == "send_launch_command")
            .count(),
        1
    );
}

#[test]
fn hiding_covers_start_login_game_wait_and_settling_then_ends() {
    let f = seeded();
    let procs = Arc::clone(&f.procs);
    let sim = f.sim.clone();
    f.procs.on_start(move || {
        assert!(
            procs.is_hidden(),
            "hide loop must begin before spawning the launcher"
        );
        let run = sim.new_run();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(60));
            playable_login(&run, A);
        });
    });
    let procs = Arc::clone(&f.procs);
    f.procs.on_launch(move || {
        assert!(procs.is_hidden());
        let procs = Arc::clone(&procs);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            assert!(procs.is_hidden());
            procs.set_game(true);
        });
    });
    let core = f.core();
    core.start_game(A, false).unwrap();
    wait_idle(&core);
    assert_started(&core, A);
    let count = f
        .procs
        .calls()
        .iter()
        .filter(|c| c.as_str() == "hide_windows")
        .count();
    assert!(
        count >= 10,
        "hiding must repeat through the settling period: {count}"
    );
    std::thread::sleep(Duration::from_millis(40));
    assert_eq!(
        f.procs
            .calls()
            .iter()
            .filter(|c| c.as_str() == "hide_windows")
            .count(),
        count,
        "hiding must end before the operation ends"
    );
}

#[test]
fn a_game_that_exits_after_being_seen_still_counts_as_started() {
    let f = seeded();
    login_on_start(&f, A);
    let procs = Arc::clone(&f.procs);
    f.procs.on_launch(move || {
        procs.set_game(true);
        let procs = Arc::clone(&procs);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(40));
            procs.set_game(false);
        });
    });
    let core = f.core();
    core.start_game(A, false).unwrap();
    wait_idle(&core);
    let s = core.state();
    assert_eq!(s.launcher, LauncherKind::Closed);
    assert!(!s.game_running);
    assert_eq!(account(&s, A).last_outcome, Some("game_started"));
}

fn script_on_start(
    f: &Fake,
    script: impl Fn(&Run) + Send + Sync + 'static,
) -> Arc<std::sync::Mutex<Option<Run>>> {
    let seen = Arc::new(std::sync::Mutex::new(None));
    let copy = Arc::clone(&seen);
    let sim = f.sim.clone();
    f.procs.on_start(move || {
        let run = sim.new_run();
        script(&run);
        *copy.lock().unwrap() = Some(run);
    });
    seen
}

fn assert_revealed(f: &Fake, core: &Core) {
    assert!(!f.procs.is_hidden());
    assert_eq!(
        f.procs.calls().last().map(String::as_str),
        Some("show_windows")
    );
    assert!(!core.state().game_running);
    assert_ne!(core.state().launcher, LauncherKind::Closed);
    let count = f.procs.calls().len();
    std::thread::sleep(Duration::from_millis(40));
    assert_eq!(
        f.procs.calls().len(),
        count,
        "no hiding after showing the launcher"
    );
}

#[test]
fn not_playable_reveals_the_launcher_without_sending_a_command_or_auto_starting_after_update() {
    let f = seeded();
    let seen = script_on_start(&f, |run| run.login(A));
    let core = f.core();
    core.start_game(A, false).unwrap();
    wait_idle(&core);
    assert_eq!(
        account(&core.state(), A).last_outcome,
        Some("game_needs_update")
    );
    assert_revealed(&f, &core);
    seen.lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .line("InstallState (prometheus): playable=1");
    core.tick();
    assert!(!f.procs.calls().iter().any(|c| c == "send_launch_command"));
    assert!(!core.state().game_running);
}

#[test]
fn login_screen_reveals_password_wanted_and_a_later_login_does_not_start_the_game() {
    let f = seeded();
    let seen = script_on_start(&f, |run| run.browser("LoginCredential"));
    let core = f.core();
    core.start_game(A, false).unwrap();
    wait_idle(&core);
    assert_eq!(
        account(&core.state(), A).login_state,
        LoginState::PasswordWanted
    );
    assert_eq!(account(&core.state(), A).last_outcome, None);
    assert_revealed(&f, &core);
    seen.lock().unwrap().as_ref().unwrap().login(A);
    core.tick();
    assert_eq!(account(&core.state(), A).login_state, LoginState::Ready);
    assert!(!f.procs.calls().iter().any(|c| c == "send_launch_command"));
}

#[test]
fn code_screen_reveals_code_wanted_and_leaves_the_launcher_open() {
    let f = seeded();
    script_on_start(&f, |run| run.browser("Challenge"));
    let core = f.core();
    core.start_game(A, false).unwrap();
    wait_idle(&core);
    assert_eq!(
        account(&core.state(), A).login_state,
        LoginState::CodeWanted
    );
    assert_eq!(account(&core.state(), A).last_outcome, None);
    assert_revealed(&f, &core);
    assert!(!f.procs.calls().iter().any(|c| c == "send_launch_command"));
}

#[test]
fn no_answer_is_bounded_revealed_and_clears_on_manual_login_without_starting_game() {
    let f = seeded();
    let seen = script_on_start(&f, |_| {});
    let core = f.core();
    let since = Instant::now();
    core.start_game(A, false).unwrap();
    wait_idle(&core);
    assert!(since.elapsed() >= core.timing.start_timeout);
    assert!(since.elapsed() < core.timing.start_timeout + Duration::from_millis(300));
    assert_eq!(account(&core.state(), A).last_outcome, Some("no_answer"));
    assert_revealed(&f, &core);
    seen.lock().unwrap().as_ref().unwrap().login(A);
    core.tick();
    assert_eq!(
        account(&core.state(), A).last_outcome,
        None,
        "the manually completed login must be ready in the interface"
    );
    assert!(!f.procs.calls().iter().any(|c| c == "send_launch_command"));
}

#[test]
fn no_game_after_the_command_is_bounded_and_reveals_game_not_started() {
    let f = seeded();
    script_on_start(&f, |run| playable_login(run, A));
    let core = f.core();
    let since = Instant::now();
    core.start_game(A, false).unwrap();
    wait_idle(&core);
    assert!(since.elapsed() >= core.timing.game_timeout);
    assert!(since.elapsed() < core.timing.game_timeout + Duration::from_millis(300));
    assert_eq!(
        account(&core.state(), A).last_outcome,
        Some("game_not_started")
    );
    assert_revealed(&f, &core);
    assert_eq!(
        f.procs
            .calls()
            .iter()
            .filter(|c| c.as_str() == "send_launch_command")
            .count(),
        1
    );
}

#[test]
fn unknown_readiness_still_requests_the_game() {
    let f = seeded();
    script_on_start(&f, |run| {
        run.line(&format!(
            "Logged into Battle.net successfully. |bnet=1:0:{A}|game=2:0:00000000"
        ))
    });
    game_on_command(&f, false);
    let core = f.core();
    core.start_game(A, false).unwrap();
    wait_idle(&core);
    assert_started(&core, A);
    assert!(f.procs.calls().iter().any(|c| c == "send_launch_command"));
}

#[test]
fn an_unplayable_reused_launcher_is_not_hidden_or_restarted() {
    let f = seeded();
    f.sim.new_run().login(A);
    f.procs.set_running(true);
    let before = f.config_bytes();
    let core = f.core();
    core.start_game(A, false).unwrap();
    wait_idle(&core);
    assert_eq!(
        account(&core.state(), A).last_outcome,
        Some("game_needs_update")
    );
    assert_eq!(f.procs.calls(), ["show_windows"]);
    assert_eq!(f.config_bytes(), before);
}

#[test]
fn a_launcher_gone_before_login_ends_after_grace_and_later_login_starts_nothing() {
    let f = seeded();
    let procs = Arc::clone(&f.procs);
    script_on_start(&f, move |_| procs.set_running(false));
    let core = f.core();
    let since = Instant::now();
    core.start_game(A, false).unwrap();
    wait_idle(&core);
    assert!(since.elapsed() >= core.timing.gone_grace);
    assert!(since.elapsed() < core.timing.start_timeout);
    assert_eq!(account(&core.state(), A).last_outcome, Some("no_answer"));
    assert!(!f.procs.is_hidden());
    f.procs.set_running(true);
    playable_login(&f.sim.new_run(), A);
    core.tick();
    assert_eq!(account(&core.state(), A).last_outcome, None);
    assert!(!f.procs.calls().iter().any(|c| c == "send_launch_command"));
}

#[test]
fn a_running_game_is_refused_without_touching_launcher_config_windows_or_game() {
    let f = seeded();
    playable_login(&f.sim.new_run(), A);
    f.procs.set_running(true);
    f.procs.set_game(true);
    let core = f.core();
    let before = f.config_bytes();
    assert_eq!(core.start_game(B, false), Err(Refusal::GameRunning));
    assert_eq!(f.config_bytes(), before);
    assert!(!f.backup_file().exists());
    assert!(f.procs.calls().is_empty());
    assert!(account(&core.state(), A).is_current);
    assert!(core.state().game_running);
    assert!(core.state().operation.is_none());
    assert_eq!(
        core.start_game("missing", false),
        Err(Refusal::UnknownAccount)
    );
    assert!(f.procs.calls().is_empty());
}

#[test]
fn consent_stops_the_game_before_switching_launcher_and_starting_a_new_game() {
    let f = seeded();
    playable_login(&f.sim.new_run(), A);
    f.procs.set_running(true);
    f.procs.set_game(true);
    login_on_start(&f, B);
    game_on_command(&f, false);
    let core = f.core();
    core.start_game(B, true).unwrap();
    wait_idle(&core);
    assert_started(&core, B);
    let calls = f.procs.calls();
    let operations: Vec<_> = calls
        .iter()
        .filter(|c| c.as_str() != "hide_windows")
        .map(String::as_str)
        .collect();
    assert_eq!(
        operations,
        [
            "stop_game",
            "stop_launcher",
            "start_launcher",
            "send_launch_command",
            "stop_launcher"
        ]
    );
}

#[test]
fn failure_to_stop_the_game_releases_the_guard_and_touches_nothing_else() {
    let f = seeded();
    playable_login(&f.sim.new_run(), A);
    f.procs.set_running(true);
    f.procs.set_game(true);
    f.procs.deny_game_stop(true);
    let core = f.core();
    let before = f.config_bytes();
    assert_eq!(core.start_game(B, true), Err(Refusal::GameNotClosable));
    assert_eq!(f.procs.calls(), ["stop_game"]);
    assert_eq!(f.config_bytes(), before);
    assert!(core.state().game_running);
    assert!(account(&core.state(), A).is_current);
    assert!(core.state().operation.is_none());
}

#[test]
fn game_consent_is_checked_before_acquiring_the_shared_operation_guard() {
    let f = seeded();
    f.procs.set_game(true);
    let core = f.core();
    assert_eq!(core.start_game(A, false), Err(Refusal::GameRunning));
    core.start_game(A, true).unwrap();
    assert_eq!(core.start_game(B, true), Err(Refusal::Busy));
    assert_eq!(core.start_launcher(B), Err(Refusal::Busy));
    assert_eq!(core.add_account(), Err(Refusal::Busy));
    assert_eq!(core.delete_account(B), Err(Refusal::Busy));
    wait_idle(&core);
}

#[test]
fn a_target_login_followed_by_another_login_in_the_same_poll_never_launches_on_the_other_account() {
    let f = seeded();
    script_on_start(&f, |run| {
        playable_login(run, A);
        playable_login(run, B);
    });
    game_on_command(&f, false);
    let core = f.core();
    core.start_game(A, false).unwrap();
    wait_idle(&core);
    assert!(
        !f.procs.calls().iter().any(|c| c == "send_launch_command"),
        "A's earlier login does not mean the launcher is still on A"
    );
    assert_eq!(account(&core.state(), A).last_outcome, Some("no_answer"));
    assert!(account(&core.state(), B).is_current);
}
