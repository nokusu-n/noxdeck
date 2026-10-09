//! The launcher adapter, module by module: log reading against the recorded
//! fixtures, config access, the BattleTag lookup, the data files.

mod common;

use common::*;
use noxdeck_lib::launcher::log::{last_session, LogTail};
use noxdeck_lib::launcher::{cache, config, GameReadiness as R, LauncherEvent as E};
use noxdeck_lib::registry::Registry;
use noxdeck_lib::settings::{Settings, Sort};
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

const STAND: Duration = Duration::from_millis(100);

/// Write a fixture as a main log, with unrelated lines mixed in between its lines.
fn put_log(dir: &Path, name: &str, fixture_name: &str) {
    fs::create_dir_all(dir).unwrap();
    let mut out = String::new();
    for (i, line) in fixture(fixture_name).lines().enumerate() {
        out.push_str(line);
        out.push('\n');
        if i > 0 {
            out.push_str("D 2026-10-09 01:00:00.000000 [Net] {Main} some unrelated line\n");
        }
    }
    fs::write(dir.join(name), out).unwrap();
}

/// Events from a whole fixture: one poll, then one more after the standing time.
fn events_of(fixture_name: &str) -> Vec<E> {
    let dir = tempfile::tempdir().unwrap();
    put_log(dir.path(), "battle.net-20261009T000001.log", fixture_name);
    let mut tail = LogTail::new(STAND);
    let t0 = Instant::now();
    let mut events: Vec<E> = tail
        .poll(dir.path(), t0)
        .events
        .into_iter()
        .map(|t| t.event)
        .collect();
    events.extend(
        tail.poll(dir.path(), t0 + STAND * 3)
            .events
            .into_iter()
            .map(|t| t.event),
    );
    events
        .into_iter()
        .filter(|e| !matches!(e, E::GameReadiness(_)))
        .collect()
}

fn readiness_of(name: &str) -> Vec<R> {
    let dir = tempfile::tempdir().unwrap();
    put_log(dir.path(), "battle.net-20261009T000001.log", name);
    LogTail::new(STAND)
        .poll(dir.path(), Instant::now())
        .events
        .into_iter()
        .filter_map(|t| {
            if let E::GameReadiness(r) = t.event {
                Some(r)
            } else {
                None
            }
        })
        .collect()
}

#[test]
fn readiness_from_each_stage2_fixture() {
    for (name, expected) in [
        ("update-pending-at-start.log", vec![R::NotPlayable]),
        (
            "update-runs-then-playable.log",
            vec![R::NotPlayable, R::Playable],
        ),
        ("playable-no-launch.log", vec![R::Playable]),
        ("launch-command-launcher-stays.log", vec![R::Playable]),
        ("launch-by-hand-launcher-exits.log", vec![R::Playable]),
        ("exec-sender.log", vec![]),
    ] {
        assert_eq!(readiness_of(name), expected, "{name}");
    }
}

#[test]
fn readiness_requires_the_exact_product_and_playable_token() {
    let f = Fake::new();
    let run = f.sim.new_run();
    let mut tail = LogTail::new(STAND);
    run.line("InstallState (prometheus): playable=0");
    let got = tail.poll(&f.paths.logs_dir(), Instant::now());
    assert!(got
        .events
        .iter()
        .any(|t| t.event == E::GameReadiness(R::NotPlayable)));
    assert_eq!(tail.readiness(), R::NotPlayable);
    for line in [
        "InstallState (prometheus): playableProgress=1",
        "InstallState (prometheus): regionalVersions=(eu: {playable:true})",
        "InstallState (prometheus):",
        "InstallState (other): playable=1 installed=1",
        "InstallState (prometheus): notplayable=1",
        "InstallState (prometheus): playable=10",
    ] {
        run.line(line);
    }
    assert!(tail
        .poll(&f.paths.logs_dir(), Instant::now())
        .events
        .is_empty());
    assert_eq!(tail.readiness(), R::NotPlayable);
    run.line("InstallState (prometheus): playable=1");
    tail.poll(&f.paths.logs_dir(), Instant::now());
    assert_eq!(tail.readiness(), R::Playable);
}

