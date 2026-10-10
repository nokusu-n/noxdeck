//! Only the enabled BattleTag lookup leaves the core; credentials and launcher
//! data never enter the request, interface networking or log output.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[test]
fn only_overwatch_battletags_leave_and_automatic_requests_require_auto() {
    use common::{account_json, accounts_file, fast, Fake, wait_for};
    use common::overfast::{Reply, Stub};
    use noxdeck_lib::{ratings::overfast::Client, settings::Settings, Core};
    use std::time::Duration;
    for (show, auto) in [(true, true), (false, true), (true, false), (false, false)] {
        let f = Fake::new();
        let tag = "ExamplePlayer#12345";
        f.seed_accounts(&accounts_file(&[account_json("aaaa0001", Some("placeholder@example.com"), Some(tag), "ready")]));
        Settings { auto_ratings: auto, ..Settings::default() }.save(&f.paths.data.join("settings.json")).unwrap();
        let stub = Stub::new(); stub.set(tag, Reply::json("{}"));
        let mut timing = fast(); timing.ratings_interval = Duration::from_millis(30);
        if !show {
            let mut registry = noxdeck_lib::registry::Registry::load(&f.paths.data.join("accounts.json")).0;
            registry.migrate(true); registry.accounts[0].game = noxdeck_lib::registry::Game::None; registry.save(&f.paths.data.join("accounts.json")).unwrap();
        }
        let core = Core::new_with_ratings(f.paths.clone(), timing, f.procs.clone(), Box::new(|_| {}), "test", f.shell.clone(), Client::new(&stub.url));
        if !show { core.set_account_game("aaaa0001", noxdeck_lib::registry::Game::None).unwrap(); }
        if show && auto {
            wait_for(&core, "rating request", |_| stub.count() > 0);
        }
        std::thread::sleep(Duration::from_millis(120));
        let requests = stub.requests.lock().unwrap();
        if !(show && auto) { assert!(requests.is_empty(), "disabled ratings sent a request"); continue; }
        assert!(!requests.is_empty());
        for request in requests.iter() {
            let (headers, body) = request.split_once("\r\n\r\n").unwrap_or_else(|| panic!("incomplete captured request: {request:?}"));
            assert!(body.is_empty());
            let mut lines = headers.split("\r\n");
            assert_eq!(lines.next(), Some("GET /players/ExamplePlayer-12345/summary HTTP/1.1"));
            let mut hosts = 0;
            for line in lines {
                let (name, value) = line.split_once(':').unwrap();
                match name.to_ascii_lowercase().as_str() {
                    "host" => { hosts += 1; assert_eq!(value.trim(), stub.url.strip_prefix("http://").unwrap()); }
                    "accept" => assert_eq!(value.trim(), "*/*"),
                    "user-agent" => assert!(value.trim().is_empty()),
                    "content-length" => assert_eq!(value.trim(), "0"),
                    _ => panic!("unexpected HTTP header: {name}"),
                }
            }
            assert_eq!(hosts, 1);
            assert!(!request.contains("placeholder@example.com"));
            assert!(!request.contains("aaaa0001"));
        }
    }
}

fn manifest() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn sources(dir: &Path, ext: &str, out: &mut Vec<(PathBuf, String)>) {
    for e in fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            sources(&p, ext, out);
        } else if p.extension().is_some_and(|x| x == ext) {
            out.push((p.clone(), fs::read_to_string(&p).unwrap()));
        }
    }
}

fn find(files: &[(PathBuf, String)], needles: &[&str]) -> Vec<String> {
    let mut hits = Vec::new();
    for (path, text) in files {
        for n in needles {
            if text.contains(n) {
                hits.push(format!("{} contains {n}", path.display()));
            }
        }
    }
    hits
}

#[test]
fn the_core_writes_no_log_output() {
    let mut files = Vec::new();
    sources(&manifest().join("src"), "rs", &mut files);
    assert!(files.len() > 5);
    let hits = find(
        &files,
        &["println!", "eprintln!", "print!(", "eprint!(", "dbg!(", "log::info", "log::warn", "log::error", "log::debug", "log::trace", "use log", "tracing::", "env_logger", "tauri_plugin_log"],
    );
    assert!(hits.is_empty(), "a log line could carry a launcher line, an e-mail or a tag: {hits:?}");
}

