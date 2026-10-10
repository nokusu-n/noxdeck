//! The user's actions: add an account, save the current one, start the launcher on
//! one, start its game, plus the small list edits. One launcher operation runs at a time.

use crate::launcher::config::{self, ConfigError};
use crate::launcher::{cache, log, LauncherEvent};
use crate::registry::{now_iso, Account, LoginState};
use crate::settings::Sort;
use crate::*;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

impl Core {
    // ---- shared steps -------------------------------------------------------

    fn set_phase(&self, phase: Phase) {
        if let Some(op) = lock(&self.inner).operation.as_mut() {
            op.phase = phase;
        }
        self.publish();
    }

    fn end_operation(&self) {
        self.finish_operation(false);
    }

    fn finish_operation(&self, success: bool) {
        let request = {
            let mut inner = lock(&self.inner);
            let op = inner.operation.take();
            if op.is_some_and(|o| matches!(o.kind, OpKind::Start | OpKind::Game)) {
                use crate::shell::ShellRequest::*;
                if !success {
                    Some(ShowWindow)
                } else {
                    match inner.settings.after_start {
                        AfterStart::Stay => None,
                        AfterStart::Tray => Some(HideWindow),
                        AfterStart::Close => Some(Exit),
                    }
                }
            } else {
                None
            }
        };
        self.publish();
        if let Some(request) = request {
            self.shell.request(request);
        }
    }

    fn seq(&self) -> u64 {
        lock(&self.inner).seq
    }

    fn events_since(&self, seq: u64) -> Vec<LauncherEvent> {
        let inner = lock(&self.inner);
        inner
            .events
            .iter()
            .filter(|(n, _)| *n > seq)
            .map(|(_, e)| e.clone())
            .collect()
    }

    fn pause(&self) {
        std::thread::sleep(self.timing.poll);
    }

    fn write_list(&self, install: &Path, value: &str) -> Result<(), ConfigError> {
        config::write_saved_names(
            &self.paths.config_file(),
            &self.paths.data,
            install,
            &*self.processes,
            value,
        )
    }

    /// Check refusals and acquire the one launcher-operation slot.
    fn begin(
        &self,
        kind: OpKind,
        account_id: Option<String>,
        check: impl FnOnce(&Inner) -> Result<(), Refusal>,
    ) -> Result<PathBuf, Refusal> {
        self.tick();
        let install = {
            let mut inner = lock(&self.inner);
            if inner.operation.is_some() {
                return Err(Refusal::Busy);
            }
            let install = inner.install.clone().ok_or(Refusal::LauncherNotFound)?;
            check(&inner)?;
            if let Some(id) = &account_id {
                inner.outcomes.remove(id);
            }
            self.cancel.store(false, Ordering::SeqCst);
            inner.ask = None;
            inner.operation = Some(Operation {
                kind,
                account_id,
                phase: Phase::Stopping,
            });
            install
        };
        self.publish();
        Ok(install)
    }

    fn stop_for_restart(&self, install: &Path) -> Result<(), Refusal> {
        if self
            .processes
            .stop_launcher(&install, self.timing.stop)
            .is_err()
        {
            self.end_operation();
            return Err(Refusal::LauncherNotClosable);
        }
        // Drain what the stopped launcher wrote last, so it is not taken for news.
        self.tick();
        Ok(())
    }

    fn prepare(
        &self,
        kind: OpKind,
        account_id: Option<String>,
        check: impl FnOnce(&Inner) -> Result<(), Refusal>,
    ) -> Result<PathBuf, Refusal> {
        let install = self.begin(kind, account_id, check)?;
        self.stop_for_restart(&install)?;
        Ok(install)
    }

    fn start_selected_launcher(&self, install: &Path, email: &str) -> bool {
        self.set_phase(Phase::Starting);
        let written = matches!(
            self.write_list(install, email),
            Ok(()) | Err(ConfigError::KeyMissing)
        );
        written && self.processes.start_launcher(install).is_ok()
    }

    fn wait_login(&self, seq0: u64, wanted: Option<&str>) -> Option<LauncherEvent> {
        self.set_phase(Phase::WaitingLogin);
        let deadline = Instant::now() + self.timing.start_timeout;
        let mut gone_since: Option<Instant> = None;
        loop {
            self.tick();
            let outcome = self.events_since(seq0).into_iter().find(|e| match e {
                LauncherEvent::LoggedIn { id } => {
                    let inner = lock(&self.inner);
                    wanted.is_none_or(|wanted| wanted == id)
                        && inner.launcher_running
                        && matches!(&inner.session, Session::LoggedIn(current) if current == id)
                }
                LauncherEvent::LoginScreen | LauncherEvent::CodeScreen => true,
                _ => false,
            });
            if outcome.is_some() {
                return outcome;
            }
            if lock(&self.inner).launcher_running {
                gone_since = None;
            } else if gone_since.get_or_insert_with(Instant::now).elapsed()
                >= self.timing.gone_grace
            {
                return None;
            }
            if Instant::now() >= deadline {
                return None;
            }
            self.pause();
        }
    }