#[test]
fn readiness_resets_to_unknown_on_a_new_log() {
    let f = Fake::new();
    let old = f.sim.new_run();
    old.line("InstallState (prometheus): installed=1 playable=1");
    let mut tail = LogTail::new(STAND);
    tail.poll(&f.paths.logs_dir(), Instant::now());
    assert_eq!(tail.readiness(), R::Playable);
    f.sim.new_run();
    assert!(tail.poll(&f.paths.logs_dir(), Instant::now()).new_file);
    assert_eq!(tail.readiness(), R::Unknown);
}

fn logged_in(id: &str) -> E {
    E::LoggedIn { id: id.into() }
}

#[test]
fn saved_login_with_challenge_flicker_is_one_login_and_nothing_else() {
    assert_eq!(
        events_of("auto-login-with-challenge.log"),
        vec![logged_in("aaaa0002")]
    );
}

#[test]
fn empty_list_login_screen_counts_after_it_stood() {
    assert_eq!(
        events_of("empty-list-login-screen.log"),
        vec![E::LoginScreen]
    );
}

#[test]
fn manual_login_with_credential_flicker_then_game_is_one_login_and_launch() {
    assert_eq!(
        events_of("manual-login-then-game.log"),
        vec![logged_in("aaaa0002"), E::GameLaunched]
    );
}

#[test]
fn logout_then_login_screen() {
    assert_eq!(
        events_of("login-then-logout.log"),
        vec![logged_in("aaaa0002"), E::LoggedOut, E::LoginScreen]
    );
}

#[test]
fn start_after_logout_stands_on_the_login_screen() {
    assert_eq!(
        events_of("after-logout-password-wanted.log"),
        vec![E::LoginScreen]
    );
}

#[test]
fn update_pending_is_just_a_login_but_a_launched_game_is_an_event() {
    assert_eq!(
        events_of("logged-in-update-pending.log"),
        vec![logged_in("aaaa0002")]
    );
    assert_eq!(
        events_of("hidden-start-game-launcher-stays.log"),
        vec![logged_in("aaaa0001"), E::GameLaunched]
    );
}

#[test]
fn a_challenge_that_stays_means_a_code() {
    let dir = tempfile::tempdir().unwrap();
    let logs = dir.path().join("Logs");
    fs::create_dir_all(&logs).unwrap();
    fs::write(
        logs.join("battle.net-20261009T000001.log"),
        "I 2026-10-09 01:00:00.000000 [Main] {Main} Command line arguments: {}\nD 2026-10-09 01:00:01.000000 [UnifiedAuth] {} UAuth: browser state changed: Challenge\n",
    )
    .unwrap();
    let mut tail = LogTail::new(STAND);
    let t0 = Instant::now();
    assert!(tail.poll(&logs, t0).events.is_empty());
    let later = tail.poll(&logs, t0 + STAND * 2).events;
    assert_eq!(later.len(), 1);
    assert_eq!(later[0].event, E::CodeScreen);
}

#[test]
fn the_exec_sender_log_is_not_the_main_log() {
    let dir = tempfile::tempdir().unwrap();
    put_log(
        dir.path(),
        "battle.net-20261009T000001.log",
        "auto-login-with-challenge.log",
    );
    put_log(
        dir.path(),
        "battle.net-20261009T000002.log",
        "exec-sender.log",
    );
    let mut tail = LogTail::new(STAND);
    let events: Vec<E> = tail
        .poll(dir.path(), Instant::now())
        .events
        .into_iter()
        .map(|t| t.event)
        .collect();
    assert_eq!(
        events,
        vec![E::GameReadiness(R::Playable), logged_in("aaaa0002")]
    );
}

