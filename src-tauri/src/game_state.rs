//! The account and process NoxDeck observed playing; no launcher session data.
use crate::registry::{self, Loaded, Registry};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct InGame {
    pub account_id: String,
    pub pid: u32,
}

#[derive(Default)]
pub(crate) struct GameTracking {
    pub confirmed: Option<InGame>,
    candidate: Option<(String, Instant, Vec<u32>)>,
    pub restored: bool,
}

#[derive(Serialize, Deserialize)]
struct Saved {
    schema: u32,
    game: Option<InGame>,
}

impl GameTracking {
    pub fn load(path: &Path) -> (Self, bool) {
        match registry::load::<Saved>(path) {
            Loaded::Ok(saved)
                if saved.schema == 1
                    && saved.game.as_ref().is_none_or(|g| {
                        g.pid != 0
                            && g.account_id.len() == 8
                            && g.account_id
                                .chars()
                                .all(|c| matches!(c, '0'..='9' | 'a'..='f'))
                    }) =>
            {
                (
                    Self {
                        confirmed: saved.game,
                        restored: true,
                        candidate: None,
                    },
                    false,
                )
            }
            Loaded::Ok(_) => {
                registry::quarantine(path);
                (Self::default(), true)
            }
            Loaded::Broken => (Self::default(), true),
            Loaded::Missing => (Self::default(), false),
        }
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        registry::save(
            path,
            &Saved {
                schema: 1,
                game: self.confirmed.clone(),
            },
        )
    }

    pub fn launched(&mut self, id: Option<&str>, now: Instant, previous_pids: &[u32]) {
        self.candidate = id.map(|id| (id.to_owned(), now, previous_pids.to_vec()));
    }

    pub fn tick(&mut self, now: Instant, pids: &[u32]) {
        if self
            .confirmed
            .as_ref()
            .is_some_and(|g| !pids.contains(&g.pid))
        {
            self.confirmed = None;
        }
        if self
            .candidate
            .as_ref()
            .is_some_and(|(_, at, _)| now.duration_since(*at) > Duration::from_secs(120))
        {
            self.candidate = None;
        }
        let new_pid = self.candidate.as_ref().and_then(|(_, _, previous)| pids.iter().find(|pid| !previous.contains(pid)).copied());
        if let Some(pid) = new_pid {
            if let Some((account_id, _, _)) = self.candidate.take() {
                self.confirmed = Some(InGame { account_id, pid });
            }
        }
    }

    pub fn retain_listed(&mut self, registry: &Registry) {
        if self
            .confirmed
            .as_ref()
            .is_some_and(|g| registry.find(&g.account_id).is_none())
        {
            self.confirmed = None;
        }
        if self
            .candidate
            .as_ref()
            .is_some_and(|(id, _, _)| registry.find(id).is_none())
        {
            self.candidate = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_covers_the_slow_game_bound_and_expires_after_it() {
        let now = Instant::now();
        let mut in_time = GameTracking::default();
        in_time.launched(Some("aaaa0001"), now, &[]);
        in_time.tick(now + Duration::from_secs(120), &[77]);
        assert_eq!(
            in_time.confirmed,
            Some(InGame {
                account_id: "aaaa0001".into(),
                pid: 77
            })
        );
        let mut late = GameTracking::default();
        late.launched(Some("aaaa0001"), now, &[]);
        late.tick(now + Duration::from_secs(120) + Duration::from_nanos(1), &[]);
        late.tick(now + Duration::from_secs(121), &[88]);
        assert_eq!(late.confirmed, None);
    }

    #[test]
    fn a_game_seen_after_the_deadline_cannot_confirm_an_expired_candidate() {
        let now = Instant::now();
        let mut game = GameTracking::default();
        game.launched(Some("aaaa0001"), now, &[]);
        game.tick(now + Duration::from_secs(121), &[77]);
        assert_eq!(game.confirmed, None);
        game.launched(Some("aaaa0002"), now + Duration::from_secs(122), &[]);
        game.tick(now + Duration::from_secs(122), &[88]);
        assert_eq!(game.confirmed.as_ref().unwrap().account_id, "aaaa0002");
    }
}
