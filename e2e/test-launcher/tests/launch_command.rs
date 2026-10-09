use std::fs;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct StandIn {
    root: tempfile::TempDir,
    install: PathBuf,
    children: Vec<Child>,
}

impl StandIn {
    fn new(mode: &str) -> Self {
        let root = tempfile::tempdir().unwrap();
        let install = root.path().join("install");
        for name in ["install", "roaming", "local", "game"] {
            fs::create_dir(root.path().join(name)).unwrap();
        }
        let exe = env!("CARGO_BIN_EXE_test-launcher");
        for name in ["Battle.net Launcher.exe", "Battle.net.exe"] {
            fs::copy(exe, install.join(name)).unwrap();
        }
        fs::copy(exe, root.path().join("game/FakeGame.exe")).unwrap();
        fs::write(
            root.path().join("roaming/Battle.net.config"),
            "{\"Client\":{\"SavedAccountNames\": \"userA@example.com\"}}",
        )
        .unwrap();
        fs::write(
            install.join("scenario.json"),
            serde_json::json!({
                "accounts": [{ "id":"aaaa0001", "email":"userA@example.com", "tag":"Alpha#1111" }],
                "login_delay_ms": 10, "game_launch": mode
            })
            .to_string(),
        )
        .unwrap();
        Self {
            root,
            install,
            children: Vec::new(),
        }
    }

    fn start(&mut self, sender: bool) -> usize {
        let mut cmd = Command::new(self.install.join(if sender {
            "Battle.net.exe"
        } else {
            "Battle.net Launcher.exe"
        }));
        cmd.env("NOXDECK_BNET_ROAMING", self.root.path().join("roaming"))
            .env("NOXDECK_BNET_LOCAL", self.root.path().join("local"))
            .env("NOXDECK_GAME_EXE", "FakeGame.exe")
            .creation_flags(0x0800_0000)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if sender {
            cmd.arg("--exec=launch Pro");
        }
        self.children.push(cmd.spawn().unwrap());
        self.children.len() - 1
    }

    fn until(&mut self, cond: impl Fn(&mut Self) -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if cond(self) {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn main_logged_in(&self) -> bool {
        fs::read_dir(self.root.path().join("local/Logs"))
            .ok()
            .into_iter()
            .flatten()
            .flatten()
            .any(|e| {
                fs::read_to_string(e.path())
                    .unwrap_or_default()
                    .contains("Logged into Battle.net successfully.")
            })
    }
}

impl Drop for StandIn {
    fn drop(&mut self) {
        for child in &mut self.children {
            let _ = child.kill();
            let _ = child.wait();
        }
        let dir = self
            .root
            .path()
            .join("game")
            .to_string_lossy()
            .replace('\'', "''");
        let script = format!("Get-CimInstance Win32_Process | Where-Object {{ $_.ExecutablePath -and $_.ExecutablePath.StartsWith('{dir}\\', 'OrdinalIgnoreCase') }} | ForEach-Object {{ Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }}");
        let _ = Command::new("powershell")
            .args(["-NoProfile", "-Command", &script])
            .creation_flags(0x0800_0000)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

fn launch_case(mode: &str, exits: bool) {
    let mut f = StandIn::new(mode);
    let main = f.start(false);
    assert!(f.until(|f| f.main_logged_in()), "main instance logged in");
    let sender = f.start(true);
    assert!(
        f.until(|f| f.children[sender].try_wait().unwrap().is_some()),
        "the sender must exit"
    );
    assert!(
        f.until(|f| f.install.join("game-started.json").exists()),
        "the main instance starts the game"
    );
    let game: serde_json::Value =
        serde_json::from_slice(&fs::read(f.install.join("game-started.json")).unwrap()).unwrap();
    assert_eq!(game["accountId"], "aaaa0001");
    assert!(game["pid"].as_u64().unwrap() > 0);
    if exits {
        assert!(f.until(|f| f.children[main].try_wait().unwrap().is_some()));
    } else {
        assert!(f.children[main].try_wait().unwrap().is_none());
    }
}

#[test]
fn fake_launches_the_game_and_keeps_the_launcher() {
    launch_case("stays", false);
}

#[test]
fn fake_launches_the_game_and_exits_the_launcher() {
    launch_case("exits", true);
}

#[test]
fn fake_can_ignore_the_launch_command() {
    let mut f = StandIn::new("ignores");
    let main = f.start(false);
    assert!(f.until(|f| f.main_logged_in()));
    let sender = f.start(true);
    assert!(
        f.until(|f| f.children[sender].try_wait().unwrap().is_some()),
        "sender exits even if command ignored"
    );
    std::thread::sleep(Duration::from_millis(200));
    assert!(!f.install.join("game-started.json").exists());
    assert!(f.children[main].try_wait().unwrap().is_none());
}

#[test]
fn fake_does_not_launch_from_a_cold_exec_request() {
    let mut f = StandIn::new("stays");
    let sender = f.start(true);
    assert!(
        f.until(|f| f.children[sender].try_wait().unwrap().is_some()),
        "cold sender exits"
    );
    f.start(false);
    assert!(f.until(|f| f.main_logged_in()));
    std::thread::sleep(Duration::from_millis(200));
    assert!(!f.install.join("game-started.json").exists());
}
