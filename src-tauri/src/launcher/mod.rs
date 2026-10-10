//! The adapter: the only code that knows Battle.net. Everything outside speaks in
//! `LauncherEvent`s, `Processes` calls and the helpers of the submodules.

pub mod cache;
pub mod config;
pub mod log;
pub mod processes;

use std::path::{Path, PathBuf};
use std::time::Duration;

pub const LAUNCHER_EXE: &str = "Battle.net Launcher.exe";
pub const CLIENT_EXE: &str = "Battle.net.exe";
pub const GAME_LAUNCH_COMMAND: &str = "--exec=launch Pro";

/// A diagnostic executable name can contain neither its directory nor account text.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ExecutableName(String);
impl ExecutableName {
    pub fn from_path(path: &Path) -> Option<Self> {
        let name = path.file_name()?.to_string_lossy();
        let name = if name.contains(['@', '#']) || name.chars().any(char::is_control) {
            "<masked executable>".to_owned()
        } else { name.into_owned() };
        Some(Self(name))
    }
    pub fn file_name(&self) -> &str { &self.0 }
}

/// Where everything lives. The environment can move every location, which is how
/// the tests point the program at a stand-in.
#[derive(Clone, Debug)]
pub struct LauncherPaths {
    /// The launcher's roaming directory (`%APPDATA%\Battle.net`): holds the config.
    pub roaming: PathBuf,
    /// The launcher's local directory (`%LOCALAPPDATA%\Battle.net`): logs and cache.
    pub local: PathBuf,
    /// NoxDeck's own data (`%APPDATA%\NoxDeck`).
    pub data: PathBuf,
    /// File name of the game executable.
    pub game_exe: String,
}

fn env_dir(var: &str, base: &str, tail: &str) -> PathBuf {
    if let Some(v) = std::env::var_os(var) {
        return PathBuf::from(v);
    }
    let base = std::env::var_os(base)
        .map(PathBuf::from)
        .unwrap_or_default();
    base.join(tail)
}

impl LauncherPaths {
    pub fn from_env() -> Self {
        LauncherPaths {
            roaming: env_dir("NOXDECK_BNET_ROAMING", "APPDATA", "Battle.net"),
            local: env_dir("NOXDECK_BNET_LOCAL", "LOCALAPPDATA", "Battle.net"),
            data: env_dir("NOXDECK_DATA_DIR", "APPDATA", "NoxDeck"),
            game_exe: std::env::var("NOXDECK_GAME_EXE").unwrap_or_else(|_| "Overwatch.exe".into()),
        }
    }

    pub fn config_file(&self) -> PathBuf {
        self.roaming.join("Battle.net.config")
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.local.join("Logs")
    }

    pub fn cache_db(&self) -> PathBuf {
        self.local.join("CachedData.db")
    }
}

/// What the log tells about the launcher, already reduced to meaning.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameReadiness {
    Unknown,
    Playable,
    NotPlayable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LauncherEvent {
    GameLaunched,
    GameReadiness(GameReadiness),
    /// Logged in as the account with this ID (eight lowercase hex digits).
    LoggedIn {
        id: String,
    },
    LoggedOut,
    /// The login screen stood long enough to count: a password is wanted.
    LoginScreen,
    /// The challenge screen stood long enough to count: a code is wanted.
    CodeScreen,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProcessError {
    /// The process exists but this user may not stop it (the launcher is elevated).
    AccessDenied,
    System(i32),
    TimedOut,
    Other(String),
}

/// Launcher and game flags read from the same process list.
pub struct ProcessSnapshot {
    pub launcher_running: bool,
    pub launcher_count: usize,
    pub game_running: bool,
    pub game_window_pids: Vec<u32>,
    pub game_pids: Vec<u32>,
}

/// Process control, limited to executables inside the install directory (and the
/// game by name). Tests substitute a recording implementation.
pub trait Processes: Send + Sync {
    fn set_journal(&self, _journal: std::sync::Arc<crate::diagnostics::Journal>) {}
    /// Answer both watcher questions from one process list.
    fn snapshot(&self, install: Option<&Path>, exe_name: &str) -> ProcessSnapshot;
    fn launcher_running(&self, install: &Path) -> bool;
    /// Stop every launcher process and wait until none is left.
    fn stop_launcher(&self, install: &Path, timeout: Duration) -> Result<(), ProcessError>;
    fn start_launcher(&self, install: &Path) -> Result<(), ProcessError>;
    fn send_launch_command(&self, install: &Path) -> Result<(), ProcessError>;
    fn refresh_launcher_pids(&self, install: &Path);
    fn hide_windows(&self, install: &Path);
    fn show_windows(&self, install: &Path);
    /// Restore only remembered windows without activating them over the game.
    fn show_windows_no_activate(&self, install: &Path);
    fn game_running(&self, exe_name: &str) -> bool;
    fn stop_game(&self, exe_name: &str) -> Result<(), ProcessError>;
}
