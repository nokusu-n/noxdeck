//! "Start launcher": flows against the test launcher directory.

mod common;

use common::*;
use noxdeck_lib::registry::LoginState;
use noxdeck_lib::{OpKind, Refusal};
use std::fs;
use std::sync::{Arc, Mutex};

const A: &str = "aaaa0001";
const B: &str = "aaaa0002";

fn seeded() -> Fake {
    let f = Fake::new();
    f.seed_accounts(&accounts_file(&[
        account_json(A, Some("userA@example.com"), Some("Alpha#1111"), "ready"),
        account_json(B, Some("userB@example.com"), Some("Bravo#2222"), "ready"),
    ]));
    f.sim.cache_row(A, "Alpha#1111");
    f.sim.cache_row(B, "Bravo#2222");
    f
}

/// The launcher starts and does `script` on its fresh log; the run is kept for later.
fn launcher_does(f: &Fake, script: impl Fn(&Run) + Send + Sync + 'static) -> Arc<Mutex<Option<Run>>> {
    let kept = Arc::new(Mutex::new(None));
    let (sim, slot) = (f.sim.clone(), kept.clone());
    f.procs.on_start(move || {
        let run = sim.new_run();
        script(&run);
        *slot.lock().unwrap() = Some(run);
    });
    kept
}

#[test]
fn saved_login_works() {
    let f = seeded();
    let before = String::from_utf8(f.config_bytes()).unwrap();
    launcher_does(&f, |r| {
        r.flicker();
        r.login(A);
    });
    let core = f.core();
    core.start_launcher(A).unwrap();
    let during = core.state().operation.expect("a start is running");
    assert_eq!((during.kind, during.account_id.as_deref()), (OpKind::Start, Some(A)));
    wait_idle(&core);

    let s = core.state();
    let a = account(&s, A);
    assert_eq!(a.login_state, LoginState::Ready);
    assert!(a.is_current && a.last_login_at.is_some() && a.last_outcome.is_none());
    assert!(!account(&s, B).is_current);
    assert_eq!(f.procs.calls(), vec!["start_launcher"]);
    // Only the value of the saved list changed, and the original is backed up.
    assert_eq!(
        String::from_utf8(f.config_bytes()).unwrap(),
        before.replace(&f.sim.list().map(|_| "user1@example.com,user2@example.com,user3@example.com,user4@example.com,user5@example.com").unwrap(), "userA@example.com")
    );
    assert_eq!(fs::read_to_string(f.backup_file()).unwrap(), before);
    // The interface saw the record "logging in" before it saw the end.
    let states = f.states.lock().unwrap();
    assert!(states.iter().any(|s| s.operation.as_ref().is_some_and(|o| o.account_id.as_deref() == Some(A))));
    assert!(states.last().unwrap().operation.is_none());
}

#[test]
fn launcher_open_on_another_account_is_closed_and_reopened() {
    let f = seeded();
    let old = f.sim.new_run();
    old.login(B);
    f.procs.set_running(true);
    launcher_does(&f, |r| r.login(A));
    let core = f.core();
    assert!(account(&core.state(), B).is_current);

    core.start_launcher(A).unwrap();
    wait_idle(&core);
    let s = core.state();
    assert!(account(&s, A).is_current && !account(&s, B).is_current);
    assert_eq!(f.procs.calls(), vec!["stop_launcher", "start_launcher"]);
}

#[test]
fn ends_on_the_login_screen() {
    let f = seeded();
    launcher_does(&f, |r| r.browser("LoginCredential"));
    let core = f.core();
    core.start_launcher(A).unwrap();
    wait_idle(&core);
    let a = core.state().accounts.into_iter().find(|a| a.id == A).unwrap();
    assert_eq!(a.login_state, LoginState::PasswordWanted);
    assert!(a.last_outcome.is_none() && !a.is_current);
}

#[test]
fn ends_on_the_code_screen() {
    let f = seeded();
    launcher_does(&f, |r| r.browser("Challenge"));
    let core = f.core();
    core.start_launcher(A).unwrap();
    wait_idle(&core);
    assert_eq!(account(&core.state(), A).login_state, LoginState::CodeWanted);
}

#[test]
fn no_answer_leaves_the_state_and_marks_the_record() {
    let f = seeded();
    launcher_does(&f, |_| {});
    let core = f.core();
    core.start_launcher(A).unwrap();
    wait_idle(&core);
    let s = core.state();
    assert_eq!(account(&s, A).login_state, LoginState::Ready);
    assert_eq!(account(&s, A).last_outcome, Some("no_answer"));
    // The next operation on the record clears the mark.
    core.start_launcher(A).unwrap();
    assert_eq!(account(&core.state(), A).last_outcome, None);
    wait_idle(&core);
}

