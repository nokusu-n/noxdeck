//! Stand-ins: a temporary launcher directory and a recording `Processes`.
//! Nothing here touches the real launcher.
#![allow(dead_code)]
pub mod overfast;

use noxdeck_lib::launcher::{LauncherPaths, ProcessError, ProcessSnapshot, Processes};
use noxdeck_lib::{Core, State, Timing};
use noxdeck_lib::shell::{Shell, ShellRequest};
use rusqlite::Connection;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tempfile::TempDir;

pub const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");

pub fn fixture(name: &str) -> String {
    fs::read_to_string(Path::new(FIXTURES).join(name)).unwrap()
}

/// Millisecond timings so a flow test takes a fraction of a second.
pub fn fast() -> Timing {
    Timing {
        poll: Duration::from_millis(10),
        stand: Duration::from_millis(120),
        stop: Duration::from_secs(1),
        start_timeout: Duration::from_millis(900),
        gone_grace: Duration::from_millis(120),
        tag_wait: Duration::from_millis(300),
        email_wait: Duration::from_millis(300),
        game_timeout: Duration::from_millis(300),
        game_settle: Duration::from_millis(100),
        game_handover: Duration::from_millis(300),
        handover_pause: Duration::from_millis(100),
        hide_interval: Duration::from_millis(10),
        pid_refresh: Duration::from_millis(250),
        ratings_interval: Duration::from_secs(1800),
    }
}

#[derive(Default)]
struct ProcState {
    running: bool,
    game: bool,
    game_pids: Vec<u32>,
    next_game_pid: u32,
    game_window: Option<bool>,
    game_window_pids: Option<Vec<u32>>,
    deny_stop: bool,
    stop_error: Option<ProcessError>,
    deny_game_stop: bool,
    keep_game_on_stop: bool,
    calls: Vec<String>,
    queries: Vec<&'static str>,
    trace: Vec<&'static str>,
    on_start: Option<Arc<dyn Fn() + Send + Sync>>,
    on_launch: Option<Arc<dyn Fn() + Send + Sync>>,
    hidden: bool,
    journal: Option<Arc<noxdeck_lib::diagnostics::Journal>>,
}

/// Records every call and lets the test decide what "runs".
#[derive(Default)]
pub struct RecordingProcesses {
    s: Mutex<ProcState>,
}

impl RecordingProcesses {
    pub fn set_running(&self, v: bool) {
        self.s.lock().unwrap().running = v;
    }
    pub fn set_game(&self, v: bool) {
        let mut s = self.s.lock().unwrap();
        if v && !s.game {
            s.next_game_pid = s.next_game_pid.max(1000) + 1;
            s.game_pids = vec![s.next_game_pid];
        } else if !v { s.game_pids.clear(); }
        s.game = v;
    }
    pub fn set_game_pids(&self, pids: Vec<u32>) {
        let mut s = self.s.lock().unwrap();
        s.game = !pids.is_empty();
        s.next_game_pid = s.next_game_pid.max(pids.iter().copied().max().unwrap_or(0));
        s.game_pids = pids;
    }
    pub fn set_game_window(&self, visible: bool) {
        let mut s = self.s.lock().unwrap();
        s.game_window = Some(visible);
        s.game_window_pids = None;
    }
    pub fn set_game_windows(&self, pids: Vec<u32>) { self.s.lock().unwrap().game_window_pids = Some(pids); }
    pub fn deny_stop(&self, v: bool) {
        self.s.lock().unwrap().deny_stop = v;
    }
    pub fn deny_game_stop(&self, v: bool) {
        self.s.lock().unwrap().deny_game_stop = v;
    }
    pub fn keep_game_on_stop(&self, v: bool) { self.s.lock().unwrap().keep_game_on_stop = v; }
    pub fn fail_stop(&self, error: ProcessError) { self.s.lock().unwrap().stop_error = Some(error); }
    /// What the "launcher" does when it is started.
    pub fn on_start(&self, f: impl Fn() + Send + Sync + 'static) {
        self.s.lock().unwrap().on_start = Some(Arc::new(f));
    }
    pub fn on_launch(&self, f: impl Fn() + Send + Sync + 'static) {
        self.s.lock().unwrap().on_launch = Some(Arc::new(f));
    }
    pub fn is_hidden(&self) -> bool {
        self.s.lock().unwrap().hidden
    }
    pub fn calls(&self) -> Vec<String> {
        self.s.lock().unwrap().calls.clone()
    }
    pub fn clear_calls(&self) {
        self.s.lock().unwrap().calls.clear();
    }
    pub fn queries(&self) -> Vec<&'static str> {
        self.s.lock().unwrap().queries.clone()
    }
    pub fn trace(&self) -> Vec<&'static str> {
        self.s.lock().unwrap().trace.clone()
    }
    pub fn clear_queries(&self) {
        self.s.lock().unwrap().queries.clear();
    }
    fn record(&self, what: &str) {
        self.s.lock().unwrap().calls.push(what.to_owned());
    }
}

