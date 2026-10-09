//! "Add account": flows against the fake launcher directory.

mod common;

use common::*;
use noxdeck_lib::registry::LoginState;
use noxdeck_lib::{Core, OpKind, Phase, Refusal};
use std::fs;
use std::sync::{Arc, Mutex};

const A: &str = "aaaa0001";
const C: &str = "aaaa0003";
const ORIGINAL_LIST: &str = "user1@example.com,user2@example.com,user3@example.com,user4@example.com,user5@example.com";

/// The launcher starts on its login screen and waits; the run is kept for the test.
fn login_screen(f: &Fake) -> Arc<Mutex<Option<Run>>> {
    let kept = Arc::new(Mutex::new(None));
    let (sim, slot) = (f.sim.clone(), kept.clone());
    f.procs.on_start(move || {
        let run = sim.new_run();
        run.browser("LoginCredential");
        *slot.lock().unwrap() = Some(run);
    });
    kept
}

fn wait_waiting(core: &Core) {
    wait_for(core, "the waiting state", |s| {
        s.operation.as_ref().is_some_and(|o| o.phase == Phase::WaitingLogin)
    });
}

/// The user logs into account `id` in the launcher.
fn user_logs_in(f: &Fake, kept: &Mutex<Option<Run>>, id: &str, email: &str, tag: Option<&str>) {
    f.sim.set_list(email);
    if let Some(tag) = tag {
        f.sim.cache_row(id, tag);
    }
    kept.lock().unwrap().as_ref().unwrap().login(id);
}

#[test]
fn a_new_account_is_added_with_tag_and_email() {
    let f = Fake::new();
    let kept = login_screen(&f);
    let core = f.core();
    assert!(core.state().accounts.is_empty());

    core.add_account().unwrap();
    wait_waiting(&core);
    let op = core.state().operation.unwrap();
    assert_eq!((op.kind, op.account_id, op.cancellable), (OpKind::Add, None, true));
    assert_eq!(f.sim.list().as_deref(), Some(""), "the launcher was started with nobody saved");
    assert!(core.state().accounts.is_empty());

    user_logs_in(&f, &kept, C, "userC@example.com", Some("Charlie#3333"));
    wait_idle(&core);
    let s = core.state();
    assert_eq!(s.accounts.len(), 1);
    let a = account(&s, C);
    assert_eq!(a.email.as_deref(), Some("userC@example.com"));
    assert_eq!(a.battletag.as_deref(), Some("Charlie#3333"));
    assert_eq!(a.login_state, LoginState::Ready);
    assert!(a.is_current && a.last_login_at.is_some());
    // It is stored: a new core reads it back.
    assert_eq!(f.core().state().accounts.len(), 1);
}

#[test]
fn an_account_already_listed_is_not_duplicated() {
    let f = Fake::new();
    f.seed_accounts(&accounts_file(&[account_json(A, Some("old@example.com"), Some("Old#1"), "password_wanted")]));
    let kept = login_screen(&f);
    let core = f.core();
    core.add_account().unwrap();
    wait_waiting(&core);
    user_logs_in(&f, &kept, A, "userA@example.com", Some("Alpha#1111"));
    wait_idle(&core);
    let s = core.state();
    assert_eq!(s.accounts.len(), 1);
    let a = account(&s, A);
    assert_eq!(a.login_state, LoginState::Ready);
    assert!(a.is_current);
    assert_eq!(a.email.as_deref(), Some("userA@example.com"), "a changed e-mail is picked up");
    assert_eq!(a.battletag.as_deref(), Some("Alpha#1111"));
}

#[test]
fn closing_the_launcher_unused_restores_the_saved_list_after_it_is_gone() {
    let f = Fake::new();
    login_screen(&f);
    let core = f.core();
    core.add_account().unwrap();
    wait_waiting(&core);
    assert_eq!(f.sim.list().as_deref(), Some(""));

    // The user closes the launcher; on a graceful exit it rewrites the config.
    f.sim.set_list("rewritten@example.com");
    f.procs.set_running(false);
    wait_idle(&core);
    assert_eq!(f.sim.list().as_deref(), Some(ORIGINAL_LIST));
    assert!(core.state().accounts.is_empty());
}

