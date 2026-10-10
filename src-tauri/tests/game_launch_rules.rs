mod common;
use common::*;
use noxdeck_lib::{registry::Game, tray, Refusal};

const ID: &str = "aaaa0001";
fn seeded() -> Fake {
    let f = Fake::new();
    f.seed_accounts(&accounts_file(&[account_json(ID, Some("placeholder@example.com"), Some("Placeholder#1111"), "ready")]));
    f
}

#[test]
fn only_overwatch_can_start_a_game_and_other_choices_leave_processes_and_config_untouched() {
    for game in [Game::WorldOfWarcraft, Game::WorldOfWarcraftForever, Game::CallOfDuty, Game::None] {
        let f = seeded();
        let core = f.core();
        core.set_account_game(ID, game).unwrap();
        let before = std::fs::read(f.paths.config_file()).unwrap();
        assert_eq!(core.start_game(ID, true), Err(Refusal::GameUnsupported));
        assert!(f.procs.calls().is_empty());
        assert_eq!(std::fs::read(f.paths.config_file()).unwrap(), before);
        assert!(core.state().operation.is_none());
    }
}

#[test]
fn the_tray_routes_each_game_to_its_allowed_start() {
    for game in [Game::Overwatch, Game::WorldOfWarcraft, Game::WorldOfWarcraftForever, Game::CallOfDuty, Game::None] {
        let f = seeded();
        let sim = f.sim.clone();
        f.procs.on_start(move || { let run = sim.new_run(); run.login(ID); run.line("InstallState (prometheus): installed=1 playable=1"); });
        let procs = f.procs.clone();
        f.procs.on_launch(move || procs.set_game(true));
        let core = f.core();
        core.set_account_game(ID, game).unwrap();
        tray::click(&core, &format!("acc-{ID}"));
        wait_idle(&core);
        assert!(f.procs.calls().iter().any(|call| call == "start_launcher"));
        assert_eq!(f.procs.calls().iter().any(|call| call == "send_launch_command"), game == Game::Overwatch);
    }
}

#[test]
fn manual_game_on_another_game_choice_is_tracked_and_blocks_changing_the_choice() {
    let f = seeded();
    let core = f.core();
    core.set_account_game(ID, Game::CallOfDuty).unwrap();
    let run = f.sim.new_run();
    run.login(ID);
    run.line("Launched <game path> -uid prometheus");
    f.procs.set_running(true);
    f.procs.set_game(true);
    core.tick();
    assert!(account(&core.state(), ID).is_in_game);
    assert_eq!(core.set_account_game(ID, Game::None), Err(Refusal::GameChangeBlocked));
    assert_eq!(core.start_game(ID, true), Err(Refusal::GameUnsupported));
    assert!(!tray::menu(&core.state()).iter().find(|item| item.id == format!("acc-{ID}")).unwrap().enabled);
}
