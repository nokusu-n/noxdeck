mod common;
use common::*;
use noxdeck_lib::diagnostics::{Journal, SaveResult, Step, Command};
use noxdeck_lib::{Core, shell::{Shell, ShellRequest}};
use std::sync::Arc;

#[test]
fn scripted_start_exports_only_typed_steps_and_no_account_or_launcher_text() {
    let f = Fake::new();
    let (id, email, tag) = ("aaaa0001", "placeholder@example.com", "ExamplePlayer#12345");
    f.seed_accounts(&accounts_file(&[account_json(id, Some(email), Some(tag), "ready")]));
    let sim = f.sim.clone();
    let kept = Arc::new(std::sync::Mutex::new(None));
    let slot = kept.clone();
    f.procs.on_start(move || {
        let run = sim.new_run();
        run.login(id);
        run.line("InstallState (prometheus): installed=1 playable=1");
        run.line("recognisable placeholder launcher text");
        *slot.lock().unwrap() = Some(run);
    });
    let core = Core::new(f.paths.clone(), fast(), f.procs.clone(), Box::new(|_| {}), env!("CARGO_PKG_VERSION"), f.shell.clone());
    core.start_launcher(id).unwrap();
    wait_idle(&core);
    let procs = f.procs.clone();
    f.procs.on_launch(move || {
        kept.lock().unwrap().as_ref().unwrap().line("Launched <game path> -uid prometheus");
        procs.set_game(true);
    });
    core.start_game(id, false).unwrap();
    wait_idle(&core);
    let text = core.diagnostics_text();
    for forbidden in [id, email, tag, "recognisable placeholder launcher text", "Logged into Battle.net", &f.paths.data.to_string_lossy(), &f.install.to_string_lossy()] {
        assert!(!text.contains(forbidden), "sensitive data in diagnostics");
    }
    for expected in ["Operation began: Start, account 1", "Operation ended: Start, account 1, Success", "Launcher event: LoggedIn, account 1", "WaitBegan", "elapsed_ms", "bound_ms", "Snapshot", "StartLauncher", "SelectAccount", "Windows:", "System locale:"] { assert!(text.contains(expected), "missing {expected}"); }
    assert!(text.contains("Operation ended: Game, account 1, Success"));
    assert!(text.contains("GameProcess"));
    assert!(text.contains("Interface language: en\n"));
    assert!(text.contains("Launcher found: yes\n"));
    assert!(text.contains("Noxdeck elevated: yes\n") || text.contains("Noxdeck elevated: no\n") || text.contains("Noxdeck elevated: unknown\n"));
    assert!(!text.split("\nTiming\n").next().unwrap().contains("Some("));
    if let Some(path) = std::env::var_os("NOXDECK_SAMPLE_DIAGNOSTICS") { core.write_diagnostics(std::path::Path::new(&path)).unwrap(); }
    let before: Vec<_> = std::fs::read_dir(&f.paths.data).unwrap().map(|e| e.unwrap().file_name()).collect();
    core.journal.record(Step::Command(Command::LaunchGame));
    let after: Vec<_> = std::fs::read_dir(&f.paths.data).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(before, after, "journalling writes nothing");
}

#[test]
fn header_uses_language_codes_and_plain_launcher_availability() {
    for (language, code) in [(noxdeck_lib::settings::Language::En, "en"), (noxdeck_lib::settings::Language::Ru, "ru"), (noxdeck_lib::settings::Language::It, "it")] {
        let f = Fake::new();
        let core = f.core();
        core.set_language(Some(language));
        assert!(core.diagnostics_text().contains(&format!("Interface language: {code}\n")));
        std::fs::remove_file(f.paths.config_file()).unwrap();
        core.tick();
        assert!(core.diagnostics_text().contains("Launcher found: no\n"));
    }
}

