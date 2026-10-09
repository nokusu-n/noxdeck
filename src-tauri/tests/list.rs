//! Reading and keeping the list: sort, delete, the watcher's rules, persistence.

mod common;

use common::*;
use noxdeck_lib::registry::LoginState;
use noxdeck_lib::settings::Sort;
use noxdeck_lib::{LauncherKind, Refusal};

const A: &str = "aaaa0001";
const B: &str = "aaaa0002";
const C: &str = "aaaa0003";

fn three(f: &Fake) {
    let acc = |id: &str, tag: Option<&str>, at: Option<&str>| {
        let at = at.map(|a| format!("\"{a}\"")).unwrap_or("null".into());
        account_json(id, Some("x@example.com"), tag, "ready")
            .replace("\"last_login_at\":null", &format!("\"last_login_at\":{at}"))
    };
    f.seed_accounts(&accounts_file(&[
        acc(A, Some("bravo#1"), Some("2026-10-01T10:00:00Z")),
        acc(B, Some("Alpha#2"), None),
        acc(C, None, Some("2026-10-05T10:00:00Z")),
    ]));
}

fn ids(core: &noxdeck_lib::Core) -> Vec<String> {
    core.state().accounts.into_iter().map(|a| a.id).collect()
}

#[test]
fn both_sort_orders_put_absent_values_last() {
    let f = Fake::new();
    three(&f);
    let core = f.core();
    assert_eq!(ids(&core), vec![C, A, B], "last login, newest first, never last");
    core.set_sort(Sort::Battletag);
    assert_eq!(ids(&core), vec![B, A, C], "BattleTag A to Z ignoring case, absent last");
    assert_eq!(core.state().sort, Sort::Battletag);
}

#[test]
fn delete_removes_only_the_record_and_touches_no_launcher() {
    let f = Fake::new();
    three(&f);
    let config = f.config_bytes();
    let core = f.core();
    core.delete_account(A).unwrap();
    assert_eq!(ids(&core), vec![C, B]);
    assert_eq!(core.delete_account(A), Err(Refusal::UnknownAccount));
    assert!(f.procs.calls().is_empty());
    assert_eq!(f.config_bytes(), config);
    assert!(!f.backup_file().exists());
}

#[test]
fn an_observed_login_of_an_unlisted_account_creates_nothing() {
    let f = Fake::new();
    three(&f);
    f.procs.set_running(true);
    let run = f.sim.new_run();
    let core = f.core();
    run.login("bbbb0009");
    wait_for(&core, "the launcher to be seen logged in", |s| s.launcher == LauncherKind::LoggedIn);
    let s = core.state();
    assert_eq!(s.accounts.len(), 3);
    assert!(s.accounts.iter().all(|a| !a.is_current));
}

#[test]
fn an_observed_login_marks_the_record_and_a_logout_wants_the_password() {
    let f = Fake::new();
    three(&f);
    f.sim.cache_row(A, "Renamed#9");
    f.procs.set_running(true);
    let run = f.sim.new_run();
    let core = f.core();

    run.login(A);
    wait_for(&core, "the mark", |s| account(s, A).is_current);
    let s = core.state();
    assert_eq!(account(&s, A).battletag.as_deref(), Some("Renamed#9"), "a changed tag is picked up");
    let stamped = account(&s, A).last_login_at.clone().unwrap();
    assert!(stamped.as_str() > "2026-10-05T10:00:00Z", "stamped with the observation time");
    assert_eq!(s.accounts.iter().filter(|a| a.is_current).count(), 1);

    run.logout();
    wait_for(&core, "the password to be wanted", |s| account(s, A).login_state == LoginState::PasswordWanted);
    assert!(!account(&core.state(), A).is_current);

    // The launcher closes: the mark goes.
    run.login(B);
    wait_for(&core, "B open", |s| account(s, B).is_current);
    f.procs.set_running(false);
    wait_for(&core, "no mark", |s| s.accounts.iter().all(|a| !a.is_current));
    assert_eq!(core.state().launcher, LauncherKind::Closed);
}

