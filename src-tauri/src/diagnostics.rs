//! An in-memory journal. Entries accept only typed facts, never launcher text.
use crate::{lock, launcher::{LauncherEvent, ProcessError, ProcessSnapshot}, Core, OpKind, Phase, Refusal};
use serde::Serialize;
use std::collections::{BTreeMap, VecDeque};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

#[derive(Clone, Copy, Debug)]
pub enum WaitFor { LauncherStopped, Login, AddLogin, Email, BattleTag, GameProcess, GameSettle, GameHandover, HandoverPause, GameStopped, Ratings }
#[derive(Clone, Copy, Debug)]
pub enum WaitEnd { Event, Bound, Gone, Cancelled, Failed, Elapsed }
#[derive(Clone, Copy, Debug)]
pub enum Command { StartLauncher, LaunchGame, StopLauncher, StopGame, SelectAccount, ClearAccountList, RestoreAccountList }
#[derive(Clone, Copy, Debug)]
pub enum Window { LauncherHidden, LauncherShown, LauncherShownWithoutActivation, LauncherDestroyed, AppShown, AppHidden, AppDestroyed }
#[derive(Clone, Copy, Debug)]
pub enum Event { GameLaunchStarted, GameLaunched, Playable, NotPlayable, ReadinessUnknown, LoggedIn, LoggedOut, LoginScreen, CodeScreen }
impl Event {
    pub fn from_launcher(event: &LauncherEvent) -> Self {
        use crate::launcher::GameReadiness::*;
        match event {
            LauncherEvent::GameLaunchStarted => Self::GameLaunchStarted,
            LauncherEvent::GameLaunched => Self::GameLaunched,
            LauncherEvent::GameReadiness(Playable) => Self::Playable,
            LauncherEvent::GameReadiness(NotPlayable) => Self::NotPlayable,
            LauncherEvent::GameReadiness(Unknown) => Self::ReadinessUnknown,
            LauncherEvent::LoggedIn { .. } => Self::LoggedIn,
            LauncherEvent::LoggedOut => Self::LoggedOut,
            LauncherEvent::LoginScreen => Self::LoginScreen,
            LauncherEvent::CodeScreen => Self::CodeScreen,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub enum Outcome { Success, NoAnswer, GameNeedsUpdate, GameNotStarted, PasswordWanted, CodeWanted, Cancelled, Failed }
#[derive(Clone, Debug)]
pub enum Step {
    OperationBegan { kind: OpKind, account: Option<u32> },
    OperationRefused { kind: OpKind, refusal: Refusal },
    OperationEnded { kind: OpKind, account: Option<u32>, outcome: Outcome },
    Phase(Phase),
    WaitBegan { what: WaitFor, bound_ms: Option<u128> },
    WaitEnded { what: WaitFor, bound_ms: Option<u128>, elapsed_ms: u128, end: WaitEnd },
    LauncherEvent { event: Event, account: Option<u32>, historic: bool },
    Snapshot { launcher_running: bool, launcher_count: usize, game_running: bool, game_window: bool },
    Command(Command),
    ConfigWritten { success: bool },
    CommandResult { command: Command, success: bool, error_code: Option<i32> },
    StopAttempt { game: bool, success: bool, error_code: Option<i32> },
    StopSnapshot { game: bool, remaining: usize },
    LauncherStopProcesses { selected: bool, executables: BTreeMap<crate::launcher::ExecutableName, usize> },
    Window(Window),
    WindowHidden { window: u32, count: u32 },
    SettingChanged { name: &'static str, value: serde_json::Value },
    AccountGameChanged { account: u32, game: crate::registry::Game },
}
struct Entry { wall: SystemTime, at: Instant, step: Step }
#[derive(Default)]
struct Entries {
    entries: VecDeque<Entry>,
    accounts: Vec<String>,
    snapshot: Option<(bool, usize, bool, bool)>,
    hidden_windows: Vec<(usize, u32)>,
    settings: Option<crate::settings::Settings>,
    autostart: Option<bool>,
}
#[derive(Default)]
pub struct Journal(Mutex<Entries>);
impl Journal {
    pub fn hide_window(&self, handle: usize, pid: u32) {
        let mut entries = lock(&self.0);
        let number = match entries.hidden_windows.iter().position(|key| *key == (handle, pid)) {
            Some(i) => i as u32 + 1,
            None => { entries.hidden_windows.push((handle, pid)); entries.hidden_windows.len() as u32 },
        };
        if let Some(entry) = entries.entries.iter_mut().find(|e| matches!(e.step, Step::WindowHidden { window, .. } if window == number)) {
            if let Step::WindowHidden { count, .. } = &mut entry.step { *count = count.saturating_add(1); }
        } else {
            if entries.entries.len() == 2000 { entries.entries.pop_front(); }
            entries.entries.push_back(Entry { wall: SystemTime::now(), at: Instant::now(), step: Step::WindowHidden { window: number, count: 1 } });
        }
    }
    pub fn settings(&self, settings: &crate::settings::Settings, autostart: bool) {
        let changes = {
            let mut entries = lock(&self.0);
            let mut changes = Vec::new();
            if let Some(previous) = &entries.settings {
                let old = serde_json::to_value(previous).unwrap();
                let new = serde_json::to_value(settings).unwrap();
                for name in ["sort", "tab", "tab_sorts", "after_start", "close_launcher", "auto_ratings", "hide_emails", "language"] {
                    if old[name] != new[name] { changes.push(Step::SettingChanged { name, value: new[name].clone() }); }
                }
                for name in ["remember_login", "logout_warning"] {
                    if old["dismissed"][name] != new["dismissed"][name] { changes.push(Step::SettingChanged { name, value: new["dismissed"][name].clone() }); }
                }
            }
            if entries.autostart.is_some_and(|old| old != autostart) { changes.push(Step::SettingChanged { name: "autostart", value: autostart.into() }); }
            entries.settings = Some(settings.clone());
            entries.autostart = Some(autostart);
            changes
        };
        for step in changes { self.record(step); }
    }
    pub fn account(&self, id: &str) -> u32 {
        let mut entries = lock(&self.0);
        if let Some(n) = entries.accounts.iter().position(|a| a == id) { return n as u32 + 1; }
        entries.accounts.push(id.to_owned());
        entries.accounts.len() as u32
    }
    pub fn record(&self, step: Step) {
        let mut entries = lock(&self.0);
        if entries.entries.len() == 2000 { entries.entries.pop_front(); }
        entries.entries.push_back(Entry { wall: SystemTime::now(), at: Instant::now(), step });
    }
    pub fn snapshot(&self, snapshot: &ProcessSnapshot) {
        let value = (snapshot.launcher_running, snapshot.launcher_count, snapshot.game_running, !snapshot.game_window_pids.is_empty());
        let changed = {
            let mut entries = lock(&self.0);
            let changed = entries.snapshot != Some(value);
            entries.snapshot = Some(value);
            changed
        };
        if changed { self.record(Step::Snapshot { launcher_running: value.0, launcher_count: value.1, game_running: value.2, game_window: value.3 }); }
    }
    pub fn text(&self) -> String {
        use std::fmt::Write;
        let entries = lock(&self.0);
        let mut text = String::new();
        let mut previous = None;
        for entry in &entries.entries {
            let delta = previous.map_or(0, |previous| entry.at.saturating_duration_since(previous).as_millis());
            previous = Some(entry.at);
            let _ = writeln!(text, "{} +{}ms {}", wall_time(entry.wall), delta, step_text(&entry.step));
        }
        text
    }
    pub fn wait(self: &Arc<Self>, what: WaitFor, bound: Option<Duration>) -> Wait {
        let bound_ms = bound.map(|b| b.as_millis());
        self.record(Step::WaitBegan { what, bound_ms });
        Wait { journal: self.clone(), what, bound_ms, started: Instant::now(), end: WaitEnd::Failed }
    }
}
pub struct Wait { journal: Arc<Journal>, what: WaitFor, bound_ms: Option<u128>, started: Instant, end: WaitEnd }
impl Wait { pub fn end(&mut self, end: WaitEnd) { self.end = end; } }
impl Drop for Wait {
    fn drop(&mut self) { self.journal.record(Step::WaitEnded { what: self.what, bound_ms: self.bound_ms, elapsed_ms: self.started.elapsed().as_millis(), end: self.end }); }
}
fn account_text(account: Option<u32>) -> String { account.map_or_else(|| "no account".into(), |n| format!("account {n}")) }
fn step_text(step: &Step) -> String {
    match step {
        Step::OperationBegan { kind, account } => format!("Operation began: {kind:?}, {}", account_text(*account)),
        Step::OperationEnded { kind, account, outcome } => format!("Operation ended: {kind:?}, {}, {outcome:?}", account_text(*account)),
        Step::LauncherEvent { event, account, historic } => format!("Launcher event: {event:?}, {}, historic={historic}", account_text(*account)),
        Step::SettingChanged { name, value } => format!("Setting changed: {name}={value}"),
        Step::AccountGameChanged { account, game } => format!("Account game changed: account {account}, {game:?}"),
        Step::WindowHidden { window, count } => format!("Launcher window {window} hidden: count={count}"),
        Step::LauncherStopProcesses { selected, executables } => {
            let names: Vec<_> = executables.iter().map(|(exe, count)| format!("{}={count}", exe.file_name())).collect();
            format!("Launcher directory processes {}: count={}, {}", if *selected { "selected" } else { "excluded" }, executables.values().sum::<usize>(), if names.is_empty() { "none".into() } else { names.join(", ") })
        }
        step => format!("{step:?}"),
    }
}
pub fn error_code(error: &ProcessError) -> Option<i32> {
    match error { ProcessError::AccessDenied => Some(5), ProcessError::System(code) => Some(*code), _ => None }
}
pub fn command_result(journal: &Journal, command: Command, result: &Result<(), ProcessError>) {
    journal.record(Step::CommandResult { command, success: result.is_ok(), error_code: result.as_ref().err().and_then(error_code) });
}
#[derive(Serialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SaveResult { Saved, Cancelled, Failed }
impl Core {
    pub fn diagnostics_text(&self) -> String {
        use std::fmt::Write;
        let inner = lock(&self.inner);
        let language = match inner.settings.resolved_language(&self.locale) {
            crate::settings::Language::En => "en", crate::settings::Language::Ru => "ru", crate::settings::Language::It => "it",
        };
        let mut text = format!("Noxdeck {} diagnostics\nTimes are UTC; durations are milliseconds.\nWindows: {}\nSystem locale: {}\nInterface language: {}\nNoxdeck elevated: {}\nLauncher found: {}\n\nTiming\n", self.version(), windows_version(), safe_locale(&self.locale), language, yes_no(elevated()), yes_no(Some(inner.install.is_some())));
        for (name, value) in self.timing.values() { let _ = writeln!(text, "{name}: {}ms", value.as_millis()); }
        text.push_str("\nSettings\n");
        let _ = writeln!(text, "{}\nautostart: {}", serde_json::to_string_pretty(&inner.settings).unwrap(), inner.autostart);
        let _ = writeln!(text, "\nAccounts: {}", inner.registry.accounts.len());
        for account in &inner.registry.accounts {
            let n = self.journal.account(&account.id);
            let _ = writeln!(text, "account {n}: {:?}, has e-mail={}, has BattleTag={}", account.login_state, account.email.is_some(), account.battletag.is_some());
        }
        text.push_str("\nJournal\n");
        text.push_str(&self.journal.text());
        text
    }
    pub fn write_diagnostics(&self, path: &Path) -> std::io::Result<()> {
        std::fs::write(path, self.diagnostics_text())
    }
    pub fn save_diagnostics(&self) -> SaveResult {
        let timestamp = crate::registry::now_iso().replace(['-', ':'], "");
        let name = format!("noxdeck-diagnostics-{}-{}.txt", &timestamp[..8], &timestamp[9..13]);
        match self.shell.diagnostics_path(&name) {
            Ok(Some(path)) => if self.write_diagnostics(&path).is_ok() { SaveResult::Saved } else { SaveResult::Failed },
            Ok(None) => SaveResult::Cancelled,
            Err(_) => SaveResult::Failed,
        }
    }
}
fn yes_no(value: Option<bool>) -> &'static str {
    match value { Some(true) => "yes", Some(false) => "no", None => "unknown" }
}
#[cfg(test)]
mod tests {
    #[test]
    fn optional_booleans_read_plainly() {
        assert_eq!(super::yes_no(Some(true)), "yes");
        assert_eq!(super::yes_no(Some(false)), "no");
        assert_eq!(super::yes_no(None), "unknown");
    }
}
fn safe_locale(locale: &str) -> &str {
    if !locale.is_empty() && locale.len() <= 85 && locale.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_')) { locale } else { "unknown" }
}
fn wall_time(time: SystemTime) -> String {
    let now = crate::registry::iso_time(time);
    let millis = time.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().subsec_millis();
    format!("{}.{millis:03}Z", now.trim_end_matches('Z'))
}
#[repr(C)]
struct Version { size: u32, major: u32, minor: u32, build: u32, platform: u32, service: [u16; 128] }
#[link(name = "ntdll")]
extern "system" { fn RtlGetVersion(version: *mut Version) -> i32; }
fn windows_version() -> String {
    let mut version = Version { size: std::mem::size_of::<Version>() as u32, major: 0, minor: 0, build: 0, platform: 0, service: [0; 128] };
    if unsafe { RtlGetVersion(&mut version) } == 0 { format!("{}.{} build {}", version.major, version.minor, version.build) } else { "unknown".into() }
}
#[link(name = "kernel32")]
extern "system" { fn GetCurrentProcess() -> *mut std::ffi::c_void; fn CloseHandle(handle: *mut std::ffi::c_void) -> i32; }
#[link(name = "advapi32")]
extern "system" {
    fn OpenProcessToken(process: *mut std::ffi::c_void, access: u32, token: *mut *mut std::ffi::c_void) -> i32;
    fn GetTokenInformation(token: *mut std::ffi::c_void, class: u32, info: *mut std::ffi::c_void, size: u32, returned: *mut u32) -> i32;
}
fn elevated() -> Option<bool> {
    unsafe {
        let mut token = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), 8, &mut token) == 0 { return None; }
        let (mut elevation, mut returned) = (0u32, 0u32);
        let ok = GetTokenInformation(token, 20, (&mut elevation as *mut u32).cast(), 4, &mut returned) != 0;
        CloseHandle(token);
        ok.then_some(elevation != 0)
    }
}
