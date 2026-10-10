//! The watcher: every poll derive the launcher state from the config, the process
//! list and the log, apply logins and log-outs to listed records, publish the state.
//! It never creates a record (FR-002).

use crate::launcher::{cache, config, LauncherEvent};
use crate::registry::{now_iso, LoginState};
use crate::settings::Sort;
use crate::*;
use std::time::Instant;

impl Core {
    /// One watcher step. Safe to call from anywhere; steps never overlap.
    pub fn tick(&self) {
        let mut tail = lock(&self.tail);
        let autostart = self.shell.autostart();
        let install = config::find_install(&self.paths.config_file());
        let snapshot = self
            .processes
            .snapshot(install.as_deref(), &self.paths.game_exe);
        self.journal.snapshot(&snapshot);
        let now = Instant::now();
        let polled = tail.poll(&self.paths.logs_dir(), now);
        {
            let mut inner = lock(&self.inner);
            inner.outcomes.retain(|_, (note, finished)| {
                *note != "game_started" || now.saturating_duration_since(*finished) < Duration::from_secs(3)
            });
            if let Ok(autostart) = autostart {
                inner.autostart = autostart;
                self.journal.settings(&inner.settings, autostart);
            }
            inner.install = install;
            inner.launcher_running = snapshot.launcher_running;
            inner.game_running = snapshot.game_running;
            inner.game_pids = snapshot.game_pids.clone();
            inner.game_window_pids = snapshot.game_window_pids.clone();
            let was_in_game = inner.game.confirmed.clone();
            if polled.new_file {
                inner.session = Session::LoginScreen;
                inner.game_readiness = GameReadiness::Unknown;
            }
            let mut dirty = false;
            for t in polled.events {
                if t.event == LauncherEvent::GameLaunched {
                    // A saved PID (including a cleared pair) is stronger evidence than old logs.
                    if !t.historic || (!inner.game.restored && !snapshot.game_pids.is_empty()) {
                        let id = match &inner.session {
                            Session::LoggedIn(id) if inner.registry.find(id).is_some() => {
                                Some(id.clone())
                            }
                            _ => None,
                        };
                        let previous = inner.operation.as_ref().filter(|op| op.kind == OpKind::Game)
                            .map(|op| op.previous_game_pids.clone()).unwrap_or_default();
                        inner.game.launched(id.as_deref(), now, &previous);
                    }
                }
                dirty |= self.apply(&mut inner, t.event, t.historic);
            }
            inner.game.restored = false;
            inner.game.tick(now, &snapshot.game_pids);
            let Inner { game, registry, .. } = &mut *inner;
            game.retain_listed(registry);
            if was_in_game != inner.game.confirmed {
                self.save_game(&inner);
            }
            // Drain final log events, then forget the stopped process's session.
            if !snapshot.launcher_running {
                inner.session = Session::LoginScreen;
                inner.game_readiness = GameReadiness::Unknown;
            }
            // A late config/cache write must not leave a listed account incomplete.
            if snapshot.launcher_running {
                let current = match &inner.session { Session::LoggedIn(id) => Some(id.clone()), _ => None };
                for account in &mut inner.registry.accounts {
                    if account.email.is_none() && current.as_deref() == Some(&account.id) {
                        if let Some(email) = config::first_saved_name(&self.paths.config_file()) {
                            account.email = Some(email);
                            dirty = true;
                        }
                    }
                    if account.battletag.is_none() {
                        if let Some(tag) = cache::battle_tag(&self.paths.cache_db(), &account.id) {
                            account.battletag = Some(tag);
                            dirty = true;
                        }
                    }
                }
            }
            if dirty {
                self.save_registry(&inner);
            }
        }
        drop(tail);
        self.publish();
    }

    /// Returns whether a record changed.
    fn apply(&self, inner: &mut Inner, event: LauncherEvent, historic: bool) -> bool {
        self.journal.record(diagnostics::Step::LauncherEvent {
            event: diagnostics::Event::from_launcher(&event),
            account: if let LauncherEvent::LoggedIn { id } = &event { Some(self.journal.account(id)) } else { None },
            historic,
        });
        inner.seq += 1;
        let seq = inner.seq;
        inner.events.push((seq, event.clone()));
        if inner.events.len() > 64 {
            inner.events.remove(0);
        }
        match event {
            LauncherEvent::GameLaunched => false,
            LauncherEvent::GameReadiness(r) => {
                inner.game_readiness = r;
                false
            }
            LauncherEvent::LoggedIn { id } => {
                inner.session = Session::LoggedIn(id.clone());
                if inner.outcomes.get(&id).is_some_and(|(note, _)| *note == "no_answer") {
                    inner.outcomes.remove(&id);
                }
                let tag = if inner.launcher_running { cache::battle_tag(&self.paths.cache_db(), &id) } else { None };
                let Some(a) = inner.registry.find_mut(&id) else {
                    return false;
                };
                a.login_state = LoginState::Ready;
                if !historic {
                    a.last_login_at = Some(now_iso());
                }
                if tag.is_some() {
                    a.battletag = tag;
                }
                true
            }
            LauncherEvent::LoggedOut => {
                let was = std::mem::replace(&mut inner.session, Session::LoginScreen);
                let Session::LoggedIn(id) = was else {
                    return false;
                };
                let Some(a) = inner.registry.find_mut(&id) else {
                    return false;
                };
                a.login_state = LoginState::PasswordWanted;
                true
            }
            LauncherEvent::LoginScreen | LauncherEvent::CodeScreen => {
                inner.session = Session::LoginScreen;
                false
            }
        }
    }

