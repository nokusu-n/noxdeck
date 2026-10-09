//! "Save current": flows against the fake launcher directory. It never stops or
//! starts a process.

mod common;

use common::*;
use noxdeck_lib::registry::LoginState;
use noxdeck_lib::Refusal;

const A: &str = "aaaa0001";

fn run_with(f: &Fake, script: impl Fn(&Run)) {
    let run = f.sim.new_run();
    script(&run);
}

fn no_process_calls(f: &Fake) {
    assert!(f.procs.calls().is_empty(), "no process may be stopped or started");
}

#[test]
fn open_and_logged_in() {
    let f = Fake::new();
    f.sim.set_list("userA@example.com");
    f.sim.cache_row(A, "Alpha#1111");
    run_with(&f, |r| r.login(A));
    f.procs.set_running(true);
    let core = f.core();
    assert_eq!(core.save_current(), Ok(A.to_string()));
    let s = core.state();
    let a = account(&s, A);
    assert_eq!(a.email.as_deref(), Some("userA@example.com"));
    assert_eq!(a.battletag.as_deref(), Some("Alpha#1111"));
    assert_eq!(a.login_state, LoginState::Ready);
    assert!(a.is_current);
    no_process_calls(&f);
    assert!(!f.backup_file().exists(), "nothing of the launcher's was touched");
}

#[test]
fn closed_after_a_session_without_logout() {
    let f = Fake::new();
    f.sim.set_list("userA@example.com");
    f.sim.cache_row(A, "Alpha#1111");
    run_with(&f, |r| r.login(A));
    let core = f.core();
    assert_eq!(core.save_current(), Ok(A.to_string()));
    let s = core.state();
    assert_eq!(s.accounts.len(), 1, "only that one");
    assert!(!account(&s, A).is_current, "the launcher is closed");
    no_process_calls(&f);
}

#[test]
fn closed_after_a_logout_or_never_logged_in_or_on_the_login_screen() {
    // After a log-out.
    let f = Fake::new();
    run_with(&f, |r| {
        r.login(A);
        r.logout();
    });
    let core = f.core();
    assert_eq!(core.save_current(), Err(Refusal::NoSavedLogin));
    assert!(core.state().accounts.is_empty());

    // Never logged in: no log at all.
    let f2 = Fake::new();
    assert_eq!(f2.core().save_current(), Err(Refusal::NoSavedLogin));

    // Open, on the login screen.
    let f3 = Fake::new();
    run_with(&f3, |r| r.browser("LoginCredential"));
    f3.procs.set_running(true);
    let core3 = f3.core();
    assert_eq!(core3.save_current(), Err(Refusal::NoSavedLogin));
    no_process_calls(&f);
    no_process_calls(&f2);
    no_process_calls(&f3);
}

#[test]
fn already_listed() {
    let f = Fake::new();
    f.seed_accounts(&accounts_file(&[account_json(A, Some("userA@example.com"), None, "ready")]));
    run_with(&f, |r| r.login(A));
    f.procs.set_running(true);
    let core = f.core();
    assert_eq!(core.save_current(), Err(Refusal::AlreadyListed));
    assert_eq!(core.state().accounts.len(), 1);
    no_process_calls(&f);
}

#[test]
fn launcher_not_found() {
    let f = Fake::new();
    std::fs::remove_file(f.install.join("Battle.net.exe")).unwrap();
    assert_eq!(f.core().save_current(), Err(Refusal::LauncherNotFound));
}
