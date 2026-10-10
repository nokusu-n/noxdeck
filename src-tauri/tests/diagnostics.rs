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
    if let Some(path) = std::env::var_os("NOXDECK_SAMPLE_DIAGNOSTICS") { core.write_diagnostics(std::path::Path::new(&path)).unwrap(); }
    let before: Vec<_> = std::fs::read_dir(&f.paths.data).unwrap().map(|e| e.unwrap().file_name()).collect();
    core.journal.record(Step::Command(Command::LaunchGame));
    let after: Vec<_> = std::fs::read_dir(&f.paths.data).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(before, after, "journalling writes nothing");
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
