//! Noxdeck core: shared state, the `State` the interface renders, the commands.

mod game_state;
pub mod launcher;
pub mod operations;
pub mod registry;
pub mod ratings;
pub mod settings;
pub mod shell;
#[cfg(debug_assertions)]
mod test_support;
pub mod tray;
pub mod watcher;

use game_state::GameTracking;
use launcher::log::LogTail;
use launcher::{GameReadiness, LauncherEvent, LauncherPaths, Processes};
use registry::{LoginState, Registry};
use serde::Serialize;
use settings::{AfterStart, Language, Settings, Sort};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

/// Every wait and interval of the core. Production values by default; tests use
/// milliseconds.
#[derive(Clone, Debug)]
pub struct Timing {
    pub poll: Duration,
    /// How long a login or challenge screen must stand before it counts.
    pub stand: Duration,
    /// How long stopping the launcher may take.
    pub stop: Duration,
    /// How long a start waits for an outcome (FR-023).
    pub start_timeout: Duration,
    /// How long the launcher may be absent before it counts as closed.
    pub gone_grace: Duration,
    pub tag_wait: Duration,
    pub email_wait: Duration,
    pub game_timeout: Duration,
    pub game_settle: Duration,
    pub hide_interval: Duration,
    pub pid_refresh: Duration,
    pub ratings_interval: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Timing {
            poll: Duration::from_millis(500),
            stand: launcher::log::STAND,
            stop: Duration::from_secs(10),
            start_timeout: Duration::from_secs(60),
            gone_grace: Duration::from_secs(3),
            tag_wait: Duration::from_secs(10),
            email_wait: Duration::from_secs(15),
            game_timeout: Duration::from_secs(20),
            game_settle: Duration::from_secs(5),
            hide_interval: Duration::from_millis(10),
            pid_refresh: Duration::from_millis(250),
            ratings_interval: Duration::from_secs(30 * 60),
        }
    }
}

impl Timing {
    /// The defaults, with `NOXDECK_POLL_MS`, `NOXDECK_STAND_MS`, `NOXDECK_START_TIMEOUT_MS`
    /// `NOXDECK_GONE_GRACE_MS`, `NOXDECK_GAME_TIMEOUT_MS`, `NOXDECK_GAME_SETTLE_MS`
    /// and `NOXDECK_HIDE_INTERVAL_MS` available to the interface tests.
    pub fn from_env() -> Self {
        let ms = |var: &str, default: Duration| {
            std::env::var(var)
                .ok()
                .and_then(|v| v.parse().ok())
                .map(Duration::from_millis)
                .unwrap_or(default)
        };
        let d = Timing::default();
        Timing {
            poll: ms("NOXDECK_POLL_MS", d.poll),
            stand: ms("NOXDECK_STAND_MS", d.stand),
            start_timeout: ms("NOXDECK_START_TIMEOUT_MS", d.start_timeout),
            gone_grace: ms("NOXDECK_GONE_GRACE_MS", d.gone_grace),
            game_timeout: ms("NOXDECK_GAME_TIMEOUT_MS", d.game_timeout),
            game_settle: ms("NOXDECK_GAME_SETTLE_MS", d.game_settle),
            hide_interval: ms("NOXDECK_HIDE_INTERVAL_MS", d.hide_interval),
            ratings_interval: ms("NOXDECK_RATINGS_MS", d.ratings_interval),
            ..d
        }
    }
}

