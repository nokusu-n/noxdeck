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
use crate::diagnostics::{Command, Outcome, Step, WaitEnd, WaitFor};

impl Core {
    // ---- shared steps -------------------------------------------------------

    fn set_phase(&self, phase: Phase) {
        if let Some(op) = lock(&self.inner).operation.as_mut() {
            op.phase = phase;
        }
        self.journal.record(Step::Phase(phase));
        self.publish();
    }

    fn end_operation(&self) {
        self.finish_operation(false);
    }

    fn finish_operation(&self, success: bool) {
        let request = {
            let mut inner = lock(&self.inner);
            let op = inner.operation.take();
            if let Some(op) = &op {
                let cancelled = self.cancel.load(Ordering::SeqCst);
                if cancelled { if let Some(id) = &op.account_id { inner.outcomes.remove(id); } }
                let outcome = if cancelled { Outcome::Cancelled } else if success {
                    Outcome::Success
                }
                else if let Some((note, _)) = op.account_id.as_ref().and_then(|id| inner.outcomes.get(id)) {
                    match *note { "no_answer" => Outcome::NoAnswer, "game_needs_update" => Outcome::GameNeedsUpdate, "game_not_started" => Outcome::GameNotStarted, _ => Outcome::Failed }
                } else if let Some(account) = op.account_id.as_ref().and_then(|id| inner.registry.find(id)) {
                    match account.login_state { LoginState::PasswordWanted => Outcome::PasswordWanted, LoginState::CodeWanted => Outcome::CodeWanted, _ => Outcome::Failed }
                } else { Outcome::Failed };
                self.journal.record(Step::OperationEnded { kind: op.kind, account: op.account_id.as_ref().map(|id| self.journal.account(id)), outcome });
            }
            if op.is_some_and(|o| matches!(o.kind, OpKind::Start | OpKind::Game)) {
                use crate::shell::ShellRequest::*;
                if self.cancel.load(Ordering::SeqCst) {
                    None
                } else if !success {
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
        let result = config::write_saved_names(
            &self.paths.config_file(),
            &self.paths.data,
            install,
            &*self.processes,
            value,
        );
        self.journal.record(Step::ConfigWritten { success: result.is_ok() });
        result
    }

    /// Check refusals and acquire the one launcher-operation slot.
    fn begin(
        &self,
        kind: OpKind,
        account_id: Option<String>,
        check: impl FnOnce(&Inner) -> Result<(), Refusal>,
    ) -> Result<PathBuf, Refusal> {
        let account = account_id.as_ref().map(|id| self.journal.account(id));
        let result = self.acquire_operation(kind, account_id, check);
        match &result {
            Ok(_) => { self.journal.record(Step::OperationBegan { kind, account }); self.journal.record(Step::Phase(Phase::Stopping)); }
            Err(refusal) => self.journal.record(Step::OperationRefused { kind, refusal: *refusal }),
        }
        result
    }

    fn acquire_operation(
        &self, kind: OpKind, account_id: Option<String>,
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
                add_game: inner.settings.tab.game(),
                kind,
                account_id,
                phase: Phase::Stopping,
                previous_game_pids: if kind == OpKind::Game { inner.game_pids.clone() } else { vec![] },
                began: Instant::now(),
                hidden_started: false,
                game_appeared: false,
            });
            install
        };
        self.publish();
        Ok(install)
    }