impl Processes for RecordingProcesses {
    fn set_journal(&self, journal: Arc<noxdeck_lib::diagnostics::Journal>) {
        self.s.lock().unwrap().journal = Some(journal);
    }
    fn snapshot(&self, install: Option<&Path>, _: &str) -> ProcessSnapshot {
        let mut s = self.s.lock().unwrap();
        s.queries.push("snapshot");
        ProcessSnapshot {
            launcher_running: install.is_some() && s.running,
            launcher_count: usize::from(install.is_some() && s.running),
            game_running: s.game,
            game_window_pids: s.game_window_pids.as_ref().map(|pids| pids.iter().copied().filter(|pid| s.game_pids.contains(pid)).collect())
                .unwrap_or_else(|| if s.game_window.unwrap_or(true) { s.game_pids.clone() } else { vec![] }),
            game_pids: s.game_pids.clone(),
        }
    }
    fn launcher_running(&self, _: &Path) -> bool {
        let mut s = self.s.lock().unwrap();
        s.queries.push("launcher_running");
        s.running
    }
    fn stop_launcher(&self, _: &Path, _: Duration) -> Result<(), ProcessError> {
        let mut s = self.s.lock().unwrap();
        if let Some(journal) = &s.journal {
            journal.record(noxdeck_lib::diagnostics::Step::LauncherStopProcesses { executables: [0, usize::from(s.running), 0] });
        }
        if s.running {
            s.calls.push("stop_launcher".into());
        }
        if s.deny_stop && s.running {
            return Err(ProcessError::AccessDenied);
        }
        if let Some(error) = &s.stop_error { if s.running { return Err(error.clone()); } }
        s.running = false;
        Ok(())
    }
    fn start_launcher(&self, _: &Path) -> Result<(), ProcessError> {
        let hook = {
            let mut s = self.s.lock().unwrap();
            s.calls.push("start_launcher".into());
            s.trace.push("start_launcher");
            s.running = true;
            s.on_start.clone()
        };
        if let Some(f) = hook {
            f();
        }
        Ok(())
    }
    fn game_running(&self, _: &str) -> bool {
        let mut s = self.s.lock().unwrap();
        s.queries.push("game_running");
        s.game
    }
    fn send_launch_command(&self, _: &Path) -> Result<(), ProcessError> {
        let hook = {
            let mut s = self.s.lock().unwrap();
            s.calls.push("send_launch_command".into());
            s.trace.push("send_launch_command");
            s.on_launch.clone()
        };
        if let Some(f) = hook {
            f();
        }
        Ok(())
    }
    fn hide_windows(&self, _: &Path) {
        let mut s = self.s.lock().unwrap();
        s.hidden = true;
        s.calls.push("hide_windows".into());
    }
    fn refresh_launcher_pids(&self, _: &Path) {
        let mut s = self.s.lock().unwrap();
        s.queries.push("refresh_launcher_pids");
        s.trace.push("refresh_launcher_pids");
    }
    fn show_windows(&self, _: &Path) {
        let mut s = self.s.lock().unwrap();
        s.hidden = false;
        s.calls.push("show_windows".into());
    }
    fn show_windows_no_activate(&self, _: &Path) {
        let mut s = self.s.lock().unwrap();
        s.hidden = false;
        s.calls.push("show_windows_no_activate".into());
    }
    fn stop_game(&self, _: &str) -> Result<(), ProcessError> {
        let mut s = self.s.lock().unwrap();
        s.calls.push("stop_game".into());
        if s.deny_game_stop {
            return Err(ProcessError::AccessDenied);
        }
        if s.keep_game_on_stop { return Ok(()); }
        s.game = false;
        s.game_pids.clear();
        Ok(())
    }
}

static RUN_COUNTER: AtomicU32 = AtomicU32::new(1);