    // ---- start launcher -----------------------------------------------------

    pub fn start_launcher(self: &Arc<Self>, id: &str) -> Result<(), Refusal> {
        let mut email = String::new();
        let target = id.to_owned();
        let install = self.prepare(OpKind::Start, Some(target.clone()), |inner| {
            let a = inner
                .registry
                .find(&target)
                .ok_or(Refusal::UnknownAccount)?;
            if inner.launcher_running
                && matches!(&inner.session, Session::LoggedIn(id) if id == &target)
            {
                return Err(Refusal::AlreadyOpen);
            }
            email = a.email.clone().ok_or(Refusal::NoEmail)?;
            Ok(())
        })?;
        let core = Arc::clone(self);
        let id = id.to_owned();
        std::thread::spawn(move || core.run_start(id, email, install));
        Ok(())
    }

    fn run_start(&self, id: String, email: String, install: PathBuf) {
        let seq0 = self.seq();
        let outcome = self
            .start_selected_launcher(&install, &email)
            .then(|| self.wait_login(seq0, None))
            .flatten();
        let success =
            matches!(&outcome, Some(LauncherEvent::LoggedIn { id: logged }) if logged == &id);
        self.set_phase(Phase::Finishing);
        {
            let mut inner = lock(&self.inner);
            match outcome {
                // The watcher already set the record to "ready" for a login.
                Some(LauncherEvent::LoggedIn { .. }) => {}
                Some(LauncherEvent::LoginScreen) => {
                    set_state(&mut inner, &id, LoginState::PasswordWanted)
                }
                Some(LauncherEvent::CodeScreen) => {
                    set_state(&mut inner, &id, LoginState::CodeWanted)
                }
                _ => {
                    inner.outcomes.insert(id, ("no_answer", Instant::now()));
                }
            }
            self.save_registry(&inner);
        }
        self.finish_operation(success);
    }

    // ---- start game ---------------------------------------------------------

    pub fn start_game(self: &Arc<Self>, id: &str, close_game: bool) -> Result<(), Refusal> {
        let target = id.to_owned();
        let (mut email, mut reuse, mut game_running) = (String::new(), false, false);
        let install = self.begin(OpKind::Game, Some(target.clone()), |inner| {
            let a = inner
                .registry
                .find(&target)
                .ok_or(Refusal::UnknownAccount)?;
            if inner
                .game
                .confirmed
                .as_ref()
                .is_some_and(|g| g.account_id == target)
            {
                return Err(Refusal::AlreadyInGame);
            }
            email = a.email.clone().ok_or(Refusal::NoEmail)?;
            game_running = inner.game_running;
            if game_running && !close_game {
                return Err(Refusal::GameRunning);
            }
            reuse = inner.launcher_running
                && matches!(&inner.session, Session::LoggedIn(id) if id == &target);
            Ok(())
        })?;
        if game_running {
            if self.processes.stop_game(&self.paths.game_exe).is_err() {
                self.end_operation();
                return Err(Refusal::GameNotClosable);
            }
            self.tick();
            let inner = lock(&self.inner);
            // Stopping the game can take time; the launcher may have changed meanwhile.
            reuse = inner.launcher_running
                && matches!(&inner.session, Session::LoggedIn(id) if id == &target);
        }
        if !reuse {
            self.stop_for_restart(&install)?;
        }
        let core = Arc::clone(self);
        std::thread::spawn(move || core.run_game(target, email, install, reuse));
        Ok(())
    }