    fn stop_for_restart(&self, install: &Path) -> Result<(), Refusal> {
        if let Err(error) = self.stop_launcher_journalled(install) {
            self.end_operation();
            return Err(match error { launcher::ProcessError::AccessDenied | launcher::ProcessError::System(5) => Refusal::LauncherNotClosable, _ => Refusal::LauncherCloseTimeout });
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
        if self.cancel.load(Ordering::SeqCst) { return false; }
        self.set_phase(Phase::Starting);
        self.journal.record(Step::Command(Command::SelectAccount));
        let written = matches!(
            self.write_list(install, email),
            Ok(()) | Err(ConfigError::KeyMissing)
        );
        let started = written && self.start_launcher_journalled(install).is_ok();
        if started {
            if let Some(op) = lock(&self.inner).operation.as_mut() {
                op.hidden_started = op.kind == OpKind::Game;
            }
        }
        started
    }

    fn start_launcher_journalled(&self, install: &Path) -> Result<(), launcher::ProcessError> {
        self.journal.record(Step::Command(Command::StartLauncher));
        let result = self.processes.start_launcher(install);
        diagnostics::command_result(&self.journal, Command::StartLauncher, &result);
        result
    }

    fn stop_launcher_journalled(&self, install: &Path) -> Result<(), launcher::ProcessError> {
        self.journal.record(Step::Command(Command::StopLauncher));
        let mut wait = self.journal.wait(WaitFor::LauncherStopped, Some(self.timing.stop));
        let started = Instant::now();
        let result = self.processes.stop_launcher(install, self.timing.stop);
        wait.end(if result.is_ok() { WaitEnd::Event } else if started.elapsed() >= self.timing.stop { WaitEnd::Bound } else { WaitEnd::Failed });
        diagnostics::command_result(&self.journal, Command::StopLauncher, &result);
        result
    }

    fn wait_login(&self, seq0: u64, wanted: Option<&str>) -> Option<LauncherEvent> {
        self.set_phase(Phase::WaitingLogin);
        let mut wait = self.journal.wait(WaitFor::Login, Some(self.timing.start_timeout));
        let deadline = Instant::now() + self.timing.start_timeout;
        let mut gone_since: Option<Instant> = None;
        loop {
            self.tick();
            if self.cancel.load(Ordering::SeqCst) {
                wait.end(WaitEnd::Cancelled);
                return None;
            }
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
                wait.end(WaitEnd::Event);
                return outcome;
            }
            if lock(&self.inner).launcher_running {
                gone_since = None;
            } else if gone_since.get_or_insert_with(Instant::now).elapsed()
                >= self.timing.gone_grace
            {
                wait.end(WaitEnd::Gone);
                return None;
            }
            if Instant::now() >= deadline {
                wait.end(WaitEnd::Bound);
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
        if self.cancel.load(Ordering::SeqCst) {
            self.finish_operation(false);
            return;
        }
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
            email = a.email.clone().ok_or(Refusal::NoEmail)?;
            game_running = inner.game_running;
            if a.game != crate::registry::Game::Overwatch { return Err(Refusal::GameUnsupported); }
            if game_running && !close_game {
                return Err(Refusal::GameRunning);
            }
            reuse = inner.launcher_running
                && matches!(&inner.session, Session::LoggedIn(id) if id == &target);
            Ok(())
        })?;
        if game_running {
            self.journal.record(Step::Command(Command::StopGame));
            let mut wait = self.journal.wait(WaitFor::GameStopped, Some(Duration::from_secs(10)));
            let result = self.processes.stop_game(&self.paths.game_exe);
            wait.end(if result.is_ok() { WaitEnd::Event } else { WaitEnd::Failed });
            diagnostics::command_result(&self.journal, Command::StopGame, &result);
            if result.is_err() {
                self.end_operation();
                return Err(Refusal::GameNotClosable);
            }
            self.tick();
            let inner = lock(&self.inner);
            if inner.game_running {
                drop(inner);
                self.end_operation();
                return Err(Refusal::GameNotClosable);
            }
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
        if self.cancel.load(Ordering::SeqCst) {
            self.tick();
            let stop = {
                let inner = lock(&self.inner);
                inner.operation.as_ref().is_some_and(|op| op.hidden_started
                    && !inner.game_pids.iter().any(|pid| !op.previous_game_pids.contains(pid)))
            };
            if stop { let _ = self.stop_launcher_journalled(&install); self.tick(); }
            let hidden_started = lock(&self.inner).operation.as_ref().is_some_and(|op| op.hidden_started);
            if hidden_started && self.processes.launcher_running(&install) {
                self.processes.show_windows_no_activate(&install);
            }
            self.finish_operation(false);
            return;
        }
        if outcome != Some("game_started") {
            self.processes.show_windows(&install);
        } else if !reuse && self.processes.launcher_running(&install) {
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
        if self.cancel.load(Ordering::SeqCst) { return None; }
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
        let launch_seq = self.seq();
        self.journal.record(Step::Command(Command::LaunchGame));
        let result = self.processes.send_launch_command(install);
        diagnostics::command_result(&self.journal, Command::LaunchGame, &result);
        if result.is_err() {
            return Some("game_not_started");
        }
        let mut resent = false;
        // A manual debug probe sends the second command while the first is being handled.
        #[cfg(debug_assertions)]
        if std::env::var("NOXDECK_PROBE_RESEND").as_deref() == Ok("1") {
            std::thread::sleep(Duration::from_millis(250));
            self.tick();
            let wanted = {
                let inner = lock(&self.inner);
                inner.launcher_running && matches!(&inner.session, Session::LoggedIn(current) if current == id)
            };
            if wanted && !self.cancel.load(Ordering::SeqCst) {
                resent = true;
                self.journal.record(Step::Command(Command::LaunchGame));
                let result = self.processes.send_launch_command(install);
                diagnostics::command_result(&self.journal, Command::LaunchGame, &result);
            }
        }
        self.processes.refresh_launcher_pids(install);
        if !reuse {
            self.processes.hide_windows(install);
        }
        let deadline = Instant::now() + self.timing.game_timeout;
        let retry_at = Instant::now() + self.timing.game_retry;
        let mut launch_seen = false;
        let mut gone_since: Option<Instant> = None;
        let mut wait = self.journal.wait(WaitFor::GameProcess, Some(self.timing.game_timeout));
        let previous = lock(&self.inner).operation.as_ref().map(|op| op.previous_game_pids.clone()).unwrap_or_default();
        let game_pid = loop {
            self.tick();
            if self.cancel.load(Ordering::SeqCst) {
                wait.end(WaitEnd::Cancelled);
                return None;
            }
            if let Some(pid) = lock(&self.inner).game_pids.iter().copied().find(|pid| !previous.contains(pid)) {
                wait.end(WaitEnd::Event);
                break pid;
            }
            launch_seen |= self.events_since(launch_seq).iter().any(|event| matches!(event, LauncherEvent::GameLaunchStarted | LauncherEvent::GameLaunched));
            let (running, wanted) = {
                let inner = lock(&self.inner);
                (inner.launcher_running, matches!(&inner.session, Session::LoggedIn(current) if current == id))
            };
            if running { gone_since = None; }
            else if gone_since.get_or_insert_with(Instant::now).elapsed() >= self.timing.gone_grace {
                wait.end(WaitEnd::Gone);
                return Some("game_not_started");
            }
            if !resent && !launch_seen && running && wanted && Instant::now() >= retry_at {
                resent = true;
                self.journal.record(Step::Command(Command::LaunchGame));
                let result = self.processes.send_launch_command(install);
                diagnostics::command_result(&self.journal, Command::LaunchGame, &result);
                self.processes.refresh_launcher_pids(install);
                if result.is_err() { wait.end(WaitEnd::Failed); return Some("game_not_started"); }
            }
            if Instant::now() >= deadline {
                wait.end(WaitEnd::Bound);
                return Some("game_not_started");
            }
            self.pause();
        };
        drop(wait);
        if close_launcher {
            self.set_phase(Phase::ClosingLauncher);
            if self.wait_game_handover(game_pid)
                && lock(&self.inner).launcher_running
                && self.stop_launcher_journalled(install).is_err()
            {
                return Some("no_answer");
            }
        } else {
            let deadline = Instant::now() + self.timing.game_settle;
            let mut wait = self.journal.wait(WaitFor::GameSettle, Some(self.timing.game_settle));
            while Instant::now() < deadline {
                if self.cancel.load(Ordering::SeqCst) { wait.end(WaitEnd::Cancelled); return None; }
                self.pause();
                self.tick();
            }
            wait.end(WaitEnd::Elapsed);
        }
        self.tick();
        Some("game_started")
    }

    /// Rule B: a visible window plus a pause is a conservative handover estimate.
    fn wait_game_handover(&self, pid: u32) -> bool {
        let deadline = Instant::now() + self.timing.game_handover;
        let mut wait = self.journal.wait(WaitFor::GameHandover, Some(self.timing.game_handover));
        loop {
            self.tick();
            if self.cancel.load(Ordering::SeqCst) { wait.end(WaitEnd::Cancelled); return false; }
            let inner = lock(&self.inner);
            if !inner.launcher_running { wait.end(WaitEnd::Event); return false; }
            if !inner.game_pids.contains(&pid) { wait.end(WaitEnd::Gone); return false; }
            if Instant::now() >= deadline { wait.end(WaitEnd::Bound); return false; }
            if inner.game_window_pids.contains(&pid) { wait.end(WaitEnd::Event); break; }
            drop(inner);
            self.pause();
        }
        drop(wait);
        let pause_until = Instant::now() + self.timing.handover_pause;
        let bound = self.timing.handover_pause.min(deadline.saturating_duration_since(Instant::now()));
        let mut wait = self.journal.wait(WaitFor::HandoverPause, Some(bound));
        loop {
            self.tick();
            if self.cancel.load(Ordering::SeqCst) { wait.end(WaitEnd::Cancelled); return false; }
            let inner = lock(&self.inner);
            if !inner.launcher_running { wait.end(WaitEnd::Event); return false; }
            if !inner.game_pids.contains(&pid) { wait.end(WaitEnd::Gone); return false; }
            if Instant::now() >= deadline { wait.end(WaitEnd::Bound); return false; }
            if Instant::now() >= pause_until { wait.end(WaitEnd::Elapsed); return true; }
            drop(inner);
            self.pause();
        }
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
        self.journal.record(Step::Command(Command::ClearAccountList));
        let ready = match (&previous, self.write_list(&install, "")) {
            (_, Ok(())) | (None, Err(ConfigError::KeyMissing)) => true,
            _ => false,
        };
        if !ready || self.start_launcher_journalled(&install).is_err() {
            self.restore(&install, &previous);
            self.end_operation();
            return;
        }
        self.set_phase(Phase::WaitingLogin);
        let mut wait = self.journal.wait(WaitFor::AddLogin, None);
        let mut gone_since: Option<Instant> = None;
        let logged_in = loop {
            self.tick();
            if self.cancel.load(Ordering::SeqCst) {
                wait.end(WaitEnd::Cancelled);
                break None;
            }
            let login = self.events_since(seq0).into_iter().find_map(|e| match e {
                LauncherEvent::LoggedIn { id } => Some(id),
                _ => None,
            });
            if login.is_some() {
                wait.end(WaitEnd::Event);
                break login;
            }
            if lock(&self.inner).launcher_running {
                gone_since = None;
            } else if gone_since.get_or_insert_with(Instant::now).elapsed()
                >= self.timing.gone_grace
            {
                wait.end(WaitEnd::Gone);
                break None;
            }
            self.pause();
        };
        drop(wait);
        self.set_phase(Phase::Finishing);
        let success = logged_in.is_some();
        match logged_in {
            Some(id) => self.finish_add(&id),
            None => self.restore(&install, &previous),
        }
        self.finish_operation(success);
    }

    /// Cancelled, or the launcher closed unused: stop it if it still runs, then put
    /// back the saved list as it was. The launcher rewrites its config on exit, so
    /// this comes after the exit, never before.
    fn restore(&self, install: &Path, previous: &Option<String>) {
        if self
            .stop_launcher_journalled(install)
            .is_ok()
        {
            if let Some(list) = previous {
                self.journal.record(Step::Command(Command::RestoreAccountList));
                let _ = self.write_list(install, list);
            }
        }
    }

    /// The launcher logged in: find the e-mail and the BattleTag, then create or
    /// refresh the record.
    fn finish_add(&self, id: &str) {
        let start = Instant::now();
        let (mut email, mut tag) = (None, None);
        let mut email_wait = Some(self.journal.wait(WaitFor::Email, Some(self.timing.email_wait)));
        let mut tag_wait = Some(self.journal.wait(WaitFor::BattleTag, Some(self.timing.tag_wait)));
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
            if email_done { if let Some(mut wait) = email_wait.take() { wait.end(if email.is_some() { WaitEnd::Event } else { WaitEnd::Bound }); } }
            if tag_done { if let Some(mut wait) = tag_wait.take() { wait.end(if tag.is_some() { WaitEnd::Event } else { WaitEnd::Bound }); } }
            if email_done && tag_done {
                break;
            }
            self.pause();
        }
        let mut inner = lock(&self.inner);
        let game = inner.operation.as_ref().map_or(crate::registry::Game::None, |op| op.add_game);
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
                game,
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
        self.tick();
        let inner = lock(&self.inner);
        let cancellable = inner
            .operation
            .as_ref()
            .is_some_and(|o| o.cancellable()
                && (o.kind == OpKind::Add || o.began.elapsed() >= self.timing.cancel_guard));
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
        let game = inner.settings.tab.game();
        inner.registry.accounts.push(Account {
            game,
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

    pub fn set_account_game(&self, id: &str, game: crate::registry::Game) -> Result<(), Refusal> {
        self.tick();
        let mut inner = lock(&self.inner);
        if inner.operation.as_ref().is_some_and(|op| op.account_id.as_deref() == Some(id))
            || inner.game.confirmed.as_ref().is_some_and(|playing| playing.account_id == id) {
            return Err(Refusal::GameChangeBlocked);
        }
        let account = inner.registry.find_mut(id).ok_or(Refusal::UnknownAccount)?;
        if account.game == game { return Ok(()); }
        account.game = game;
        self.journal.record(Step::AccountGameChanged { account: self.journal.account(id), game });
        if game == crate::registry::Game::Overwatch { self.refresh_added_account(&mut inner, id); }
        self.save_registry(&inner);
        drop(inner);
        self.publish();
        Ok(())
    }

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
        if sort.role().is_some() && inner.settings.tab != crate::settings::Tab::Overwatch { return; }
        let tab = inner.settings.tab;
        inner.settings.tab_sorts.insert(tab, sort);
        inner.settings.sort = sort;
        self.save_settings(&inner);
        drop(inner);
        self.publish();
    }

    pub fn set_tab(&self, tab: crate::settings::Tab) {
        let mut inner = lock(&self.inner);
        inner.settings.tab = tab;
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
