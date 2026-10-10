//! A stand-in for the Battle.net launcher, for the interface tests.
//!
//! Copied into a fake install directory under both launcher file names (and, in
//! another directory, as a fake game). It reads `scenario.json` beside itself and
//! acts it out: writes a new main log, edits the saved list, fills the login cache,
//! then stays alive until it is stopped. Without a scenario it only stays alive.
//!
//! Where it writes comes from the same environment variables the program reads:
//! `NOXDECK_BNET_ROAMING` and `NOXDECK_BNET_LOCAL`.

use rusqlite::Connection;
use serde::Deserialize;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Deserialize)]
struct Scenario {
    #[serde(default)]
    accounts: Vec<Account>,
    #[serde(default = "default_delay")]
    login_delay_ms: u64,
    /// Steps acted out when the launcher comes up on its login screen.
    #[serde(default)]
    login_screen: Vec<Step>,
    /// Steps acted out after a saved login worked.
    #[serde(default)]
    after_login: Vec<Step>,
    #[serde(default = "default_game_launch")]
    game_launch: String,
    #[serde(default = "default_readiness")]
    readiness: String,
}

fn default_game_launch() -> String {
    "ignores".into()
}
fn default_readiness() -> String {
    "playable".into()
}

fn default_delay() -> u64 {
    300
}

#[derive(Deserialize, Clone)]
struct Account {
    email: String,
    id: String,
    tag: String,
    /// `login` (default): the saved login works. `code`: stays on the challenge
    /// screen. `silent`: says nothing. `login_screen`: the saved login is gone.
    #[serde(default = "default_mode")]
    mode: String,
}

fn default_mode() -> String {
    "login".into()
}

#[derive(Deserialize)]
struct Step {
    at_ms: u64,
    /// Log in by hand as the account with this e-mail.
    login: Option<String>,
    #[serde(default)]
    logout: bool,
    #[serde(default)]
    exit: bool,
    playable: Option<bool>,
}

struct Env {
    roaming: PathBuf,
    local: PathBuf,
    log: PathBuf,
    accounts: Vec<Account>,
    logged_in: Mutex<Option<Account>>,
    playable: Mutex<Option<bool>>,
}

