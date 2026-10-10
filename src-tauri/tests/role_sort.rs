mod common;
use common::*;
use noxdeck_lib::{settings::{Sort, Tab}, tray};

fn seeded() -> Fake {
    let f = Fake::new();
    let values = [("champion", Some(1), "01"), ("bronze", Some(5), "02"), ("bronze", Some(1), "03"), ("gold", Some(2), "04"), ("gold", Some(2), "05"), ("none", None, "07"), ("placement", None, "06")];
    let accounts: Vec<String> = values.iter().enumerate().map(|(i, (rank, division, day))| {
        let mut a: serde_json::Value = serde_json::from_str(&account_json(&format!("aaaa{:04}", i + 1), Some("placeholder@example.com"), None, "ready")).unwrap();
        a["last_login_at"] = format!("2026-10-{day}T00:00:00Z").into();
        let rating = if *rank == "none" { serde_json::Value::Null } else { serde_json::json!({ "rank": rank, "division": division, "source": "manual" }) };
        a["ratings"] = serde_json::json!({ "tank": rating, "damage": rating, "support": rating });
        a.to_string()
    }).collect();
    f.seed_accounts(&accounts_file(&accounts));
    f
}
fn ids(core: &noxdeck_lib::Core) -> Vec<String> { core.state().visible_account_ids }

#[test]
fn every_role_sorts_rank_division_ties_and_unranked_in_both_directions() {
    let f = seeded();
    let core = f.core();
    core.set_tab(Tab::Overwatch);
    for (descending, ascending) in [(Sort::TankDesc, Sort::TankAsc), (Sort::DamageDesc, Sort::DamageAsc), (Sort::SupportDesc, Sort::SupportAsc)] {
        core.set_sort(descending);
        assert_eq!(ids(&core), ["aaaa0001", "aaaa0005", "aaaa0004", "aaaa0003", "aaaa0002", "aaaa0006", "aaaa0007"]);
        core.set_sort(ascending);
        assert_eq!(ids(&core), ["aaaa0002", "aaaa0003", "aaaa0005", "aaaa0004", "aaaa0001", "aaaa0006", "aaaa0007"]);
    }
}

#[test]
fn one_sort_is_remembered_per_tab_and_roles_are_unavailable_elsewhere() {
    let f = seeded();
    let core = f.core();
    core.set_sort(Sort::Battletag);
    core.set_tab(Tab::Overwatch);
    assert_eq!(core.state().sort, Sort::LastLogin);
    core.set_sort(Sort::SupportAsc);
    core.set_tab(Tab::CallOfDuty);
    assert_eq!(core.state().sort, Sort::LastLogin);
    core.set_sort(Sort::TankDesc);
    assert_eq!(core.state().sort, Sort::LastLogin);
    core.set_sort(Sort::Battletag);
    core.set_tab(Tab::Overwatch);
    assert_eq!(core.state().sort, Sort::SupportAsc);
    let restarted = f.core();
    assert_eq!(restarted.state().sort, Sort::SupportAsc);
    restarted.set_tab(Tab::All);
    assert_eq!(restarted.state().sort, Sort::Battletag);
    restarted.set_tab(Tab::CallOfDuty);
    assert_eq!(restarted.state().sort, Sort::Battletag);
}

#[test]
fn tray_stays_the_five_most_recent_even_in_a_sorted_game_tab() {
    let f = seeded();
    let core = f.core();
    let before = tray::menu(&core.state());
    core.set_tab(Tab::Overwatch);
    core.set_sort(Sort::TankDesc);
    assert_eq!(tray::menu(&core.state()), before);
    assert_eq!(before[1].id, "acc-aaaa0006");
}
