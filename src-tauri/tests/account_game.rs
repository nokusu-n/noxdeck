mod common;
use common::*;
use noxdeck_lib::{registry::{Game, Registry}, settings::Settings, Refusal};

const ID: &str = "aaaa0001";
fn seeded() -> Fake {
    let f = Fake::new();
    f.seed_accounts(&accounts_file(&[account_json(ID, Some("placeholder@example.com"), Some("Placeholder#1111"), "ready")]));
    f
}

#[test]
fn migration_follows_the_old_game_button_setting_and_runs_once() {
    for shown in [true, false] {
        let f = seeded();
        Settings { show_game_start: shown, ..Settings::default() }.save(&f.paths.data.join("settings.json")).unwrap();
        let core = f.core();
        assert_eq!(account(&core.state(), ID).game, if shown { Game::Overwatch } else { Game::None });
        assert_eq!(Registry::load(&f.paths.data.join("accounts.json")).0.schema, 2);
        core.set_account_game(ID, Game::CallOfDuty).unwrap();
        assert_eq!(account(&f.core().state(), ID).game, Game::CallOfDuty);
    }
}

#[test]
fn every_game_and_none_persist_and_journal_only_the_account_number() {
    let f = seeded();
    let core = f.core();
    for game in [Game::CallOfDuty, Game::WorldOfWarcraft, Game::WorldOfWarcraftForever, Game::None, Game::Overwatch] {
        core.set_account_game(ID, game).unwrap();
        assert_eq!(account(&core.state(), ID).game, game);
        assert_eq!(Registry::load(&f.paths.data.join("accounts.json")).0.find(ID).unwrap().game, game);
    }
    let text = core.journal.text();
    assert_eq!(text.matches("Account game changed: account 1").count(), 5);
    assert!(!text.contains(ID) && !text.contains('@') && !text.contains('#'));
    assert_eq!(core.set_account_game("bbbb0002", Game::None), Err(Refusal::UnknownAccount));
}

#[test]
fn the_core_refuses_changing_the_starting_or_playing_accounts_game() {
    let f = seeded();
    let core = f.core();
    core.start_launcher(ID).unwrap();
    assert_eq!(core.set_account_game(ID, Game::None), Err(Refusal::GameChangeBlocked));
    wait_idle(&core);
    let run = f.sim.new_run();
    run.login(ID);
    run.line("Launched <game path> -uid prometheus");
    f.procs.set_running(true);
    f.procs.set_game(true);
    core.tick();
    assert!(account(&core.state(), ID).is_in_game);
    assert_eq!(core.set_account_game(ID, Game::None), Err(Refusal::GameChangeBlocked));
    assert_eq!(account(&core.state(), ID).game, Game::Overwatch);
}
