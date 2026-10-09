mod common;
use common::*;
use noxdeck_lib::{registry::Registry, ratings::{RatingValue, Role}, Refusal};
use serde_json::json;

const ID: &str = "aaaa0001";
fn seeded() -> Fake {
    let f = Fake::new();
    f.seed_accounts(&accounts_file(&[account_json(ID, None, None, "ready")]));
    f
}
fn value(rank: &str, division: Option<u8>) -> RatingValue {
    RatingValue { rank: rank.into(), division }
}

#[test]
fn old_accounts_load_without_ratings_and_manual_values_round_trip() {
    let f = seeded();
    let core = f.core();
    let before = serde_json::to_value(core.state()).unwrap();
    assert_eq!(before["accounts"][0]["ratings"], json!({"tank":null,"damage":null,"support":null}));
    core.set_rating(ID, Role::Tank, Some(value("gold", Some(3)))).unwrap();
    core.set_rating(ID, Role::Damage, Some(value("placement", None))).unwrap();
    core.set_rating(ID, Role::Support, Some(value("champion", Some(1)))).unwrap();
    let (stored, reset) = Registry::load(&f.paths.data.join("accounts.json"));
    assert!(!reset);
    assert_eq!(stored.schema, 1);
    let ratings = serde_json::to_value(&stored.accounts[0]).unwrap()["ratings"].clone();
    assert_eq!(ratings["tank"], json!({"rank":"gold","division":3,"source":"manual"}));
    assert_eq!(ratings["damage"], json!({"rank":"placement","source":"manual"}));
    assert_eq!(core.state().accounts, f.core().state().accounts);
    core.set_rating(ID, Role::Tank, None).unwrap();
    assert!(serde_json::to_value(core.state()).unwrap()["accounts"][0]["ratings"]["tank"].is_null());
    assert_eq!(core.state().accounts, f.core().state().accounts);
    assert!(f.procs.calls().is_empty());
}

#[test]
fn every_rank_and_division_is_accepted_and_invalid_edits_change_nothing() {
    let f = seeded();
    let core = f.core();
    for rank in ["bronze","silver","gold","platinum","emerald","diamond","master","grandmaster","champion"] {
        for division in 1..=5 {
            core.set_rating(ID, Role::Tank, Some(value(rank, Some(division)))).unwrap();
        }
    }
    let before = core.state().accounts;
    for invalid in [value("future", Some(3)), value("gold", None), value("gold", Some(0)), value("gold", Some(6)), value("placement", Some(1))] {
        assert_eq!(core.set_rating(ID, Role::Tank, Some(invalid)), Err(Refusal::InvalidRating));
        assert_eq!(core.state().accounts, before);
    }
    assert_eq!(core.set_rating("bbbb0002", Role::Tank, None), Err(Refusal::UnknownAccount));
    assert!(serde_json::from_value::<Role>(json!("other")).is_err());
}

#[test]
fn deleting_an_account_also_deletes_its_ratings() {
    let f = seeded();
    let core = f.core();
    core.set_rating(ID, Role::Tank, Some(value("gold", Some(3)))).unwrap();
    core.delete_account(ID).unwrap();
    assert!(f.core().state().accounts.is_empty());
    assert_eq!(Registry::load(&f.paths.data.join("accounts.json")).0.accounts.len(), 0);
}