#[test]
fn a_login_completed_later_turns_the_record_ready() {
    let f = seeded();
    let kept = launcher_does(&f, |r| r.browser("LoginCredential"));
    let core = f.core();
    core.start_launcher(A).unwrap();
    wait_idle(&core);
    assert_eq!(account(&core.state(), A).login_state, LoginState::PasswordWanted);

    kept.lock().unwrap().as_ref().unwrap().login(A);
    wait_for(&core, "the record to be ready and open", |s| {
        let a = account(s, A);
        a.login_state == LoginState::Ready && a.is_current
    });
}

#[test]
fn a_running_game_does_not_change_a_start_or_stop_the_game() {
    let f = seeded();
    f.sim.new_run().login(B);
    f.procs.set_running(true);
    f.procs.set_game(true);
    launcher_does(&f, |r| r.login(A));
    let core = f.core();
    core.start_launcher(A).unwrap();
    wait_idle(&core);
    assert_eq!(f.procs.calls(), vec!["stop_launcher", "start_launcher"]);
    assert!(account(&core.state(), A).is_current);
    assert!(core.state().game_running);
    assert_eq!(f.sim.list().as_deref(), Some("userA@example.com"));
}

#[test]
fn a_start_on_the_current_account_is_refused_without_changes() {
    let f = seeded();
    let run = f.sim.new_run();
    run.login(A);
    f.procs.set_running(true);
    let core = f.core();
    let before = f.config_bytes();
    let state = core.state();
    assert_eq!(core.start_launcher(A), Err(Refusal::AlreadyOpen));
    assert_eq!(serde_json::to_string(&Refusal::AlreadyOpen).unwrap(), "\"already_open\"");
    assert_eq!(core.state(), state);
    assert_eq!(f.config_bytes(), before);
    assert!(f.procs.calls().is_empty() && !f.backup_file().exists());

    run.logout();
    launcher_does(&f, |r| r.login(A));
    core.start_launcher(A).unwrap();
    wait_idle(&core);
    assert!(account(&core.state(), A).is_current);
    f.procs.set_running(false);
    core.start_launcher(A).unwrap();
    wait_idle(&core);
    assert_eq!(f.procs.calls(), vec!["stop_launcher", "start_launcher", "start_launcher"]);
}

#[test]
fn a_second_operation_is_refused_as_busy() {
    let f = seeded();
    launcher_does(&f, |_| {});
    let core = f.core();
    core.start_launcher(A).unwrap();
    assert_eq!(core.start_launcher(B), Err(Refusal::Busy));
    assert_eq!(core.add_account(), Err(Refusal::Busy));
    assert_eq!(core.save_current(), Err(Refusal::Busy));
    assert_eq!(core.delete_account(A), Err(Refusal::Busy));
    wait_idle(&core);
}

#[test]
fn a_record_without_an_email_cannot_start() {
    let f = Fake::new();
    f.seed_accounts(&accounts_file(&[account_json(A, None, Some("Alpha#1111"), "ready")]));
    let core = f.core();
    assert_eq!(core.start_launcher(A), Err(Refusal::NoEmail));
    assert!(f.procs.calls().is_empty() && !f.backup_file().exists());
}

#[test]
fn a_launcher_that_cannot_be_stopped_changes_nothing() {
    let f = seeded();
    f.procs.set_running(true);
    f.procs.deny_stop(true);
    let before = f.config_bytes();
    let core = f.core();
    assert_eq!(core.start_launcher(A), Err(Refusal::LauncherNotClosable));
    assert!(core.state().operation.is_none());
    assert_eq!(f.config_bytes(), before);
    assert_eq!(f.procs.calls(), vec!["stop_launcher"]);
}

#[test]
fn unknown_record_and_missing_launcher_are_refused() {
    let f = seeded();
    let core = f.core();
    assert_eq!(core.start_launcher("ffff0000"), Err(Refusal::UnknownAccount));
    fs::remove_file(f.install.join("Battle.net.exe")).unwrap();
    assert_eq!(core.start_launcher(A), Err(Refusal::LauncherNotFound));
    assert_eq!(core.state().launcher, noxdeck_lib::LauncherKind::NotFound);
}
