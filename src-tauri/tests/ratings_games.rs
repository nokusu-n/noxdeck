mod common;
use common::*;
use common::overfast::{Reply, Stub};
use noxdeck_lib::{Core, registry::{Game, Registry}, ratings::{Role, RatingValue, overfast::Client}, settings::Settings};
use std::time::Duration;
const ID: &str = "aaaa0001";
const TAG: &str = "Placeholder#1111";
const BODY: &str = r#"{"competitive":{"pc":{"tank":{"division":"gold","tier":3}}}}"#;

fn setup(game: Game, auto: bool) -> (Fake, Stub, std::sync::Arc<Core>) {
    let f = Fake::new();
    f.seed_accounts(&accounts_file(&[account_json(ID, Some("placeholder@example.com"), Some(TAG), "ready")]));
    let mut registry = Registry::load(&f.paths.data.join("accounts.json")).0;
    registry.migrate(true); registry.accounts[0].game = game;
    registry.save(&f.paths.data.join("accounts.json")).unwrap();
    Settings { auto_ratings: auto, ..Settings::default() }.save(&f.paths.data.join("settings.json")).unwrap();
    let stub = Stub::new(); stub.set(TAG, Reply::json(BODY));
    let core = Core::new_with_ratings(f.paths.clone(), fast(), f.procs.clone(), Box::new(|_| {}), "test", f.shell.clone(), Client::new(&stub.url));
    (f, stub, core)
}

#[test]
fn all_other_games_are_excluded_from_manual_and_automatic_requests() {
    for game in [Game::None, Game::WorldOfWarcraft, Game::WorldOfWarcraftForever, Game::CallOfDuty] {
        let (_f, stub, core) = setup(game, true);
        core.sync_ratings();
        wait_for(&core, "refresh finished", |s| !s.syncing);
        assert_eq!(stub.count(), 0);
        core.set_account_game(ID, Game::Overwatch).unwrap();
        wait_for(&core, "new Overwatch refreshed", |s| stub.count() == 1 && !s.syncing);
        assert_eq!(account(&core.state(), ID).ratings.tank.as_ref().unwrap().value.rank, "gold");
    }
}

#[test]
fn changing_back_with_auto_off_keeps_ranks_and_waits_for_manual_refresh() {
    let (_f, stub, core) = setup(Game::Overwatch, false);
    core.set_rating(ID, Role::Tank, Some(RatingValue { rank: "silver".into(), division: Some(1) })).unwrap();
    let ranks = account(&core.state(), ID).ratings.clone();
    core.set_account_game(ID, Game::None).unwrap();
    core.set_account_game(ID, Game::Overwatch).unwrap();
    std::thread::sleep(Duration::from_millis(60));
    assert_eq!(stub.count(), 0);
    assert_eq!(account(&core.state(), ID).ratings, ranks);
    core.sync_ratings();
    wait_for(&core, "manual refreshed", |s| stub.count() == 1 && !s.syncing);
    assert_eq!(account(&core.state(), ID).ratings.tank.as_ref().unwrap().value.rank, "gold");
}

#[test]
fn changing_game_during_a_request_discards_its_answer_and_preserves_old_ranks() {
    let (_f, stub, core) = setup(Game::Overwatch, false);
    core.set_rating(ID, Role::Tank, Some(RatingValue { rank: "silver".into(), division: Some(1) })).unwrap();
    let ranks = account(&core.state(), ID).ratings.clone();
    stub.set(TAG, Reply { delay: Duration::from_millis(150), ..Reply::json(BODY) });
    core.sync_ratings();
    wait_for(&core, "request began", |_| stub.count() == 1);
    core.set_account_game(ID, Game::CallOfDuty).unwrap();
    wait_for(&core, "request finished", |s| !s.syncing);
    assert_eq!(account(&core.state(), ID).ratings, ranks);
}
