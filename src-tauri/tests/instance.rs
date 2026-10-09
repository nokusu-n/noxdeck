use noxdeck_lib::shell::{instance::InstanceGuard, ShellRequest};
use std::{fs, sync::mpsc, time::Duration};

#[test]
fn instance_child() {
    let Ok(marker) = std::env::var("NOXDECK_GUARD_MARKER") else {
        return;
    };
    let first = std::env::var("NOXDECK_GUARD_FIRST").unwrap() == "true";
    let tray = std::env::var("NOXDECK_GUARD_TRAY").unwrap() == "true";
    let (tx, rx) = mpsc::channel();
    let guard = InstanceGuard::acquire(tx, tray).unwrap();
    assert_eq!(guard.is_some(), first);
    if first {
        fs::write(marker, "ready").unwrap();
        if tray {
            assert!(rx.recv_timeout(Duration::from_millis(800)).is_err());
        } else {
            assert_eq!(
                rx.recv_timeout(Duration::from_secs(5)).unwrap(),
                ShellRequest::ShowWindow
            );
        }
    }
}

fn pair(tray: bool) {
    use std::os::windows::process::CommandExt;
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("ready");
    let instance = format!("guard-test-{}-{tray}", std::process::id());
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "instance_child", "--nocapture"])
        .env("NOXDECK_INSTANCE", instance)
        .env("NOXDECK_GUARD_MARKER", &marker)
        .env("NOXDECK_GUARD_TRAY", tray.to_string())
        .env("NOXDECK_GUARD_FIRST", "true")
        .creation_flags(0x0800_0000);
    let mut first = command.spawn().unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while !marker.exists() {
        assert!(first.try_wait().unwrap().is_none());
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    command.env("NOXDECK_GUARD_FIRST", "false");
    let second = command.output().unwrap();
    assert!(
        second.status.success(),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
    assert!(first.wait().unwrap().success());
}

#[test]
fn a_second_manual_start_signals_the_existing_instance() {
    pair(false);
}

#[test]
fn a_second_tray_start_does_not_signal_a_window() {
    pair(true);
}