#[test]
fn cap_drops_oldest_and_numbers_stay_stable_after_account_deletion() {
    let journal = Journal::default();
    assert_eq!(journal.account("aaaa0002"), 1);
    assert_eq!(journal.account("aaaa0001"), 2);
    journal.record(Step::Command(Command::ClearAccountList));
    for _ in 0..2000 { journal.record(Step::Command(Command::LaunchGame)); }
    let text = journal.text();
    assert_eq!(text.lines().count(), 2000);
    assert!(!text.contains("ClearAccountList"));
    assert_eq!(journal.account("aaaa0002"), 1);
}

#[test]
fn repeated_hides_of_each_window_have_one_entry_and_a_count() {
    let journal = Journal::default();
    for _ in 0..250 {
        journal.hide_window(1234, 10);
        journal.record(Step::Command(Command::LaunchGame));
        journal.hide_window(9876, 10);
    }
    let text = journal.text();
    assert_eq!(text.lines().filter(|l| l.contains("hidden:")).count(), 2);
    assert!(text.contains("Launcher window 1 hidden: count=250"));
    assert!(text.contains("Launcher window 2 hidden: count=250"));
    assert!(!text.contains("1234") && !text.contains("9876"));
}

#[test]
fn settings_journal_keeps_every_new_value_without_duplicate_or_account_text() {
    use noxdeck_lib::settings::{AfterStart, Language, Sort};
    let f = Fake::new();
    let core = f.core();
    core.set_hide_emails(true);
    core.set_hide_emails(true);
    core.set_close_launcher(false);
    core.set_auto_ratings(false);
    core.set_after_start(AfterStart::Stay);
    core.set_language(Some(Language::It));
    core.set_sort(Sort::Battletag);
    core.dismiss("rememberLogin");
    core.dismiss("logoutWarning");
    let text = core.journal.text();
    for change in ["hide_emails=true", "close_launcher=false", "auto_ratings=false", "after_start=\"stay\"", "language=\"it\"", "sort=\"battletag\"", "remember_login=true", "logout_warning=true"] {
        assert_eq!(text.matches(&format!("Setting changed: {change}")).count(), 1, "{change}");
    }
    assert!(!text.contains('@') && !text.contains('#'));
}

#[test]
fn autostart_is_journalled_from_its_actual_state() {
    let journal = Journal::default();
    let settings = noxdeck_lib::settings::Settings::default();
    journal.settings(&settings, false);
    journal.settings(&settings, true);
    journal.settings(&settings, true);
    assert_eq!(journal.text().matches("Setting changed: autostart=true").count(), 1);
}

struct SaveShell { path: Option<std::path::PathBuf> }
impl Shell for SaveShell {
    fn request(&self, _: ShellRequest) {}
    fn diagnostics_path(&self, name: &str) -> std::io::Result<Option<std::path::PathBuf>> {
        assert!(name.starts_with("noxdeck-diagnostics-"));
        assert!(name.ends_with(".txt"));
        assert_eq!(name.len(), "noxdeck-diagnostics-YYYYMMDD-HHMM.txt".len());
        Ok(self.path.clone())
    }
}

#[test]
fn save_cancel_and_write_failure_return_only_a_result() {
    for choice in [0, 1, 2] {
        let f = Fake::new();
        let path = match choice { 0 => None, 1 => Some(f.root.path().join("chosen.txt")), _ => Some(f.root.path().join("missing/chosen.txt")) };
        let core = Core::new(f.paths.clone(), fast(), f.procs.clone(), Box::new(|_| {}), "test", Arc::new(SaveShell { path: path.clone() }));
        let result = core.save_diagnostics();
        assert_eq!(result, match choice { 0 => SaveResult::Cancelled, 1 => SaveResult::Saved, _ => SaveResult::Failed });
        if choice == 1 { assert!(std::fs::read_to_string(path.unwrap()).unwrap().contains("Journal")); }
        assert!(!f.paths.data.join("chosen.txt").exists());
    }
}
