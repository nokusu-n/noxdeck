//! The launcher's log: choose the main log, tail it, turn lines into events.
//!
//! A log line never leaves this module as text: it becomes a `LauncherEvent` or is
//! dropped (constitution III).

use super::{GameReadiness, LauncherEvent};
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::time::{Duration, Instant};

/// How long a login or challenge screen must stand before it counts (`research.md` §5).
pub const STAND: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Browser {
    Login,
    Code,
    Other,
}

#[derive(Debug, PartialEq, Eq)]
enum Line {
    GameLaunched,
    Readiness(GameReadiness),
    LoggedIn(String),
    Logout,
    Browser(Browser),
    Unrelated,
}

fn parse_line(line: &str) -> Line {
    if line.contains("Launched ") && line.contains("-uid prometheus") {
        return Line::GameLaunched;
    }
    if let Some((_, fields)) = line.split_once("InstallState (prometheus):") {
        let tokens: Vec<&str> = fields.split_ascii_whitespace().collect();
        let playable = tokens.iter().find(|t| t.starts_with("playable="));
        let readiness = match playable.copied() {
            Some("playable=1") => Some(GameReadiness::Playable),
            Some("playable=0") => Some(GameReadiness::NotPlayable),
            None if tokens.iter().any(|t| t.starts_with("installed=")) => {
                Some(GameReadiness::NotPlayable)
            }
            _ => None,
        };
        if let Some(r) = readiness {
            return Line::Readiness(r);
        }
    }
    if line.contains("Logged into Battle.net successfully.") {
        if let Some(at) = line.find("|bnet=1:0:") {
            let id: String = line[at + "|bnet=1:0:".len()..]
                .chars()
                .take_while(|c| c.is_ascii_hexdigit())
                .collect();
            if id.len() == 8 {
                return Line::LoggedIn(id.to_ascii_lowercase());
            }
        }
        return Line::Unrelated;
    }
    if line.trim_end().ends_with("} Logout") {
        return Line::Logout;
    }
    if let Some(at) = line.find("browser state changed: ") {
        return Line::Browser(match line[at + "browser state changed: ".len()..].trim() {
            "LoginCredential" => Browser::Login,
            "Challenge" => Browser::Code,
            _ => Browser::Other,
        });
    }
    Line::Unrelated
}

enum Selection {
    /// The newest log has not written its first line yet: decide next poll.
    Undetermined,
    None,
    Main(String),
}

/// The newest `battle.net-*.log` whose `Command line arguments` line has no `--exec`.
fn select(dir: &Path) -> Selection {
    let Ok(entries) = fs::read_dir(dir) else {
        return Selection::None;
    };
    let mut names: Vec<String> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| {
            let n = n.to_ascii_lowercase();
            n.starts_with("battle.net-") && n.ends_with(".log")
        })
        .collect();
    names.sort();
    for name in names.into_iter().rev() {
        let mut head = vec![0u8; 8192];
        let Ok(mut f) = File::open(dir.join(&name)) else {
            continue;
        };
        let n = f.read(&mut head).unwrap_or(0);
        let head = String::from_utf8_lossy(&head[..n]);
        match head.lines().find(|l| l.contains("Command line arguments")) {
            None => return Selection::Undetermined,
            Some(l) if l.contains("--exec") => continue,
            Some(_) => return Selection::Main(name),
        }
    }
    Selection::None
}

/// With the launcher closed: the account its newest main log ended on, provided it
/// logged in and did not log out afterwards (`research.md` §8).
pub fn last_session(dir: &Path) -> Option<String> {
    let Selection::Main(name) = select(dir) else {
        return None;
    };
    let bytes = fs::read(dir.join(name)).ok()?;
    let mut current = None;
    for line in String::from_utf8_lossy(&bytes).lines() {
        match parse_line(line) {
            Line::LoggedIn(id) => current = Some(id),
            Line::Logout => current = None,
            _ => {}
        }
    }
    current
}

pub struct Tailed {
    pub event: LauncherEvent,
    /// Read from the log on the first poll after the program started: it happened
    /// before the program was watching.
    pub historic: bool,
}

#[derive(Default)]
pub struct Polled {
    /// The poll switched to a different log file: the launcher was started again.
    pub new_file: bool,
    pub events: Vec<Tailed>,
}

pub struct LogTail {
    file: Option<String>,
    offset: u64,
    pending: Option<(Browser, Instant)>,
    first_poll: bool,
    stand: Duration,
    readiness: GameReadiness,
}

impl LogTail {
    pub fn readiness(&self) -> GameReadiness {
        self.readiness
    }