impl Env {
    fn line(&self, tag: &str, text: &str) {
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log)
            .unwrap();
        writeln!(f, "I 2026-10-09 01:00:00.000000 [{tag}] {{Main}} {text}").unwrap();
    }

    fn browser(&self, state: &str) {
        self.line(
            "UnifiedAuth",
            &format!("UAuth: browser state changed: {state}"),
        );
    }

    fn config(&self) -> PathBuf {
        self.roaming.join("Battle.net.config")
    }

    /// The saved list as a string, with the byte range of its value in the config.
    fn list_span(&self) -> Option<(String, usize, usize)> {
        let text = fs::read_to_string(self.config()).ok()?;
        let key = "\"SavedAccountNames\": \"";
        let start = text.find(key)? + key.len();
        let end = start + text[start..].find('"')?;
        Some((text, start, end))
    }

    fn saved_first(&self) -> Option<String> {
        let (text, s, e) = self.list_span()?;
        text[s..e]
            .split(',')
            .next()
            .filter(|x| !x.is_empty())
            .map(str::to_owned)
    }

    /// The launcher moves the account it logged into to the front of its list.
    fn put_first(&self, email: &str) {
        if let Some((text, s, e)) = self.list_span() {
            let mut names: Vec<&str> = text[s..e]
                .split(',')
                .filter(|n| !n.is_empty() && *n != email)
                .collect();
            names.insert(0, email);
            let new = format!("{}{}{}", &text[..s], names.join(","), &text[e..]);
            let _ = fs::write(self.config(), new);
        }
    }

    fn cache_row(&self, a: &Account) {
        let Ok(conn) = Connection::open(self.local.join("CachedData.db")) else {
            return;
        };
        let _ = conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS login_cache (name TEXT NOT NULL, environment TEXT NOT NULL,
             battle_tag TEXT NOT NULL, account_id_hi INT NOT NULL, account_id_lo INT NOT NULL,
             connected_environments TEXT NOT NULL DEFAULT '',
             UNIQUE(account_id_hi, account_id_lo, environment) ON CONFLICT REPLACE)",
        );
        let lo = i64::from_str_radix(&a.id, 16).unwrap_or(0);
        let _ = conn.execute(
            "INSERT INTO login_cache (name, environment, battle_tag, account_id_hi, account_id_lo)
             VALUES (?1, 'eu.actual.battle.net', ?2, 72057594037927936, ?3)",
            rusqlite::params![format!("hash{}", a.id), a.tag, lo],
        );
    }

    fn login(&self, a: &Account) {
        self.cache_row(a);
        self.put_first(&a.email);
        self.line(
            "BAPIGateway",
            "Using cached auth token on startup. tokenType=Default",
        );
        self.line(
            "BNLogin",
            &format!(
                "Logged into Battle.net successfully. |bnet=1:0:{}|game=2:0:00000000",
                a.id
            ),
        );
        *self.logged_in.lock().unwrap() = Some(a.clone());
    }

    fn act(&self, step: &Step) {
        if let Some(email) = &step.login {
            if let Some(a) = self.accounts.iter().find(|a| &a.email == email) {
                // A manual login passes through the credential screen first.
                self.browser("LoginCredential");
                std::thread::sleep(Duration::from_millis(150));
                self.login(a);
            }
        }
        if step.logout {
            *self.logged_in.lock().unwrap() = None;
            self.line(
                "BNLogin",
                "BattleNetLogin::DeleteToken(): Clearing cached token",
            );
            self.line("BNLogin", "Logout");
            self.browser("LoginCredential");
        }
        if let Some(playable) = step.playable {
            self.set_playable(Some(playable));
        }
        if step.exit {
            std::process::exit(0);
        }
    }

    fn run(&self, steps: &[Step], from_ms: u64) {
        let mut now = from_ms;
        let mut steps: Vec<&Step> = steps.iter().collect();
        steps.sort_by_key(|s| s.at_ms);
        for s in steps {
            std::thread::sleep(Duration::from_millis(s.at_ms.saturating_sub(now)));
            now = now.max(s.at_ms);
            self.act(s);
        }
    }

    fn set_playable(&self, playable: Option<bool>) {
        *self.playable.lock().unwrap() = playable;
        match playable {
            Some(true) => self.line("Agent", "InstallState (prometheus): installed=1 playable=1"),
            // A full state dump without playable= is the observed update case.
            Some(false) => self.line("Agent", "InstallState (prometheus): installed=1"),
            None => {}
        }
    }

    fn serve(&self, install: &std::path::Path, mode: &str) -> ! {
        let request = install.join("launch.request");
        loop {
            if fs::remove_file(&request).is_ok() {
                self.line("IPC", "Received IPCMessage: launch Pro");
                let account = self.logged_in.lock().unwrap().clone();
                if let Some(account) =
                    account.filter(|_| *self.playable.lock().unwrap() != Some(false))
                {
                    if matches!(mode, "stays" | "exits") {
                        let game_name = std::env::var_os("NOXDECK_GAME_EXE")
                            .unwrap_or_else(|| "FakeGame.exe".into());
                        let game = install.parent().unwrap().join("game").join(game_name);
                        let child = Command::new(game)
                            .creation_flags(0x08000000)
                            .stdin(Stdio::null())
                            .stdout(Stdio::null())
                            .stderr(Stdio::null())
                            .spawn()
                            .expect("the stand-in game");
                        fs::write(
                            install.join("game-started.json"),
                            serde_json::to_vec(&serde_json::json!({
                                "accountId": account.id, "pid": child.id()
                            }))
                            .unwrap(),
                        )
                        .unwrap();
                        let log = if mode == "exits" {
                            include_str!(
                                "../../../src-tauri/tests/fixtures/launch-by-hand-launcher-exits.log"
                            )
                        } else {
                            include_str!(
                                "../../../src-tauri/tests/fixtures/launch-command-launcher-stays.log"
                            )
                        };
                        // Keep the supplied fixture's launch sequence, without its login identity.
                        for line in log
                            .lines()
                            .skip_while(|line| !line.contains("LaunchBinary"))
                        {
                            let mut file = OpenOptions::new().append(true).open(&self.log).unwrap();
                            writeln!(file, "{line}").unwrap();
                        }
                        if mode == "exits" {
                            std::process::exit(0);
                        }
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

fn stay_alive() -> ! {
    loop {
        std::thread::sleep(Duration::from_secs(3600));
    }
}

// A visible top-level game window kept off-screen, without taskbar or activation.
fn game_window() -> ! {
    use std::ffi::c_void;
    #[repr(C)]
    struct Message {
        hwnd: *mut c_void, message: u32, wparam: usize, lparam: isize,
        time: u32, point: [i32; 2], private: u32,
    }
    #[link(name = "user32")]
    extern "system" {
        fn CreateWindowExW(ex: u32, class: *const u16, title: *const u16, style: u32,
            x: i32, y: i32, width: i32, height: i32, parent: *mut c_void,
            menu: *mut c_void, instance: *mut c_void, param: *mut c_void) -> *mut c_void;
        fn GetMessageW(message: *mut Message, window: *mut c_void, min: u32, max: u32) -> i32;
        fn DispatchMessageW(message: *const Message) -> isize;
    }
    let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
    let title: Vec<u16> = "Stand-in game\0".encode_utf16().collect();
    unsafe {
        let window = CreateWindowExW(0x08000080, class.as_ptr(), title.as_ptr(), 0x90000000,
            -32000, -32000, 1, 1, std::ptr::null_mut(), std::ptr::null_mut(),
            std::ptr::null_mut(), std::ptr::null_mut());
        assert!(!window.is_null(), "stand-in game window was not created");
        let mut message: Message = std::mem::zeroed();
        while GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) > 0 {
            DispatchMessageW(&message);
        }
    }
    std::process::exit(0)
}

fn main() {
    let exe = std::env::current_exe().unwrap();
    if exe.file_name().is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case(
        &std::env::var("NOXDECK_GAME_EXE").unwrap_or_else(|_| "FakeGame.exe".into())
    )) { game_window(); }
    let Ok(text) = fs::read_to_string(exe.with_file_name("scenario.json")) else {
        stay_alive();
    };
    let scenario: Scenario = serde_json::from_str(&text).expect("scenario.json");
    let dir = |var: &str| {
        PathBuf::from(
            std::env::var_os(var).expect("the stand-in directories come from the environment"),
        )
    };
    let (roaming, local) = (dir("NOXDECK_BNET_ROAMING"), dir("NOXDECK_BNET_LOCAL"));
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let logs = local.join("Logs");
    fs::create_dir_all(&logs).unwrap();
    let env = Env {
        log: logs.join(format!("battle.net-{millis:016}.log")),
        roaming,
        local,
        accounts: scenario.accounts.clone(),
        logged_in: Mutex::new(None),
        playable: Mutex::new(None),
    };
    let install = exe.parent().unwrap();
    if std::env::args().any(|arg| arg == "--exec=launch Pro") {
        env.line("Main", "Command line arguments: {[0]=--exec=launch Pro}");
        fs::write(install.join("launch.request"), b"launch Pro").unwrap();
        return;
    }
    // A cold sender request has no logged-in main process to receive it.
    let _ = fs::remove_file(install.join("launch.request"));
    env.line("Main", "Command line arguments: {[0]=--from-launcher}");
    env.set_playable(match scenario.readiness.as_str() {
        "not_playable" => Some(false),
        "unknown" => None,
        _ => Some(true),
    });

    let first = env.saved_first();
    let known = first
        .as_ref()
        .and_then(|e| scenario.accounts.iter().find(|a| &a.email == e));
    match known {
        Some(a) if a.mode != "login_screen" => {
            std::thread::sleep(Duration::from_millis(scenario.login_delay_ms));
            match a.mode.as_str() {
                "code" => env.browser("Challenge"),
                "silent" => {}
                _ => {
                    // The challenge screen flickers on the way to a saved login.
                    env.browser("Challenge");
                    std::thread::sleep(Duration::from_millis(100));
                    env.browser("None");
                    env.line(
                        "UnifiedAuth",
                        "UAuth: external challenge complete, submitting credentials",
                    );
                    env.browser("Done");
                    env.login(a);
                    std::thread::scope(|scope| {
                        scope.spawn(|| {
                            env.run(&scenario.after_login, scenario.login_delay_ms + 100)
                        });
                        env.serve(install, &scenario.game_launch);
                    });
                }
            }
        }
        _ => {
            std::thread::sleep(Duration::from_millis(scenario.login_delay_ms));
            env.browser("LoginCredential");
            std::thread::scope(|scope| {
                scope.spawn(|| env.run(&scenario.login_screen, scenario.login_delay_ms));
                env.serve(install, &scenario.game_launch);
            });
        }
    }
    env.serve(install, &scenario.game_launch)
}