#[test]
fn a_logout_made_while_the_program_was_closed_is_noticed_at_start() {
    let f = Fake::new();
    three(&f);
    let run = f.sim.new_run();
    run.login(A);
    run.logout();
    let core = f.core();
    assert_eq!(account(&core.state(), A).login_state, LoginState::PasswordWanted);
}

#[test]
fn the_state_survives_a_restart() {
    let f = Fake::new();
    three(&f);
    let core = f.core();
    core.set_sort(Sort::Battletag);
    core.dismiss("rememberLogin");
    core.dismiss("logoutWarning");
    core.delete_account(B).unwrap();
    let before = core.state();
    drop(core);

    let after = f.core().state();
    assert_eq!(after.accounts, before.accounts);
    assert_eq!(after.sort, Sort::Battletag);
    assert!(after.dismissed.remember_login && after.dismissed.logout_warning);
    assert_eq!(after.accounts.len(), 2);
}

#[test]
fn a_broken_data_file_is_reset_once_with_a_notice() {
    let f = Fake::new();
    std::fs::write(f.paths.data.join("accounts.json"), "garbage").unwrap();
    let core = f.core();
    assert_eq!(core.state().notice, Some("data_file_reset"));
    assert!(f.paths.data.join("accounts.json.broken").exists());
    core.acknowledge_notice();
    assert_eq!(core.state().notice, None);
}

#[test]
fn a_missing_launcher_is_a_state_and_the_list_stays_readable() {
    let f = Fake::new();
    three(&f);
    std::fs::remove_file(f.install.join("Battle.net Launcher.exe")).unwrap();
    let core = f.core();
    let s = core.state();
    assert_eq!(s.launcher, LauncherKind::NotFound);
    assert_eq!(s.accounts.len(), 3);
}

#[test]
fn one_tick_requests_one_process_snapshot_for_both_flags() {
    let f = Fake::new();
    three(&f);
    let core = f.core();
    f.procs.set_running(true);
    f.procs.set_game(true);
    f.procs.clear_queries();
    core.tick();
    assert_eq!(f.procs.queries(), vec!["snapshot"]);
    assert_eq!(core.state().launcher, LauncherKind::LoginScreen);
    assert!(core.state().game_running);

    std::fs::remove_file(f.install.join("Battle.net.exe")).unwrap();
    f.procs.clear_queries();
    core.tick();
    assert_eq!(f.procs.queries(), vec!["snapshot"]);
    assert_eq!(core.state().launcher, LauncherKind::NotFound);
    assert!(core.state().game_running);
}

#[test]
fn a_restarted_launcher_forgets_the_old_session_until_its_own_login() {
    let f = Fake::new();
    three(&f);
    let old = f.sim.new_run();
    old.login(A);
    f.procs.set_running(true);
    let core = f.core();
    assert!(account(&core.state(), A).is_current);

    f.procs.set_running(false);
    // Drain a final login written by the old process, then discard its session.
    old.login(A);
    core.tick();
    assert_eq!(core.state().launcher, LauncherKind::Closed);
    assert!(core.state().accounts.iter().all(|a| !a.is_current));

    f.procs.set_running(true);
    core.tick(); // The new process has not created its log yet.
    assert_eq!(core.state().launcher, LauncherKind::LoginScreen);
    assert!(core.state().accounts.iter().all(|a| !a.is_current));
    assert_eq!(core.save_current(), Err(Refusal::NoSavedLogin));

    let new = f.sim.new_run();
    core.tick();
    assert_eq!(core.state().launcher, LauncherKind::LoginScreen);
    new.login(B);
    core.tick();
    assert!(account(&core.state(), B).is_current);
    assert!(!account(&core.state(), A).is_current);
}