    pub fn new(stand: Duration) -> Self {
        LogTail {
            file: None,
            offset: 0,
            pending: None,
            first_poll: true,
            stand,
            readiness: GameReadiness::Unknown,
        }
    }

    pub fn poll(&mut self, dir: &Path, now: Instant) -> Polled {
        let historic = std::mem::replace(&mut self.first_poll, false);
        let mut out = Polled::default();
        match select(dir) {
            Selection::Undetermined | Selection::None => return out,
            Selection::Main(name) => {
                if self.file.as_deref() != Some(name.as_str()) {
                    self.file = Some(name);
                    self.offset = 0;
                    self.pending = None;
                    self.readiness = GameReadiness::Unknown;
                    out.new_file = true;
                }
            }
        }
        let path = dir.join(self.file.as_ref().unwrap());
        let Ok(mut f) = File::open(&path) else {
            return out;
        };
        let len = f.metadata().map(|m| m.len()).unwrap_or(0);
        if len < self.offset {
            self.offset = 0; // truncated or replaced
        }
        let mut buf = Vec::new();
        if f.seek(SeekFrom::Start(self.offset)).is_ok() {
            let _ = f.read_to_end(&mut buf);
        }
        // Only whole lines: a half-written one waits for the next poll.
        let used = buf
            .iter()
            .rposition(|b| *b == b'\n')
            .map(|i| i + 1)
            .unwrap_or(0);
        self.offset += used as u64;
        for line in String::from_utf8_lossy(&buf[..used]).lines() {
            match parse_line(line) {
                Line::GameLaunched => out.events.push(Tailed {
                    event: LauncherEvent::GameLaunched,
                    historic,
                }),
                Line::Readiness(r) => {
                    if self.readiness != r {
                        self.readiness = r;
                        out.events.push(Tailed {
                            event: LauncherEvent::GameReadiness(r),
                            historic,
                        });
                    }
                }
                Line::LoggedIn(id) => {
                    self.pending = None;
                    out.events.push(Tailed {
                        event: LauncherEvent::LoggedIn { id },
                        historic,
                    });
                }
                Line::Logout => {
                    self.pending = None;
                    out.events.push(Tailed {
                        event: LauncherEvent::LoggedOut,
                        historic,
                    });
                }
                Line::Browser(Browser::Other) => self.pending = None,
                Line::Browser(b) => self.pending = Some((b, now)),
                Line::Unrelated => {}
            }
        }
        if let Some((b, since)) = self.pending {
            if now.duration_since(since) >= self.stand {
                self.pending = None;
                let event = if b == Browser::Login {
                    LauncherEvent::LoginScreen
                } else {
                    LauncherEvent::CodeScreen
                };
                out.events.push(Tailed {
                    event,
                    historic: false,
                });
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_launch_requires_both_markers_and_keeps_log_order() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("battle.net-1.log"),
            concat!(
                "Command line arguments: {}\n",
                "Logged into Battle.net successfully. |bnet=1:0:aaaa0001|\n",
                "LaunchBinary: uid=prometheus\n",
                "Launched Other.exe with args: -uid other\n",
                "Launch failed with args: -uid prometheus\n",
                "Launched StandIn.exe with args: -uid prometheus, pid: 9999\n",
                "Logged into Battle.net successfully. |bnet=1:0:aaaa0002|\n"
            ),
        )
        .unwrap();
        let mut tail = LogTail::new(STAND);
        let events: Vec<_> = tail
            .poll(dir.path(), Instant::now())
            .events
            .into_iter()
            .map(|t| t.event)
            .collect();
        assert_eq!(
            events,
            vec![
                LauncherEvent::LoggedIn {
                    id: "aaaa0001".into()
                },
                LauncherEvent::GameLaunched,
                LauncherEvent::LoggedIn {
                    id: "aaaa0002".into()
                },
            ]
        );
    }

    #[test]
    fn parses_the_lines_that_matter() {
        let login = "I 2026-10-09 01:28:36.788847 [BNLogin] {Main} Logged into Battle.net successfully. |bnet=1:0:AAAA0002|game=2:0:00000000";
        assert_eq!(parse_line(login), Line::LoggedIn("aaaa0002".into()));
        assert_eq!(parse_line("I 2026 [BNLogin] {Main} Logout"), Line::Logout);
        assert_eq!(
            parse_line("I [BNLogin] {Main} BattleNetLogin::DeleteToken(): Clearing cached token"),
            Line::Unrelated
        );
        assert_eq!(
            parse_line("D [UnifiedAuth] {} UAuth: browser state changed: Challenge"),
            Line::Browser(Browser::Code)
        );
        assert_eq!(
            parse_line("D [UnifiedAuth] {} UAuth: browser state changed: None"),
            Line::Browser(Browser::Other)
        );
    }
}