#[test]
fn networking_is_confined_to_the_rating_client_and_absent_from_the_interface() {
    let mut rust = Vec::new();
    sources(&manifest().join("src"), "rs", &mut rust);
    let hits = find(&rust, &["std::net", "TcpStream", "UdpSocket", "reqwest", "hyper::"]);
    assert!(hits.is_empty(), "{hits:?}");
    rust.retain(|(path, _)| *path != manifest().join("src/ratings/overfast.rs"));
    let hits = find(&rust, &["ureq::"]);
    assert!(hits.is_empty(), "{hits:?}");

    let mut ui = Vec::new();
    let root = manifest().join("..").join("src");
    sources(&root, "ts", &mut ui);
    sources(&root, "tsx", &mut ui);
    assert!(ui.len() > 3);
    let hits = find(&ui, &["fetch(", "XMLHttpRequest", "WebSocket", "EventSource", "http://", "https://", "sendBeacon"]);
    assert!(hits.is_empty(), "{hits:?}");
}

#[test]
fn the_windows_build_uses_only_the_chosen_http_client_and_platform_tls() {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let run = |extra: &[&str]| {
        let out = Command::new(&cargo)
            .args(["tree", "-p", "noxdeck", "--target", "x86_64-pc-windows-msvc", "-e", "normal"])
            .args(extra)
            .current_dir(manifest())
            .output()
            .expect("cargo tree runs");
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).into_owned()
    };
    let tree = run(&["--prefix", "none", "--format", "{p}"]);
    let crates: Vec<&str> = tree.lines().filter_map(|l| l.split_whitespace().next()).collect();
    assert!(crates.contains(&"tauri"), "the tree was read");
    for permitted in ["ureq", "native-tls", "schannel"] {
        assert!(crates.contains(&permitted), "the chosen client and Windows TLS are present");
    }
    for net in [
        "reqwest", "hyper", "curl", "isahc", "attohttpc", "h2", "rustls", "openssl",
        "tungstenite", "tokio-tungstenite", "mio", "socket2",
    ] {
        assert!(!crates.contains(&net), "network crate in the build: {net}");
    }
    // The async runtime Tauri brings is built without sockets.
    let features = run(&["-e", "features", "-i", "tokio"]);
    assert!(!features.contains("feature \"net\""), "tokio is built with sockets:\n{features}");
}

#[test]
fn the_window_may_only_talk_to_the_core() {
    let boundary = fs::read_to_string(manifest().join("../src/core.ts")).unwrap();
    let mut commands: Vec<_> = boundary.lines().filter(|line| line.contains("=> call")).map(|line| line.rsplit_once("(\"").unwrap().1.split('"').next().unwrap()).collect();
    commands.sort_unstable();
    let mut permitted = vec!["open_repository", "set_rating", "set_auto_ratings", "sync_ratings", "add_account", "cancel_operation", "save_current", "start_launcher", "start_game", "delete_account", "set_sort", "dismiss", "acknowledge_notice", "clear_ask", "set_autostart", "set_after_start", "set_close_launcher", "set_language", "save_diagnostics"];
    permitted.sort_unstable();
    permitted.extend(["set_hide_emails", "set_account_game", "set_tab"]);
    permitted.sort_unstable();
    assert_eq!(commands, permitted, "only the pinned core commands cross the boundary");
    let cap = fs::read_to_string(manifest().join("capabilities").join("default.json")).unwrap();
    let cap: serde_json::Value = serde_json::from_str(&cap).unwrap();
    // The window's own controls for the custom title bar, and nothing else.
    assert_eq!(
        cap["permissions"],
        serde_json::json!(["core:default", "core:window:allow-minimize", "core:window:allow-toggle-maximize", "core:window:allow-close", "core:window:allow-start-dragging"]),
        "no http, fs, shell or opener permission"
    );

    let conf = fs::read_to_string(manifest().join("tauri.conf.json")).unwrap();
    let conf: serde_json::Value = serde_json::from_str(&conf).unwrap();
    let csp = conf["app"]["security"]["csp"].as_str().unwrap();
    assert!(csp.contains("default-src 'self'"));
    assert!(!csp.replace("http://ipc.localhost", "").contains("http"), "the page may not reach any host: {csp}");
}