/// An expected "no": the interface turns it into a message or a question.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Refusal {
    RepositoryOpenFailed,
    InvalidRating,
    AutostartFailed,
    Busy,
    LauncherNotFound,
    LauncherNotClosable,
    NoSavedLogin,
    AlreadyListed,
    NoEmail,
    AlreadyOpen,
    AlreadyInGame,
    UnknownAccount,
    GameRunning,
    GameNotClosable,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LauncherKind {
    NotFound,
    Closed,
    LoginScreen,
    LoggedIn,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OpKind {
    Add,
    Start,
    Game,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Stopping,
    Starting,
    WaitingLogin,
    Finishing,
    LaunchingGame,
    ClosingLauncher,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OperationView {
    pub kind: OpKind,
    pub account_id: Option<String>,
    pub phase: Phase,
    pub cancellable: bool,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AccountView {
    pub ratings: ratings::Ratings,
    pub id: String,
    pub battletag: Option<String>,
    pub email: Option<String>,
    pub login_state: LoginState,
    pub last_login_at: Option<String>,
    pub is_current: bool,
    pub is_in_game: bool,
    pub last_outcome: Option<&'static str>,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DismissedView {
    pub remember_login: bool,
    pub logout_warning: bool,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    pub close_launcher: bool,
    pub auto_ratings: bool,
    pub show_ratings: bool,
    pub autostart: bool,
    pub after_start: AfterStart,
    pub show_game_start: bool,
    pub language: Option<Language>,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Ask {
    CloseGame {
        #[serde(rename = "accountId")]
        account_id: String,
    },
}

/// Sent whole as the `state` event after every change (`contracts/ui-core.md`).
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct State {
    pub syncing: bool,
    pub version: String,
    pub launcher: LauncherKind,
    pub game_running: bool,
    pub operation: Option<OperationView>,
    pub accounts: Vec<AccountView>,
    pub sort: Sort,
    pub dismissed: DismissedView,
    pub notice: Option<&'static str>,
    pub lang: Language,
    pub settings: SettingsView,
    pub ask: Option<Ask>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Session {
    LoginScreen,
    LoggedIn(String),
}

#[derive(Clone, Debug)]
pub(crate) struct Operation {
    pub kind: OpKind,
    pub account_id: Option<String>,
    pub phase: Phase,
}

pub(crate) struct Inner {
    pub syncing: bool,
    pub ratings_last: Option<std::time::SystemTime>,
    pub ratings_pending: Vec<String>,
    pub registry: Registry,
    pub settings: Settings,
    pub install: Option<PathBuf>,
    pub launcher_running: bool,
    pub game_running: bool,
    pub session: Session,
    pub game_readiness: GameReadiness,
    pub game: GameTracking,
    /// Events the log produced, numbered, newest last; operations wait on these.
    pub seq: u64,
    pub events: Vec<(u64, LauncherEvent)>,
    pub operation: Option<Operation>,
    pub outcomes: HashMap<String, &'static str>,
    pub notice: bool,
    pub autostart: bool,
    pub ask: Option<Ask>,
}

pub struct Core {
    pub(crate) ratings_trigger: std::sync::mpsc::SyncSender<ratings::Refresh>,
    pub paths: LauncherPaths,
    pub timing: Timing,
    pub(crate) processes: Arc<dyn Processes>,
    pub(crate) inner: Mutex<Inner>,
    /// Also the lock that makes one `tick` run at a time.
    pub(crate) tail: Mutex<LogTail>,
    emit: Box<dyn Fn(&State) + Send + Sync>,
    last_emitted: Mutex<Option<String>>,
    pub(crate) cancel: AtomicBool,
    version: String,
    locale: String,
    pub(crate) shell: Arc<dyn shell::Shell>,
}

pub(crate) fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Core {
    pub fn open_repository(&self) -> Result<(), Refusal> {
        self.shell.open_url("https://github.com/nokusu-n/noxdeck").map_err(|_| Refusal::RepositoryOpenFailed)
    }
    pub fn set_show_ratings(&self, value: bool) {
        let enabled;
        {
            let mut inner = lock(&self.inner);
            enabled = value && !inner.settings.show_ratings;
            inner.settings.show_ratings = value;
            self.save_settings(&inner);
        }
        self.publish();
        if enabled { self.sync_ratings(); }
    }
    pub fn set_rating(&self, id: &str, role: ratings::Role, value: Option<ratings::RatingValue>) -> Result<(), Refusal> {
        if value.as_ref().is_some_and(|v| !v.valid()) { return Err(Refusal::InvalidRating); }
        {
            let mut inner = lock(&self.inner);
            let account = inner.registry.find_mut(id).ok_or(Refusal::UnknownAccount)?;
            *account.ratings.role_mut(role) = value.map(|value| ratings::Rating { value, source: ratings::Source::Manual });
            self.save_registry(&inner);
        }
        self.publish();
        Ok(())
    }
    pub fn set_autostart(&self, value: bool) -> Result<(), Refusal> {
        let result = self
            .shell
            .set_autostart(value)
            .map_err(|_| Refusal::AutostartFailed);
        self.tick();
        result
    }
    pub fn set_after_start(&self, value: AfterStart) {
        {
            let mut inner = lock(&self.inner);
            inner.settings.after_start = value;
            self.save_settings(&inner);
        }
        self.publish();
    }
    pub fn set_close_launcher(&self, value: bool) {
        {
            let mut inner = lock(&self.inner);
            inner.settings.close_launcher = value;
            self.save_settings(&inner);
        }
        self.publish();
    }
    pub fn set_show_game_start(&self, value: bool) {
        {
            let mut inner = lock(&self.inner);
            inner.settings.show_game_start = value;
            self.save_settings(&inner);
        }
        self.publish();
    }
    pub fn set_language(&self, value: Option<Language>) {
        {
            let mut inner = lock(&self.inner);
            inner.settings.language = value;
            self.save_settings(&inner);
        }
        self.publish();
    }
    pub fn new(
        paths: LauncherPaths,
        timing: Timing,
        processes: Arc<dyn Processes>,
        emit: Box<dyn Fn(&State) + Send + Sync>,
        version: &str,
        shell: Arc<dyn shell::Shell>,
    ) -> Arc<Core> {
        Self::new_with_ratings(paths, timing, processes, emit, version, shell, ratings::overfast::Client::from_env())
    }

    pub fn new_with_ratings(
        paths: LauncherPaths,
        timing: Timing,
        processes: Arc<dyn Processes>,
        emit: Box<dyn Fn(&State) + Send + Sync>,
        version: &str,
        shell: Arc<dyn shell::Shell>,
        ratings: ratings::overfast::Client,
    ) -> Arc<Core> {
        let (ratings_trigger, receiver) = std::sync::mpsc::sync_channel(1);
        let (registry, reset_a) = Registry::load(&paths.data.join("accounts.json"));
        let (settings, reset_s) = Settings::load(&paths.data.join("settings.json"));
        let (game, reset_g) = GameTracking::load(&paths.data.join("state.json"));
        let core = Arc::new(Core {
            ratings_trigger,
            tail: Mutex::new(LogTail::new(timing.stand)),
            inner: Mutex::new(Inner {
                syncing: false,
                ratings_last: None,
                ratings_pending: Vec::new(),
                registry,
                settings,
                install: None,
                launcher_running: false,
                game_running: false,
                session: Session::LoginScreen,
                game_readiness: GameReadiness::Unknown,
                game,
                seq: 0,
                events: Vec::new(),
                operation: None,
                outcomes: HashMap::new(),
                notice: reset_a || reset_s || reset_g,
                autostart: false,
                ask: None,
            }),
            paths,
            timing,
            processes,
            emit,
            last_emitted: Mutex::new(None),
            cancel: AtomicBool::new(false),
            version: version.to_owned(),
            locale: settings::user_locale(),
            shell,
        });
        // The first poll reads what the launcher logged before we started watching.
        core.tick();
        ratings::spawn(&core, ratings, receiver);
        core.sync_ratings();
        core
    }

    pub fn version(&self) -> &str {
        &self.version
    }

    pub(crate) fn emit_state(&self, state: &State) {
        let json = serde_json::to_string(state).unwrap_or_default();
        let mut last = lock(&self.last_emitted);
        if last.as_deref() != Some(json.as_str()) {
            *last = Some(json);
            (self.emit)(state);
            self.shell.request(shell::ShellRequest::TrayChanged);
        }
    }

    pub(crate) fn save_registry(&self, inner: &Inner) {
        // ponytail: a failed write is dropped; the list stays right in memory. It starts
        // to matter when a disk is read-only, which needs a message in the interface.
        let _ = inner.registry.save(&self.paths.data.join("accounts.json"));
    }

    pub(crate) fn save_settings(&self, inner: &Inner) {
        let _ = inner.settings.save(&self.paths.data.join("settings.json"));
    }

    pub(crate) fn save_game(&self, inner: &Inner) {
        // ponytail: failed own-file writes stay in memory, as for accounts/settings;
        // a disk-write error message is needed when read-only storage is supported.
        let _ = inner.game.save(&self.paths.data.join("state.json"));
    }
}

// ---- Tauri side ---------------------------------------------------------------

mod commands {
    use super::*;
    use tauri::State as Managed;

    type Shared<'a> = Managed<'a, Arc<Core>>;

    #[tauri::command]
    pub fn get_state(core: Shared) -> State {
        core.state()
    }
    #[tauri::command]
    pub fn open_repository(core: Shared) -> Result<(), Refusal> {
        core.open_repository()
    }

    #[tauri::command]
    pub fn set_rating(core: Shared, id: String, role: ratings::Role, value: Option<ratings::RatingValue>) -> Result<(), Refusal> {
        core.set_rating(&id, role, value)
    }

    #[tauri::command(async)]
    pub fn add_account(core: Shared) -> Result<(), Refusal> {
        core.inner().add_account()
    }

    #[tauri::command]
    pub fn cancel_operation(core: Shared) {
        core.cancel_operation()
    }

    #[tauri::command(async)]
    pub fn save_current(core: Shared) -> Result<serde_json::Value, Refusal> {
        core.save_current()
            .map(|id| serde_json::json!({ "id": id }))
    }

    #[tauri::command(async)]
    pub fn start_launcher(core: Shared, id: String) -> Result<(), Refusal> {
        core.inner().start_launcher(&id)
    }

    #[tauri::command(async)]
    pub fn start_game(core: Shared, id: String, close_game: bool) -> Result<(), Refusal> {
        core.inner().start_game(&id, close_game)
    }

    #[tauri::command]
    pub fn delete_account(core: Shared, id: String) -> Result<(), Refusal> {
        core.delete_account(&id)
    }

    #[tauri::command]
    pub fn set_sort(core: Shared, sort: Sort) {
        core.set_sort(sort)
    }

    #[tauri::command]
    pub fn dismiss(core: Shared, what: String) {
        core.dismiss(&what)
    }

    #[tauri::command]
    pub fn acknowledge_notice(core: Shared) {
        core.acknowledge_notice()
    }

    #[tauri::command]
    pub fn clear_ask(core: Shared) {
        core.clear_ask();
    }

    #[tauri::command]
    pub fn set_after_start(core: Shared, value: AfterStart) {
        core.set_after_start(value);
    }
    #[tauri::command]
    pub fn set_close_launcher(core: Shared, value: bool) {
        core.set_close_launcher(value);
    }
    #[tauri::command]
    pub fn set_show_game_start(core: Shared, value: bool) {
        core.set_show_game_start(value);
    }
    #[tauri::command]
    pub fn set_show_ratings(core: Shared, value: bool) {
        core.set_show_ratings(value);
    }
    #[tauri::command]
    pub fn set_auto_ratings(core: Shared, value: bool) {
        core.set_auto_ratings(value);
    }
    #[tauri::command]
    pub fn sync_ratings(core: Shared) {
        core.sync_ratings();
    }
    #[tauri::command]
    pub fn set_language(core: Shared, value: Option<Language>) {
        core.set_language(value);
    }
    #[tauri::command]
    pub fn set_autostart(core: Shared, value: bool) -> Result<(), Refusal> {
        core.set_autostart(value)
    }

    #[cfg(debug_assertions)]
    #[tauri::command]
    pub fn test_process_id() -> Result<u32, Refusal> {
        if std::env::var_os("NOXDECK_INSTANCE").is_none() {
            return Err(Refusal::Busy);
        }
        Ok(std::process::id())
    }

    #[cfg(debug_assertions)]
    #[tauri::command]
    pub fn test_tray_menu(core: Shared) -> Result<Vec<crate::tray::TrayItem>, Refusal> {
        if std::env::var_os("NOXDECK_INSTANCE").is_none() {
            return Err(Refusal::Busy);
        }
        Ok(crate::tray::menu(&core.state()))
    }

    #[cfg(debug_assertions)]
    #[tauri::command(async)]
    pub fn test_tray_click(core: Shared, id: String) -> Result<(), Refusal> {
        if std::env::var_os("NOXDECK_INSTANCE").is_none() {
            return Err(Refusal::Busy);
        }
        crate::tray::click(core.inner(), &id);
        Ok(())
    }
}

pub fn run() {
    use tauri::{Emitter, Manager};
    let (requests, receiver) = std::sync::mpsc::channel();
    let start_in_tray = std::env::args().any(|arg| arg == "--tray");
    let Some(_instance) = shell::instance::InstanceGuard::acquire(requests.clone(), start_in_tray)
        .expect("Noxdeck instance guard")
    else {
        return;
    };
    let desktop: Arc<dyn shell::Shell> = Arc::new(shell::Desktop(requests));
    let mut context = tauri::generate_context!();
    if std::env::var_os("NOXDECK_INSTANCE").is_some() {
        context.config_mut().app.windows[0].data_directory =
            Some(LauncherPaths::from_env().data.join("webview"));
    }
    tauri::Builder::default()
        .setup(move |app| {
            #[cfg(not(debug_assertions))]
            shell::autostart::Entry::configured().repair(&std::env::current_exe()?)?;
            let handle = app.handle().clone();
            let core = Core::new(
                LauncherPaths::from_env(),
                Timing::from_env(),
                Arc::new(launcher::processes::RealProcesses::default()),
                Box::new(move |state| {
                    let _ = handle.emit("state", state);
                }),
                &app.package_info().version.to_string(),
                desktop,
            );
            watcher::spawn(Arc::clone(&core));
            app.manage(core.clone());
            shell::setup(app, receiver, start_in_tray)?;
            #[cfg(debug_assertions)]
            test_support::start(&core);
            Ok(())
        })
        .on_window_event(shell::window_event)
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::open_repository,
            commands::set_rating,
            commands::add_account,
            commands::cancel_operation,
            commands::save_current,
            commands::start_launcher,
            commands::start_game,
            commands::delete_account,
            commands::set_sort,
            commands::dismiss,
            commands::acknowledge_notice,
            commands::clear_ask,
            commands::set_after_start,
            commands::set_show_game_start,
            commands::set_close_launcher,
            commands::set_show_ratings,
            commands::set_auto_ratings,
            commands::sync_ratings,
            commands::set_language,
            commands::set_autostart,
            #[cfg(debug_assertions)]
            commands::test_tray_menu,
            #[cfg(debug_assertions)]
            commands::test_process_id,
            #[cfg(debug_assertions)]
            commands::test_tray_click,
        ])
        .build(context)
        .expect("error while building Noxdeck")
        .run(|_, event| {
            if let tauri::RunEvent::ExitRequested {
                api, code: None, ..
            } = event
            {
                api.prevent_exit();
            }
        });
}
