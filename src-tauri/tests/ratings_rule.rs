mod common;
use common::*;
use noxdeck_lib::ratings::{RatingValue, Role, Source};

const ID: &str = "aaaa0001";
fn value(rank: &str, division: u8) -> RatingValue { RatingValue { rank: rank.into(), division: Some(division) } }
fn seeded() -> Fake {
    let f = Fake::new();
    f.seed_accounts(&accounts_file(&[account_json(ID, None, None, "ready")]));
    f
}

#[test]
fn a_service_rank_replaces_manual_and_service_values_in_either_direction() {
    let f = seeded(); let core = f.core();
    core.set_rating(ID, Role::Tank, Some(value("gold", 2))).unwrap();
    for next in [value("gold", 3), value("silver", 5), value("champion", 1)] {
        core.apply_rating_report(ID, [Some(next.clone()), None, None]);
        let actual = core.state().accounts[0].ratings.tank.clone().unwrap();
        assert_eq!(actual.value, next); assert_eq!(actual.source, Source::Service);
    }
    assert_eq!(core.state().accounts, f.core().state().accounts);
}

#[test]
fn no_report_unknown_rank_and_failure_preserve_manual_values() {
    let f = seeded(); let core = f.core();
    core.set_rating(ID, Role::Tank, Some(value("gold", 2))).unwrap();
    let before = core.state().accounts;
    for report in [[None, None, None], [Some(value("future", 3)), None, None], [None, None, None]] {
        core.apply_rating_report(ID, report);
        assert_eq!(core.state().accounts, before);
    }
    core.apply_rating_report(ID, [None, Some(value("silver", 1)), None]);
    assert_eq!(core.state().accounts[0].ratings.tank, before[0].ratings.tank);
    assert_eq!(core.state().accounts[0].ratings.damage.as_ref().unwrap().source, Source::Service);
}

#[test]
fn manual_and_service_writes_in_either_order_leave_one_whole_value() {
    let f = seeded(); let core = f.core();
    for _ in 0..10 {
        core.apply_rating_report(ID, [Some(value("silver", 5)), None, None]);
        core.set_rating(ID, Role::Tank, Some(value("gold", 2))).unwrap();
        let rating = core.state().accounts[0].ratings.tank.clone().unwrap();
        assert_eq!(rating.value, value("gold", 2)); assert_eq!(rating.source, Source::Manual);
        core.apply_rating_report(ID, [Some(value("silver", 5)), None, None]);
        let rating = core.state().accounts[0].ratings.tank.clone().unwrap();
        assert_eq!(rating.value, value("silver", 5)); assert_eq!(rating.source, Source::Service);
    }
    core.set_rating(ID, Role::Tank, None).unwrap();
    assert!(core.state().accounts[0].ratings.tank.is_none());
}