    fn run_game(&self, id: String, email: String, install: PathBuf, reuse: bool) {
        let close_launcher = lock(&self.inner).settings.close_launcher;
        let hiding = AtomicBool::new(!reuse);
        let refresh_install = &install;
        let outcome = std::thread::scope(|scope| {
            let (stop_refresh, refresh_wait) = std::sync::mpsc::channel::<()>();
            if !reuse {
                self.processes.hide_windows(&install);
                scope.spawn(|| {
                    while hiding.load(Ordering::SeqCst) {
                        self.processes.hide_windows(&install);
                        std::thread::sleep(self.timing.hide_interval);
                    }
                });
                scope.spawn(move || {
                    while matches!(
                        refresh_wait.recv_timeout(self.timing.pid_refresh),
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout)
                    ) {
                        self.processes.refresh_launcher_pids(refresh_install);
                    }
                });
            }
            let outcome = self.launch_game(&id, &email, &install, reuse, close_launcher);
            hiding.store(false, Ordering::SeqCst);
            let _ = stop_refresh.send(());
            outcome
        });
        // Joining the hiding loop above prevents it from hiding a revealed window.
        if outcome != Some("game_started") {
            self.processes.show_windows(&install);
        } else if !close_launcher && !reuse && self.processes.launcher_running(&install) {
            self.processes.show_windows_no_activate(&install);
        }
        self.set_phase(Phase::Finishing);
        {
            let mut inner = lock(&self.inner);
            if let Some(outcome) = outcome {
                inner.outcomes.insert(id, (outcome, Instant::now()));
            }
            self.save_registry(&inner);
        }
        self.finish_operation(outcome == Some("game_started"));
    }

    fn launch_game(
        &self,
        id: &str,
        email: &str,
        install: &Path,
        reuse: bool,
        close_launcher: bool,
    ) -> Option<&'static str> {
        if !reuse {
            let seq0 = self.seq();
            if !self.start_selected_launcher(install, email) {
                return Some("no_answer");
            }
            self.processes.refresh_launcher_pids(install);
            self.processes.hide_windows(install);
            match self.wait_login(seq0, Some(id)) {
                Some(LauncherEvent::LoggedIn { .. }) => {}
                Some(LauncherEvent::LoginScreen) => {
                    set_state(&mut lock(&self.inner), id, LoginState::PasswordWanted);
                    return None;
                }
                Some(LauncherEvent::CodeScreen) => {
                    set_state(&mut lock(&self.inner), id, LoginState::CodeWanted);
                    return None;
                }
                _ => return Some("no_answer"),
            }
        }
        self.tick();
        {
            let inner = lock(&self.inner);
            if !inner.launcher_running
                || !matches!(&inner.session, Session::LoggedIn(current) if current == id)
            {
                return Some("no_answer");
            }
        }
        if lock(&self.inner).game_readiness == GameReadiness::NotPlayable {
            return Some("game_needs_update");
        }
        self.set_phase(Phase::LaunchingGame);
        if self.processes.send_launch_command(install).is_err() {
            return Some("game_not_started");
        }
        self.processes.refresh_launcher_pids(install);
        if !reuse {
            self.processes.hide_windows(install);
        }
        let deadline = Instant::now() + self.timing.game_timeout;
        loop {
            self.tick();
            if lock(&self.inner).game_running {
                break;
            }
            if Instant::now() >= deadline {
                return Some("game_not_started");
            }
            self.pause();
        }
        if close_launcher {
            self.set_phase(Phase::ClosingLauncher);
        }
        let deadline = Instant::now() + self.timing.game_settle;
        while Instant::now() < deadline {
            self.pause();
            self.tick();
        }
        if close_launcher && lock(&self.inner).launcher_running
            && self
                .processes
                .stop_launcher(install, self.timing.stop)
                .is_err()
        {
            return Some("no_answer");
        }
        self.tick();
        Some("game_started")
    }

    // ---- add account --------------------------------------------------------

    pub fn add_account(self: &Arc<Self>) -> Result<(), Refusal> {
        let install = self.prepare(OpKind::Add, None, |_| Ok(()))?;
        let previous = config::read_saved_names(&self.paths.config_file());
        let core = Arc::clone(self);
        std::thread::spawn(move || core.run_add(previous, install));
        Ok(())
    }

    fn run_add(&self, previous: Option<String>, install: PathBuf) {
        self.set_phase(Phase::Starting);
        let seq0 = self.seq();
        let ready = match (&previous, self.write_list(&install, "")) {
            (_, Ok(())) | (None, Err(ConfigError::KeyMissing)) => true,
            _ => false,
        };
        if !ready || self.processes.start_launcher(&install).is_err() {
            self.restore(&install, &previous);
            self.end_operation();
            return;
        }
        self.set_phase(Phase::WaitingLogin);
        let mut gone_since: Option<Instant> = None;
        let logged_in = loop {
            self.tick();
            if self.cancel.load(Ordering::SeqCst) {
                break None;
            }
            let login = self.events_since(seq0).into_iter().find_map(|e| match e {
                LauncherEvent::LoggedIn { id } => Some(id),
                _ => None,
            });
            if login.is_some() {
                break login;
            }
            if lock(&self.inner).launcher_running {
                gone_since = None;
            } else if gone_since.get_or_insert_with(Instant::now).elapsed()
                >= self.timing.gone_grace
            {
                break None;
            }
            self.pause();
        };
        self.set_phase(Phase::Finishing);
        match logged_in {
            Some(id) => self.finish_add(&id),
            None => self.restore(&install, &previous),
        }
        self.end_operation();
    }

    /// Cancelled, or the launcher closed unused: stop it if it still runs, then put
    /// back the saved list as it was. The launcher rewrites its config on exit, so
    /// this comes after the exit, never before.
    fn restore(&self, install: &Path, previous: &Option<String>) {
        if self
            .processes
            .stop_launcher(install, self.timing.stop)
            .is_ok()
        {
            if let Some(list) = previous {
                let _ = self.write_list(install, list);
            }
        }
    }

    /// The launcher logged in: find the e-mail and the BattleTag, then create or
    /// refresh the record.
    fn finish_add(&self, id: &str) {
        let start = Instant::now();
        let (mut email, mut tag) = (None, None);
        loop {
            let elapsed = start.elapsed();
            if email.is_none() && elapsed < self.timing.email_wait {
                email = config::first_saved_name(&self.paths.config_file());
            }
            if tag.is_none() && elapsed < self.timing.tag_wait {
                tag = cache::battle_tag(&self.paths.cache_db(), id);
            }
            let email_done = email.is_some() || elapsed >= self.timing.email_wait;
            let tag_done = tag.is_some() || elapsed >= self.timing.tag_wait;
            if email_done && tag_done {
                break;
            }
            self.pause();
        }
        let mut inner = lock(&self.inner);
        let now = Some(now_iso());
        let added = inner.registry.find(id).is_none();
        match inner.registry.find_mut(id) {
            Some(a) => {
                a.email = email.or(a.email.take());
                a.battletag = tag.or(a.battletag.take());
                a.login_state = LoginState::Ready;
                a.last_login_at = now;
            }
            None => inner.registry.accounts.push(Account {
                ratings: Default::default(),
                id: id.to_owned(),
                email,
                battletag: tag,
                login_state: LoginState::Ready,
                last_login_at: now,
            }),
        }
        self.save_registry(&inner);
        if added { self.refresh_added_account(&mut inner, id); }
    }

    pub fn cancel_operation(&self) {
        let cancellable = lock(&self.inner)
            .operation
            .as_ref()
            .is_some_and(|o| o.kind == OpKind::Add && o.phase == Phase::WaitingLogin);
        if cancellable {
            self.cancel.store(true, Ordering::SeqCst);
        }
    }

    // ---- save current -------------------------------------------------------

    pub fn save_current(&self) -> Result<String, Refusal> {
        self.tick();
        let mut inner = lock(&self.inner);
        if inner.operation.is_some() {
            return Err(Refusal::Busy);
        }
        if inner.install.is_none() {
            return Err(Refusal::LauncherNotFound);
        }
        let id = if inner.launcher_running {
            match &inner.session {
                Session::LoggedIn(id) => id.clone(),
                Session::LoginScreen => return Err(Refusal::NoSavedLogin),
            }
        } else {
            log::last_session(&self.paths.logs_dir()).ok_or(Refusal::NoSavedLogin)?
        };
        if inner.registry.find(&id).is_some() {
            return Err(Refusal::AlreadyListed);
        }
        inner.registry.accounts.push(Account {
            ratings: Default::default(),
            email: config::first_saved_name(&self.paths.config_file()),
            battletag: cache::battle_tag(&self.paths.cache_db(), &id),
            id: id.clone(),
            login_state: LoginState::Ready,
            last_login_at: None,
        });
        self.save_registry(&inner);
        self.refresh_added_account(&mut inner, &id);
        drop(inner);
        self.publish();
        Ok(id)
    }

    // ---- the list -----------------------------------------------------------

    pub fn delete_account(&self, id: &str) -> Result<(), Refusal> {
        let mut inner = lock(&self.inner);
        if inner.operation.is_some() {
            return Err(Refusal::Busy);
        }
        let before = inner.registry.accounts.len();
        inner.registry.accounts.retain(|a| a.id != id);
        if inner.registry.accounts.len() == before {
            return Err(Refusal::UnknownAccount);
        }
        inner.outcomes.remove(id);
        let Inner { game, registry, .. } = &mut *inner;
        game.retain_listed(registry);
        self.save_game(&inner);
        self.save_registry(&inner);
        drop(inner);
        self.publish();
        Ok(())
    }

    pub fn set_sort(&self, sort: Sort) {
        let mut inner = lock(&self.inner);
        inner.settings.sort = sort;
        self.save_settings(&inner);
        drop(inner);
        self.publish();
    }

    pub fn dismiss(&self, what: &str) {
        let mut inner = lock(&self.inner);
        match what {
            "rememberLogin" => inner.settings.dismissed.remember_login = true,
            "logoutWarning" => inner.settings.dismissed.logout_warning = true,
            _ => return,
        }
        self.save_settings(&inner);
        drop(inner);
        self.publish();
    }

    pub fn acknowledge_notice(&self) {
        lock(&self.inner).notice = false;
        self.publish();
    }
}

fn set_state(inner: &mut Inner, id: &str, state: LoginState) {
    if let Some(a) = inner.registry.find_mut(id) {
        a.login_state = state;
    }
}