/// Writes what a launcher run would write. Cloneable, so a start hook can own one.
#[derive(Clone)]
pub struct Sim {
    pub paths: LauncherPaths,
}

#[derive(Clone)]
pub struct Run {
    file: PathBuf,
}

impl Sim {
    /// A new main log, as a launcher start creates.
    pub fn new_run(&self) -> Run {
        let n = RUN_COUNTER.fetch_add(1, Ordering::SeqCst);
        let file = self
            .paths
            .logs_dir()
            .join(format!("battle.net-20261009T{n:06}.log"));
        let run = Run { file };
        run.raw("I 2026-10-09 01:00:00.000000 [Main] {Main} Command line arguments: {[0]=--from-launcher}");
        run
    }

    /// A small log from a second `Battle.net.exe --exec=...` invocation.
    pub fn exec_sender_run(&self) {
        let n = RUN_COUNTER.fetch_add(1, Ordering::SeqCst);
        let run = Run {
            file: self
                .paths
                .logs_dir()
                .join(format!("battle.net-20261009T{n:06}.log")),
        };
        run.raw("I 2026-10-09 01:00:00.000000 [Main] {Main} Command line arguments: {[0]=--exec=launch Pro}");
    }

    pub fn set_list(&self, value: &str) {
        let cfg = self.paths.config_file();
        let text = fs::read_to_string(&cfg).unwrap();
        let start =
            text.find("\"SavedAccountNames\": \"").unwrap() + "\"SavedAccountNames\": \"".len();
        let end = start + text[start..].find('"').unwrap();
        fs::write(&cfg, format!("{}{}{}", &text[..start], value, &text[end..])).unwrap();
    }

    pub fn list(&self) -> Option<String> {
        noxdeck_lib::launcher::config::read_saved_names(&self.paths.config_file())
    }

    pub fn cache_row(&self, id: &str, tag: &str) {
        let conn = Connection::open(self.paths.cache_db()).unwrap();
        conn.execute(
            "insert into login_cache (name, environment, battle_tag, account_id_hi, account_id_lo) values (?1, 'eu.actual.battle.net', ?2, 72057594037927936, ?3)",
            rusqlite::params![format!("hash{id}"), tag, i64::from_str_radix(id, 16).unwrap()],
        )
        .unwrap();
    }
}

impl Run {
    pub fn raw(&self, line: &str) {
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.file)
            .unwrap();
        writeln!(f, "{line}").unwrap();
    }
    pub fn line(&self, text: &str) {
        self.raw(&format!(
            "I 2026-10-09 01:00:01.000000 [Test] {{Main}} {text}"
        ));
    }
    pub fn unrelated(&self) {
        self.raw("D 2026-10-09 01:00:01.000000 [ProductState] {Main} InstallState (prometheus): <masked> installed=1");
    }
    pub fn login(&self, id: &str) {
        self.unrelated();
        self.line("Using cached auth token on startup. tokenType=Default");
        self.line(&format!(
            "Logged into Battle.net successfully. |bnet=1:0:{id}|game=2:0:00000000"
        ));
        self.unrelated();
    }
    pub fn logout(&self) {
        self.line("BattleNetLogin::DeleteToken(): Clearing cached token");
        self.line("Logout");
    }
    pub fn browser(&self, state: &str) {
        self.raw(&format!(
            "D 2026-10-09 01:00:01.000000 [UnifiedAuth] {{}} UAuth: browser state changed: {state}"
        ));
    }
    /// The way a normal passwordless login passes through the challenge screen.
    pub fn flicker(&self) {
        self.browser("Challenge");
        self.browser("None");
        self.line("external challenge complete, submitting credentials");
        self.browser("Done");
    }
}

pub struct Fake {
    pub root: TempDir,
    pub paths: LauncherPaths,
    pub install: PathBuf,
    pub procs: Arc<RecordingProcesses>,
    pub sim: Sim,
    pub states: Arc<Mutex<Vec<State>>>,
    pub shell: Arc<RecordingShell>,
}

