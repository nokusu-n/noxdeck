mod common;
use common::*;
use common::overfast::{Reply, Stub};
use noxdeck_lib::{ratings::{overfast::Client, RatingValue, Role}, settings::Settings, Core};
use std::{sync::Arc, time::{Duration, Instant}, thread};

const A: &str = "aaaa0001";
const B: &str = "aaaa0002";
const TAG: &str = "ExamplePlayer#12345";
const BODY: &str = r#"{"competitive":{"pc":{"tank":{"division":"gold","tier":3}}}}"#;
fn setup(show: bool, auto: bool) -> (Fake, Stub) {
    let f = Fake::new();
    f.seed_accounts(&accounts_file(&[account_json(A, Some("placeholder@example.com"), Some(TAG), "ready"), account_json(B, None, None, "ready")]));
    Settings { show_ratings: show, auto_ratings: auto, ..Settings::default() }.save(&f.paths.data.join("settings.json")).unwrap();
    let stub = Stub::new(); stub.set(TAG, Reply::json(BODY)); (f, stub)
}
fn core(f: &Fake, stub: &Stub, interval: Duration) -> Arc<Core> {
    let mut timing = fast(); timing.ratings_interval = interval;
    let states = f.states.clone();
    Core::new_with_ratings(f.paths.clone(), timing, f.procs.clone(), Box::new(move |s| states.lock().unwrap().push(s.clone())), "test", f.shell.clone(), Client::with_timeout(&stub.url, Duration::from_secs(1)))
}
fn until(cond: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !cond() { assert!(Instant::now() < deadline, "refresh did not finish"); thread::sleep(Duration::from_millis(5)); }
}
fn idle(core: &Core, stub: &Stub, count: usize) { until(|| stub.count() >= count && !core.state().syncing); }

#[test]
fn both_settings_gate_startup_manual_and_timer_requests() {
    assert!(Settings::default().auto_ratings);
    for (show, auto) in [(false,false),(false,true),(true,false)] {
        let (f, stub) = setup(show, auto); let core = core(&f,&stub,Duration::from_millis(20));
        core.sync_ratings(); thread::sleep(Duration::from_millis(100));
        assert_eq!(stub.count(), 0); assert!(!core.state().syncing);
    }
}

#[test]
fn startup_and_manual_triggers_skip_missing_tags_and_publish_each_answer() {
    let (f,stub) = setup(true,true); let core = core(&f,&stub,Duration::from_secs(1800));
    idle(&core,&stub,1);
    assert!(core.state().accounts[0].ratings.tank.is_some());
    assert!(core.state().accounts[1].ratings.tank.is_none());
    assert!(f.states.lock().unwrap().iter().any(|s| s.syncing));
    core.sync_ratings(); idle(&core,&stub,2);
    assert_eq!(stub.count(),2);
}

#[test]
fn enabling_either_setting_triggers_and_persists_without_clearing_ratings() {
    let (f,stub) = setup(true,false); let core = core(&f,&stub,Duration::from_secs(1800));
    core.set_auto_ratings(true); idle(&core,&stub,1);
    let before = core.state().accounts;
    core.set_show_ratings(false); core.sync_ratings();
    assert_eq!(core.state().accounts,before);
    core.set_show_ratings(true); idle(&core,&stub,2);
    core.set_auto_ratings(false);
    assert_eq!(core.state().accounts,before);
    let stored = Settings::load(&f.paths.data.join("settings.json")).0;
    assert!(!stored.auto_ratings); assert!(stored.show_ratings);
    thread::sleep(Duration::from_millis(30)); assert_eq!(stub.count(),2);
}

#[test]
fn timer_runs_without_a_window_and_a_sleep_jump_is_due_once() {
    let (f,stub) = setup(true,true); let core = core(&f,&stub,Duration::from_millis(80));
    idle(&core,&stub,1); until(|| stub.count() >= 2);
    core.set_auto_ratings(false);
    use std::time::SystemTime;
    let start = SystemTime::now(); let interval = Duration::from_secs(1800);
    assert!(noxdeck_lib::ratings::due(Some(start), start + interval * 4, interval));
    assert!(!noxdeck_lib::ratings::due(Some(start + interval * 4), start + interval * 4, interval));
}

#[test]
fn an_inflight_refresh_is_single_and_a_deleted_account_is_not_recreated() {
    let (f,stub) = setup(true,false);
    stub.set(TAG, Reply { delay:Duration::from_millis(60), ..Reply::json(BODY) });
    let core = core(&f,&stub,Duration::from_secs(1800));
    core.set_auto_ratings(true); until(|| stub.count()==1);
    assert!(core.state().syncing);
    for _ in 0..20 { core.sync_ratings(); }
    core.delete_account(A).unwrap(); idle(&core,&stub,1);
    assert_eq!(stub.count(),1); assert!(core.state().accounts.iter().all(|a| a.id!=A));
}