#[test]
fn tailing_survives_growth_a_new_file_and_truncation() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("battle.net-20261009T000001.log");
    fs::write(&log, "I 2026 [Main] {Main} Command line arguments: {}\n").unwrap();
    let mut tail = LogTail::new(STAND);
    let t0 = Instant::now();
    assert!(tail.poll(dir.path(), t0).events.is_empty());

    // A line written in two halves is read once it is whole.
    let mut f = fs::OpenOptions::new().append(true).open(&log).unwrap();
    use std::io::Write;
    write!(f, "I 2026 [BNLogin] {{Main}} Logged into Battle.net succ").unwrap();
    assert!(tail.poll(dir.path(), t0).events.is_empty());
    writeln!(f, "essfully. |bnet=1:0:aaaa0001|game=2:0:0").unwrap();
    let got = tail.poll(dir.path(), t0).events;
    assert_eq!(got.len(), 1);
    assert!(
        !got[0].historic,
        "only the first poll of a program run reads history"
    );

    // The log is replaced by a shorter one: read from its start.
    fs::write(&log, "I 2026 [Main] {Main} Command line arguments: {}\nI [BNLogin] {Main} Logged into Battle.net successfully. |bnet=1:0:aaaa0002|game=2\n").unwrap();
    let got = tail.poll(dir.path(), t0).events;
    assert_eq!(
        got.last().map(|t| t.event.clone()),
        Some(logged_in("aaaa0002"))
    );

    // A newer launcher run switches files.
    fs::write(
        dir.path().join("battle.net-20261009T000002.log"),
        "I 2026 [Main] {Main} Command line arguments: {}\n",
    )
    .unwrap();
    assert!(tail.poll(dir.path(), t0).new_file);
}

#[test]
fn closed_launcher_session_follows_the_last_login_and_logout() {
    let dir = tempfile::tempdir().unwrap();
    put_log(
        dir.path(),
        "battle.net-20261009T000001.log",
        "auto-login-with-challenge.log",
    );
    assert_eq!(last_session(dir.path()), Some("aaaa0002".into()));
    put_log(
        dir.path(),
        "battle.net-20261009T000002.log",
        "login-then-logout.log",
    );
    assert_eq!(last_session(dir.path()), None);
    put_log(
        dir.path(),
        "battle.net-20261009T000003.log",
        "empty-list-login-screen.log",
    );
    assert_eq!(last_session(dir.path()), None);
}

// ---- config ----------------------------------------------------------------

#[test]
fn config_finds_the_install_and_reads_the_list() {
    let f = Fake::new();
    let cfg = f.paths.config_file();
    assert_eq!(config::find_install(&cfg), Some(f.install.clone()));
    assert_eq!(
        config::read_saved_names(&cfg).as_deref(),
        Some("user1@example.com,user2@example.com,user3@example.com,user4@example.com,user5@example.com")
    );
    assert_eq!(
        config::first_saved_name(&cfg).as_deref(),
        Some("user1@example.com")
    );
    fs::remove_file(f.install.join("Battle.net.exe")).unwrap();
    assert_eq!(
        config::find_install(&cfg),
        None,
        "both executables must exist"
    );
}

#[test]
fn config_write_changes_only_the_value_and_keeps_crlf() {
    let f = Fake::new();
    let before = f.config_bytes();
    assert!(before.windows(2).any(|w| w == b"\r\n"));
    config::write_saved_names(
        &f.paths.config_file(),
        &f.paths.data,
        &f.install,
        &*f.procs,
        "new@example.com",
    )
    .unwrap();
    let after = f.config_bytes();
    let expected = String::from_utf8(before.clone()).unwrap().replace(
        "user1@example.com,user2@example.com,user3@example.com,user4@example.com,user5@example.com",
        "new@example.com",
    );
    assert_eq!(String::from_utf8(after).unwrap(), expected);
    config::write_saved_names(
        &f.paths.config_file(),
        &f.paths.data,
        &f.install,
        &*f.procs,
        "",
    )
    .unwrap();
    assert!(String::from_utf8(f.config_bytes())
        .unwrap()
        .contains("\"SavedAccountNames\": \"\","));
}

#[test]
fn config_backup_is_written_once_and_never_replaced() {
    let f = Fake::new();
    let original = f.config_bytes();
    let write = |v: &str| {
        config::write_saved_names(
            &f.paths.config_file(),
            &f.paths.data,
            &f.install,
            &*f.procs,
            v,
        )
        .unwrap()
    };
    assert!(!f.backup_file().exists());
    write("a@example.com");
    assert_eq!(fs::read(f.backup_file()).unwrap(), original);
    write("b@example.com");
    write("");
    assert_eq!(
        fs::read(f.backup_file()).unwrap(),
        original,
        "the backup stays the first original"
    );
}