impl Fake {
    /// A launcher directory like the real one: the fixture config pointed at a
    /// temporary install directory, an empty `Logs`, a `CachedData.db`.
    pub fn new() -> Fake {
        let url = std::env::var("NOXDECK_OVERFAST_URL").expect("Use the test wrapper: NOXDECK_OVERFAST_URL must name a local stub");
        let url = tauri::Url::parse(&url).expect("local stub URL");
        assert!(url.scheme() == "http" && url.host_str() == Some("127.0.0.1"), "core tests require a loopback HTTP stub");
        let root = tempfile::tempdir().unwrap();
        let install = root.path().join("install");
        let roaming = root.path().join("roaming");
        let local = root.path().join("local");
        let data = root.path().join("data");
        for d in [&install, &roaming, &local.join("Logs"), &data] {
            fs::create_dir_all(d).unwrap();
        }
        for exe in ["Battle.net Launcher.exe", "Battle.net.exe"] {
            fs::write(install.join(exe), b"stand-in").unwrap();
        }
        let json_path = install.to_string_lossy().replace('\\', "\\\\");
        let config = fixture("Battle.net.config").replace("C:\\\\Fake\\\\Battle.net", &json_path);
        fs::write(roaming.join("Battle.net.config"), config).unwrap();
        let conn = Connection::open(local.join("CachedData.db")).unwrap();
        conn.execute_batch(
            "CREATE TABLE login_cache (name TEXT NOT NULL, environment TEXT NOT NULL,
             battle_tag TEXT NOT NULL, account_id_hi INT NOT NULL, account_id_lo INT NOT NULL,
             connected_environments TEXT NOT NULL DEFAULT '',
             UNIQUE(account_id_hi, account_id_lo, environment) ON CONFLICT REPLACE)",
        )
        .unwrap();
        let paths = LauncherPaths {
            roaming,
            local,
            data,
            game_exe: "FakeGame.exe".into(),
        };
        Fake {
            sim: Sim {
                paths: paths.clone(),
            },
            paths,
            install,
            procs: Arc::new(RecordingProcesses::default()),
            root,
            states: Arc::new(Mutex::new(Vec::new())),
            shell: Arc::new(RecordingShell::default()),
        }
    }

    pub fn core(&self) -> Arc<Core> {
        let states = Arc::clone(&self.states);
        Core::new(
            self.paths.clone(),
            fast(),
            self.procs.clone(),
            Box::new(move |s| states.lock().unwrap().push(s.clone())),
            "0.0.0-test",
            self.shell.clone(),
        )
    }

    /// Write `accounts.json` before the core starts.
    pub fn seed_accounts(&self, json: &str) {
        fs::write(self.paths.data.join("accounts.json"), json).unwrap();
    }

    pub fn config_bytes(&self) -> Vec<u8> {
        fs::read(self.paths.config_file()).unwrap()
    }

    pub fn backup_file(&self) -> PathBuf {
        self.paths
            .data
            .join("backup")
            .join("Battle.net.config.original")
    }
}

#[derive(Default)]
pub struct RecordingShell(Mutex<Vec<ShellRequest>>);

impl Shell for RecordingShell {
    fn request(&self, request: ShellRequest) { self.0.lock().unwrap().push(request); }
}

impl RecordingShell {
    pub fn requests(&self) -> Vec<ShellRequest> { self.0.lock().unwrap().clone() }
    pub fn clear(&self) { self.0.lock().unwrap().clear(); }
}

pub fn account_json(id: &str, email: Option<&str>, tag: Option<&str>, state: &str) -> String {
    let q = |v: Option<&str>| v.map(|s| format!("\"{s}\"")).unwrap_or("null".into());
    format!(
        "{{\"id\":\"{id}\",\"email\":{},\"battletag\":{},\"login_state\":\"{state}\",\"last_login_at\":null}}",
        q(email),
        q(tag)
    )
}

pub fn accounts_file(accounts: &[String]) -> String {
    format!("{{\"schema\":1,\"accounts\":[{}]}}", accounts.join(","))
}

/// Wait until the running operation, if any, has ended.
pub fn wait_idle(core: &Core) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while core.state().operation.is_some() {
        assert!(Instant::now() < deadline, "the operation did not end");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Wait until `cond` holds on the state.
pub fn wait_for(core: &Core, what: &str, cond: impl Fn(&State) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        core.tick();
        let s = core.state();
        if cond(&s) {
            return;
        }
        assert!(Instant::now() < deadline, "timed out waiting for: {what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

pub fn account<'a>(s: &'a State, id: &str) -> &'a noxdeck_lib::AccountView {
    s.accounts
        .iter()
        .find(|a| a.id == id)
        .unwrap_or_else(|| panic!("no record {id}"))
}