    pub fn state(&self) -> State {
        let inner = lock(&self.inner);
        let current = match (&inner.install, inner.launcher_running, &inner.session) {
            (Some(_), true, Session::LoggedIn(id)) => Some(id.clone()),
            _ => None,
        };
        let launcher = match (&inner.install, inner.launcher_running) {
            (None, _) => LauncherKind::NotFound,
            (Some(_), false) => LauncherKind::Closed,
            (Some(_), true) if current.is_some() => LauncherKind::LoggedIn,
            (Some(_), true) => LauncherKind::LoginScreen,
        };
        let mut accounts: Vec<AccountView> = inner
            .registry
            .accounts
            .iter()
            .map(|a| AccountView {
                game: a.game,
                ratings: a.ratings.clone(),
                id: a.id.clone(),
                battletag: a.battletag.clone(),
                email: a.email.as_deref().map(|email| if inner.settings.hide_emails { settings::mask_email(email) } else { email.to_owned() }),
                login_state: a.login_state,
                last_login_at: a.last_login_at.clone(),
                is_current: current.as_deref() == Some(a.id.as_str()),
                is_in_game: inner
                    .game
                    .confirmed
                    .as_ref()
                    .is_some_and(|g| g.account_id == a.id),
                last_outcome: inner.outcomes.get(&a.id).map(|(note, _)| *note),
            })
            .collect();
        let sort = inner.settings.current_sort();
        match sort {
            // ISO times compare as text; most recent first, `None` last.
            Sort::LastLogin => {
                accounts.sort_by(|a, b| match (&a.last_login_at, &b.last_login_at) {
                    (Some(x), Some(y)) => y.cmp(x),
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => std::cmp::Ordering::Equal,
                })
            }
            Sort::Battletag => accounts.sort_by(|a, b| match (&a.battletag, &b.battletag) {
                (Some(x), Some(y)) => x.to_lowercase().cmp(&y.to_lowercase()),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            }),
            role_sort => {
                let (role, ascending) = role_sort.role().unwrap();
                accounts.sort_by(|a, b| match (a.ratings.score(role), b.ratings.score(role)) {
                    (Some(x), Some(y)) => (if ascending { x.cmp(&y) } else { y.cmp(&x) }).then_with(|| b.last_login_at.cmp(&a.last_login_at)),
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => b.last_login_at.cmp(&a.last_login_at),
                });
            },
        }
        State {
            tab: inner.settings.tab,
            visible_account_ids: accounts.iter().filter(|a| inner.settings.tab.includes(a.game)).map(|a| a.id.clone()).collect(),
            syncing: inner.syncing,
            version: self.version().to_owned(),
            launcher,
            game_running: inner.game_running,
            operation: inner.operation.as_ref().map(|o| OperationView {
                kind: o.kind,
                account_id: o.account_id.clone(),
                phase: o.phase,
                cancellable: o.kind != OpKind::Add || o.phase == Phase::WaitingLogin,
            }),
            accounts,
            sort,
            dismissed: DismissedView {
                remember_login: inner.settings.dismissed.remember_login,
                logout_warning: inner.settings.dismissed.logout_warning,
            },
            notice: inner.notice.then_some("data_file_reset"),
            lang: inner.settings.resolved_language(&self.locale),
            settings: SettingsView {
                hide_emails: inner.settings.hide_emails,
                close_launcher: inner.settings.close_launcher,
                auto_ratings: inner.settings.auto_ratings,
                show_ratings: inner.settings.show_ratings,
                autostart: inner.autostart,
                after_start: inner.settings.after_start,
                show_game_start: inner.settings.show_game_start,
                language: inner.settings.language,
            },
            ask: inner.ask.clone(),
        }
    }

    /// Send the state to the interface if it changed.
    pub fn publish(&self) {
        let state = self.state();
        self.emit_state(&state);
    }
}

/// The background loop of the running program.
pub fn spawn(core: Arc<Core>) {
    std::thread::spawn(move || loop {
        std::thread::sleep(core.timing.poll);
        core.tick();
    });
}
