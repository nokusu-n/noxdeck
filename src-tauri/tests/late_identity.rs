mod common;
use common::*;

#[test]
fn stopped_launcher_does_not_read_cached_identity_until_it_runs_again() {
    let f = Fake::new();
    f.seed_accounts(&accounts_file(&[account_json("aaaa0001", None, None, "ready")]));
    f.sim.set_list("placeholder@example.com");
    f.sim.cache_row("aaaa0001", "ExamplePlayer#12345");
    let run = f.sim.new_run();
    run.login("aaaa0001");
    let core = f.core();
    for _ in 0..3 {
        core.tick();
        let state = core.state();
        assert!(account(&state, "aaaa0001").email.is_none());
        assert!(account(&state, "aaaa0001").battletag.is_none(), "a stopped launcher's cache is not used, even after a historic login");
    }
    f.procs.set_running(true);
    run.login("aaaa0001");
    core.tick();
    assert_eq!(account(&core.state(), "aaaa0001").email.as_deref(), Some("placeholder@example.com"));
    assert_eq!(account(&core.state(), "aaaa0001").battletag.as_deref(), Some("ExamplePlayer#12345"));
}

#[test]
fn expired_identity_waits_are_repaired_by_a_later_tick_and_persisted() {
    let f = Fake::new();
    let sim = f.sim.clone();
    f.procs.on_start(move || sim.new_run().login("aaaa0001"));
    let core = f.core();
    core.add_account().unwrap();
    wait_idle(&core);
    assert!(core.state().accounts[0].email.is_none());
    assert!(core.state().accounts[0].battletag.is_none());
    f.sim.set_list("placeholder@example.com");
    f.sim.cache_row("aaaa0001", "ExamplePlayer#12345");
    core.tick();
    assert_eq!(core.state().accounts[0].email.as_deref(), Some("placeholder@example.com"));
    assert_eq!(core.state().accounts[0].battletag.as_deref(), Some("ExamplePlayer#12345"));
    assert_eq!(f.core().state().accounts, core.state().accounts);
}

#[test]
fn a_later_login_repairs_only_its_missing_email_and_keeps_known_values() {
    let f = Fake::new();
    f.seed_accounts(&accounts_file(&[
        account_json("aaaa0001", None, None, "ready"),
        account_json("aaaa0002", Some("known@example.com"), Some("Known#12345"), "ready"),
        account_json("aaaa0003", None, None, "ready"),
    ]));
    let core = f.core();
    f.sim.set_list("placeholder@example.com");
    core.tick();
    assert!(account(&core.state(), "aaaa0001").email.is_none(), "no active login must not claim the config e-mail");
    f.procs.set_running(true);
    let run = f.sim.new_run();
    run.login("aaaa0001");
    core.tick();
    assert_eq!(account(&core.state(), "aaaa0001").email.as_deref(), Some("placeholder@example.com"));
    assert!(account(&core.state(), "aaaa0003").email.is_none());
    f.sim.set_list("other@example.com");
    core.tick();
    assert_eq!(account(&core.state(), "aaaa0001").email.as_deref(), Some("placeholder@example.com"));
    assert_eq!(account(&core.state(), "aaaa0002").email.as_deref(), Some("known@example.com"));
    f.sim.cache_row("aaaa0003", "LatePlayer#12345");
    core.tick();
    assert_eq!(account(&core.state(), "aaaa0003").battletag.as_deref(), Some("LatePlayer#12345"));
}