#[test]
fn cancel_stops_the_launcher_and_restores_the_list() {
    let f = Fake::new();
    login_screen(&f);
    let core = f.core();
    core.add_account().unwrap();
    wait_waiting(&core);
    core.cancel_operation();
    wait_idle(&core);
    assert_eq!(f.sim.list().as_deref(), Some(ORIGINAL_LIST));
    assert_eq!(f.procs.calls(), vec!["start_launcher", "stop_launcher"]);
    assert!(core.state().accounts.is_empty());
}

#[test]
fn tag_or_email_not_found_in_time_is_stored_as_absent() {
    let f = Fake::new();
    let kept = login_screen(&f);
    let core = f.core();
    core.add_account().unwrap();
    wait_waiting(&core);
    kept.lock().unwrap().as_ref().unwrap().login(C); // no list entry, no cache row
    wait_idle(&core);
    let a = account(&core.state(), C).clone();
    assert_eq!((a.email, a.battletag), (None, None));
    assert_eq!(a.login_state, LoginState::Ready);
}

#[test]
fn a_config_without_the_saved_list_still_adds() {
    let f = Fake::new();
    let text = String::from_utf8(f.config_bytes()).unwrap().replace("\"SavedAccountNames\"", "\"SomethingElse\"");
    fs::write(f.paths.config_file(), &text).unwrap();
    let kept = login_screen(&f);
    let core = f.core();
    core.add_account().unwrap();
    wait_waiting(&core);
    kept.lock().unwrap().as_ref().unwrap().login(C);
    wait_idle(&core);
    assert_eq!(core.state().accounts.len(), 1);
    assert_eq!(fs::read_to_string(f.paths.config_file()).unwrap(), text, "nothing was written");
}

#[test]
fn a_running_game_does_not_change_an_add_or_stop_the_game() {
    let f = Fake::new();
    f.sim.new_run().login(A);
    f.procs.set_running(true);
    let kept = login_screen(&f);
    f.procs.set_game(true);
    let core = f.core();
    core.add_account().unwrap();
    wait_waiting(&core);
    assert_eq!(f.procs.calls(), vec!["stop_launcher", "start_launcher"]);
    assert_eq!(f.sim.list().as_deref(), Some(""));
    user_logs_in(&f, &kept, C, "userC@example.com", Some("Charlie#3333"));
    wait_idle(&core);
    assert!(account(&core.state(), C).is_current && core.state().game_running);
    assert_eq!(f.procs.calls(), vec!["stop_launcher", "start_launcher"]);

    core.add_account().unwrap();
    wait_waiting(&core);
    core.cancel_operation();
    wait_idle(&core);
    core.tick();
    assert!(core.state().game_running);
    assert_eq!(f.sim.list().as_deref(), Some("userC@example.com"));
    assert!(f.procs.calls().iter().all(|c| c != "stop_game"));
}

#[test]
fn a_missing_launcher_is_refused() {
    let f = Fake::new();
    let core = f.core();
    fs::remove_file(f.install.join("Battle.net Launcher.exe")).unwrap();
    assert_eq!(core.add_account(), Err(Refusal::LauncherNotFound));
}

#[test]
fn earlier_accounts_still_start_after_adding() {
    let f = Fake::new();
    f.seed_accounts(&accounts_file(&[account_json(A, Some("userA@example.com"), Some("Alpha#1111"), "ready")]));
    f.sim.cache_row(A, "Alpha#1111");
    let kept = login_screen(&f);
    let core = f.core();
    core.add_account().unwrap();
    wait_waiting(&core);
    user_logs_in(&f, &kept, C, "userC@example.com", Some("Charlie#3333"));
    wait_idle(&core);

    // Now the launcher logs in whoever is first in the saved list.
    let sim = f.sim.clone();
    f.procs.on_start(move || {
        let run = sim.new_run();
        match sim.list().as_deref() {
            Some("userA@example.com") => run.login(A),
            _ => run.browser("LoginCredential"),
        }
    });
    core.start_launcher(A).unwrap();
    wait_idle(&core);
    let s = core.state();
    assert!(account(&s, A).is_current && s.accounts.len() == 2);
}