#[test]
fn config_write_is_refused_while_a_launcher_runs() {
    let f = Fake::new();
    f.procs.set_running(true);
    let before = f.config_bytes();
    let r = config::write_saved_names(
        &f.paths.config_file(),
        &f.paths.data,
        &f.install,
        &*f.procs,
        "a@example.com",
    );
    assert_eq!(r, Err(config::ConfigError::LauncherRunning));
    assert_eq!(f.config_bytes(), before);
}

#[test]
fn config_without_the_key_reports_it() {
    let f = Fake::new();
    fs::write(f.paths.config_file(), "{\"Client\":{}}").unwrap();
    let r = config::write_saved_names(
        &f.paths.config_file(),
        &f.paths.data,
        &f.install,
        &*f.procs,
        "a@example.com",
    );
    assert_eq!(r, Err(config::ConfigError::KeyMissing));
    assert_eq!(config::read_saved_names(&f.paths.config_file()), None);
}

// ---- cache -------------------------------------------------------------------

#[test]
fn battletag_is_found_by_account_id() {
    let f = Fake::new();
    f.sim.cache_row("1d158388", "Alpha#1111");
    assert_eq!(
        cache::battle_tag(&f.paths.cache_db(), "1d158388").as_deref(),
        Some("Alpha#1111")
    );
    assert_eq!(cache::battle_tag(&f.paths.cache_db(), "aaaa0001"), None);
    assert_eq!(
        cache::battle_tag(&f.paths.local.join("missing.db"), "1d158388"),
        None
    );
}

#[test]
fn battletag_is_read_through_a_copy_when_the_database_is_locked() {
    let f = Fake::new();
    f.sim.cache_row("aaaa0001", "Alpha#1111");
    // An exclusive write lock, held while we read.
    let conn = rusqlite::Connection::open(f.paths.cache_db()).unwrap();
    conn.execute_batch("PRAGMA locking_mode=EXCLUSIVE; BEGIN EXCLUSIVE;")
        .unwrap();
    assert_eq!(
        cache::battle_tag(&f.paths.cache_db(), "aaaa0001").as_deref(),
        Some("Alpha#1111")
    );
}

// ---- data files ----------------------------------------------------------------

#[test]
fn data_files_default_when_missing_and_are_set_aside_when_broken() {
    let dir = tempfile::tempdir().unwrap();
    let accounts = dir.path().join("accounts.json");
    let settings = dir.path().join("settings.json");
    let (r, reset) = Registry::load(&accounts);
    assert!(r.accounts.is_empty() && !reset);
    let (s, reset) = Settings::load(&settings);
    assert!(s.sort == Sort::LastLogin && !reset);

    fs::write(&accounts, "{ not json").unwrap();
    fs::write(&settings, "[]").unwrap();
    assert!(Registry::load(&accounts).1 && Settings::load(&settings).1);
    assert!(dir.path().join("accounts.json.broken").exists());
    assert!(dir.path().join("settings.json.broken").exists());
    assert!(!accounts.exists());
}

#[test]
fn a_record_with_a_bad_or_repeated_id_makes_the_file_broken() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("accounts.json");
    let a = account_json("1d158388", None, None, "ready");
    fs::write(&path, accounts_file(&[a.clone(), a])).unwrap();
    assert!(Registry::load(&path).1, "an ID may appear once");
    fs::write(
        &path,
        accounts_file(&[account_json("1D158388", None, None, "ready")]),
    )
    .unwrap();
    assert!(Registry::load(&path).1, "an ID is lowercase hex");
}

#[test]
fn data_files_round_trip_atomically() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sub").join("settings.json");
    let mut s = Settings::default();
    s.sort = Sort::Battletag;
    s.dismissed.logout_warning = true;
    s.save(&path).unwrap();
    assert!(!dir.path().join("sub").join("settings.json.tmp").exists());
    assert_eq!(Settings::load(&path).0, s);
    assert!(fs::read_to_string(&path)
        .unwrap()
        .contains("\"logout_warning\": true"));
}
