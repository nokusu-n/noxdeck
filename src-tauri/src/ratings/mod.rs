//! The three values shown on a record. Manual and service writes share the core lock.
use serde::{Deserialize, Serialize};
use crate::{lock, Core, Inner};
use std::{sync::{Arc, mpsc::{Receiver, RecvTimeoutError}}, time::{Duration, SystemTime}};
pub mod overfast;

pub(crate) enum Refresh { All, NewAccounts }

pub const RANKS: [&str; 9] = ["bronze", "silver", "gold", "platinum", "emerald", "diamond", "master", "grandmaster", "champion"];

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Role { Tank, Damage, Support }

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct RatingValue {
    pub rank: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub division: Option<u8>,
}
impl RatingValue {
    pub fn valid(&self) -> bool {
        if self.rank == "placement" { self.division.is_none() }
        else { RANKS.contains(&self.rank.as_str()) && self.division.is_some_and(|d| (1..=5).contains(&d)) }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Source { Manual, Service }

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Rating {
    #[serde(flatten)]
    pub value: RatingValue,
    pub source: Source,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(default)]
pub struct Ratings {
    pub tank: Option<Rating>,
    pub damage: Option<Rating>,
    pub support: Option<Rating>,
}
impl Ratings {
    pub fn score(&self, role: Role) -> Option<usize> {
        let rating = match role { Role::Tank => &self.tank, Role::Damage => &self.damage, Role::Support => &self.support }.as_ref()?;
        Some(RANKS.iter().position(|r| *r == rating.value.rank)? * 5 + 5 - usize::from(rating.value.division?))
    }
    pub fn role_mut(&mut self, role: Role) -> &mut Option<Rating> {
        match role { Role::Tank => &mut self.tank, Role::Damage => &mut self.damage, Role::Support => &mut self.support }
    }
    pub fn valid(&self) -> bool {
        [&self.tank, &self.damage, &self.support].iter().all(|r| r.as_ref().is_none_or(|r| r.value.valid()))
    }
}

impl Core {
    pub fn apply_rating_report(&self, id: &str, report: [Option<RatingValue>; 3]) {
        self.apply_report(id, report, None);
    }

    fn apply_report(&self, id: &str, report: [Option<RatingValue>; 3], requested_tag: Option<&str>) {
        {
            let mut inner = lock(&self.inner);
            if requested_tag.is_some() && !(inner.settings.auto_ratings && inner.settings.show_ratings) { return; }
            let Some(account) = inner.registry.find_mut(id) else { return; };
            if requested_tag.is_some() && account.battletag.as_deref() != requested_tag { return; }
            let mut changed = false;
            for (role, value) in [Role::Tank, Role::Damage, Role::Support].into_iter().zip(report) {
                if let Some(value) = value.filter(|v| v.rank != "placement" && v.valid()) {
                    *account.ratings.role_mut(role) = Some(Rating { value, source: Source::Service });
                    changed = true;
                }
            }
            if changed { self.save_registry(&inner); }
        }
        self.publish();
    }

    pub fn set_auto_ratings(&self, value: bool) {
        let enabled;
        {
            let mut inner = lock(&self.inner);
            enabled = value && !inner.settings.auto_ratings;
            inner.settings.auto_ratings = value;
            self.save_settings(&inner);
        }
        self.publish();
        if enabled { self.sync_ratings(); }
    }

    pub fn sync_ratings(&self) {
        {
            let mut inner = lock(&self.inner);
            if inner.syncing || !(inner.settings.show_ratings && inner.settings.auto_ratings) { return; }
            inner.syncing = true;
            inner.ratings_last = Some(SystemTime::now());
        }
        self.publish();
        let _ = self.ratings_trigger.try_send(Refresh::All);
    }

    /// Queue under the insertion lock so a full pass cannot see the record first.
    pub(crate) fn refresh_added_account(&self, inner: &mut Inner, id: &str) {
        if !(inner.settings.show_ratings && inner.settings.auto_ratings)
            || !inner.registry.find(id).is_some_and(|a| a.battletag.is_some()) { return; }
        if !inner.ratings_pending.iter().any(|pending| pending == id) {
            inner.ratings_pending.push(id.to_owned());
        }
        if !inner.syncing {
            inner.syncing = true;
            let _ = self.ratings_trigger.try_send(Refresh::NewAccounts);
        }
    }
}

/// Wall time includes time asleep; advancing it never creates missed-refresh jobs.
pub fn due(last: Option<SystemTime>, now: SystemTime, interval: Duration) -> bool {
    last.is_none_or(|last| now.duration_since(last).unwrap_or_default() >= interval)
}

pub(crate) fn spawn(core: &Arc<Core>, client: overfast::Client, receiver: Receiver<Refresh>) {
    let weak = Arc::downgrade(core);
    let poll = core.timing.ratings_interval.min(Duration::from_millis(250)).max(Duration::from_millis(10));
    std::thread::spawn(move || loop {
        match receiver.recv_timeout(poll) {
            Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {
                let Some(core) = weak.upgrade() else { break; };
                let should_run = due(lock(&core.inner).ratings_last, SystemTime::now(), core.timing.ratings_interval);
                if should_run { core.sync_ratings(); }
            }
            Ok(refresh) => {
                let Some(core) = weak.upgrade() else { break; };
                let mut ids: Vec<_> = match refresh {
                    Refresh::All => core.state().accounts.into_iter().map(|a| a.id).collect(),
                    Refresh::NewAccounts => std::mem::take(&mut lock(&core.inner).ratings_pending),
                };
                // Records inserted before the full snapshot are already included.
                lock(&core.inner).ratings_pending.retain(|id| !ids.contains(id));
                core.publish();
                drop(core);
                loop {
                    let mut stopped = false;
                    for id in ids {
                        let Some(core) = weak.upgrade() else { return; };
                        let tag = {
                            let inner = lock(&core.inner);
                            if !(inner.settings.show_ratings && inner.settings.auto_ratings) { break; }
                            inner.registry.find(&id).and_then(|a| a.battletag.clone())
                        };
                        drop(core);
                        let Some(tag) = tag else { continue; };
                        let mut wait = core_wait(&weak);
                        // No core lock or strong reference is held across a network wait.
                        let report = client.fetch(&tag);
                        if let Some(wait) = &mut wait { wait.end(match &report { overfast::Report::Stop => crate::diagnostics::WaitEnd::Failed, _ => crate::diagnostics::WaitEnd::Event }); }
                        drop(wait);
                        let Some(core) = weak.upgrade() else { return; };
                        match report {
                            overfast::Report::Stop => { stopped = true; break; },
                            overfast::Report::Ratings(values) => core.apply_report(&id, values, Some(&tag)),
                        }
                    }
                    let Some(core) = weak.upgrade() else { return; };
                    let mut inner = lock(&core.inner);
                    if stopped || !(inner.settings.show_ratings && inner.settings.auto_ratings) {
                        inner.ratings_pending.clear();
                    }
                    ids = std::mem::take(&mut inner.ratings_pending);
                    let done = ids.is_empty();
                    inner.syncing = !done;
                    drop(inner);
                    core.publish();
                    if done { break; }
                }
            }
        }
    });
}

fn core_wait(core: &std::sync::Weak<Core>) -> Option<crate::diagnostics::Wait> {
    core.upgrade().map(|core| core.journal.wait(crate::diagnostics::WaitFor::Ratings, Some(overfast::TIMEOUT)))
}