#[test]
fn rate_limit_stops_the_remaining_accounts_and_failures_preserve_values() {
    for status in [429,503,404,500] {
        let (f,stub) = setup(true,false);
        f.seed_accounts(&accounts_file(&[account_json(A,None,Some(TAG),"ready"), account_json(B,None,Some("SecondExample#23456"),"ready")]));
        stub.set(TAG, Reply { status, ..Reply::json(BODY) });
        let core = core(&f,&stub,Duration::from_secs(1800));
        core.set_rating(A,Role::Tank,Some(RatingValue{rank:"gold".into(),division:Some(2)})).unwrap();
        let before = core.state().accounts;
        core.set_auto_ratings(true); idle(&core,&stub,1);
        assert_eq!(stub.count(), if [429,503].contains(&status) {1} else {2});
        assert_eq!(core.state().accounts,before);
    }
}

#[test]
fn switching_off_drops_inflight_answers_and_stops_remaining_requests() {
    for hide in [false,true] {
        let (f,stub) = setup(true,false);
        stub.set(TAG, Reply { delay:Duration::from_millis(60), ..Reply::json(BODY) });
        let core = core(&f,&stub,Duration::from_secs(1800));
        core.set_auto_ratings(true); until(|| stub.count()==1);
        if hide { core.set_show_ratings(false); } else { core.set_auto_ratings(false); }
        let before = core.state().accounts; idle(&core,&stub,1);
        assert_eq!(core.state().accounts,before);
    }
}

#[test]
fn slow_or_unanswered_requests_leave_the_core_usable_and_do_not_delay_drop() {
    let (f,stub) = setup(true,false);
    stub.set(TAG,Reply{delay:Duration::from_millis(500),..Reply::json(BODY)});
    let core = core(&f,&stub,Duration::from_secs(1800));
    core.set_auto_ratings(true); until(||stub.count()==1);
    core.set_rating(A,Role::Tank,Some(RatingValue{rank:"silver".into(),division:Some(1)})).unwrap();
    let weak = Arc::downgrade(&core); let start = Instant::now(); drop(core);
    until(|| weak.upgrade().is_none());
    assert!(start.elapsed()<Duration::from_millis(80));
}

const NEW_ID: &str = "aaaa0003";
const NEW_TAG: &str = "NewExample#34567";
fn create_account(core: &Arc<Core>, f: &Fake, save: bool, tag: Option<&str>) {
    if let Some(tag) = tag { f.sim.cache_row(NEW_ID, tag); }
    if save {
        f.sim.set_list("new-placeholder@example.com");
        f.sim.new_run().login(NEW_ID);
        f.procs.set_running(true);
        assert_eq!(core.save_current().unwrap(), NEW_ID);
    } else {
        let sim = f.sim.clone();
        f.procs.on_start(move || {
            sim.set_list("new-placeholder@example.com");
            sim.new_run().login(NEW_ID);
        });
        core.add_account().unwrap(); wait_idle(core);
    }
}

fn immediate_refresh(save: bool, busy: bool) {
    let (f, stub) = setup(true, true);
    if busy { stub.set(TAG, Reply { delay: Duration::from_millis(400), ..Reply::json(BODY) }); }
    stub.set(NEW_TAG, Reply::json(BODY));
    let core = core(&f, &stub, Duration::from_secs(1800));
    if busy { until(|| stub.count() == 1); } else { idle(&core, &stub, 1); }
    create_account(&core, &f, save, Some(NEW_TAG));
    idle(&core, &stub, 2);
    assert_eq!(account(&core.state(), NEW_ID).ratings.tank.as_ref().unwrap().value.rank, "gold");
    thread::sleep(Duration::from_millis(40));
    let requests = stub.requests.lock().unwrap();
    assert_eq!(requests.len(), 2, "refresh only the new account, exactly once");
    assert!(requests[0].starts_with("GET /players/ExamplePlayer-12345/summary "));
    assert!(requests[1].starts_with("GET /players/NewExample-34567/summary "));
}

#[test]
fn added_account_refreshes_immediately_without_refetching_existing_accounts() { immediate_refresh(false, false); }
#[test]
fn saved_account_refreshes_immediately_without_refetching_existing_accounts() { immediate_refresh(true, false); }
#[test]
fn added_account_is_picked_up_once_after_the_running_request() { immediate_refresh(false, true); }
#[test]
fn saved_account_is_picked_up_once_after_the_running_request() { immediate_refresh(true, true); }

#[test]
fn new_account_refresh_requires_both_settings_and_a_tag() {
    for save in [false, true] { for (show, auto, tag) in [(false,true,Some(NEW_TAG)),(true,false,Some(NEW_TAG)),(true,true,None)] {
        let (f, stub) = setup(show, auto); stub.set(NEW_TAG, Reply::json(BODY));
        let core = core(&f, &stub, Duration::from_secs(1800));
        if show && auto { idle(&core, &stub, 1); }
        let before = stub.count();
        create_account(&core, &f, save, tag);
        thread::sleep(Duration::from_millis(50));
        assert_eq!(stub.count(), before);
        assert!(account(&core.state(), NEW_ID).ratings.tank.is_none());
    } }
}
