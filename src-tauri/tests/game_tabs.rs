mod common;
use common::*;
use noxdeck_lib::{registry::Game, settings::{Tab, Settings}, Core};

const ID: &str = "aaaa0001";
const OTHER: &str = "bbbb0002";
fn two() -> Fake {
    let f = Fake::new();
    f.seed_accounts(&accounts_file(&[account_json(ID, Some("placeholder@example.com"), None, "ready"), account_json(OTHER, Some("other@example.com"), None, "ready")]));
    f
}

#[test]
fn filtering_and_empty_tabs_keep_all_accounts_available_for_counts_and_tray() {
    let f = two();
    let core = f.core();
    core.set_account_game(ID, Game::CallOfDuty).unwrap();
    core.set_account_game(OTHER, Game::None).unwrap();
    core.set_tab(Tab::All);
    assert_eq!(core.state().visible_account_ids.len(), 2);
    core.set_tab(Tab::CallOfDuty);
    assert_eq!(core.state().visible_account_ids, [ID]);
    assert_eq!(core.state().accounts.len(), 2);
    core.set_account_game(ID, Game::WorldOfWarcraftForever).unwrap();
    assert_eq!(core.state().tab, Tab::CallOfDuty);
    assert!(core.state().visible_account_ids.is_empty());
    core.set_tab(Tab::WorldOfWarcraftForever);
    assert_eq!(core.state().visible_account_ids, [ID]);
    core.set_tab(Tab::Overwatch);
    assert!(core.state().visible_account_ids.is_empty());
}

#[test]
fn a_tab_survives_restart_and_returns_to_all_once_after_a_version_change() {
    let f = two();
    let core = f.core();
    assert_eq!(core.state().tab, Tab::All);
    core.set_tab(Tab::CallOfDuty);
    assert_eq!(f.core().state().tab, Tab::CallOfDuty);
    let updated = Core::new(f.paths.clone(), fast(), f.procs.clone(), Box::new(|_| {}), "next-version", f.shell.clone());
    assert_eq!(updated.state().tab, Tab::All);
    updated.set_tab(Tab::Overwatch);
    assert_eq!(Settings::load(&f.paths.data.join("settings.json")).0.tab, Tab::Overwatch);
}

#[test]
fn save_current_and_add_take_the_pressed_tabs_game_including_none() {
    for tab in [Tab::All, Tab::Overwatch, Tab::WorldOfWarcraft, Tab::WorldOfWarcraftForever, Tab::CallOfDuty] {
        let f = Fake::new();
        f.sim.new_run().login(ID);
        let core = f.core();
        core.set_tab(tab);
        core.save_current().unwrap();
        assert_eq!(account(&core.state(), ID).game, tab.game());

        let f = Fake::new();
        f.sim.set_list("placeholder@example.com");
        let sim = f.sim.clone();
        f.procs.on_start(move || { sim.set_list("placeholder@example.com"); sim.new_run().login(ID); });
        let core = f.core();
        core.set_tab(tab);
        core.add_account().unwrap();
        core.set_tab(Tab::All);
        wait_idle(&core);
        assert_eq!(account(&core.state(), ID).game, tab.game(), "add keeps the original tab even if it changes during login");
    }
}
