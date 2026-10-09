mod common;
use common::*;
use noxdeck_lib::{
    settings::{AfterStart, Settings},
    shell::ShellRequest,
    Core,
};
use std::time::{Duration, Instant};

const A: &str = "aaaa0001";
const MODES: [AfterStart; 3] = [AfterStart::Stay, AfterStart::Tray, AfterStart::Close];

fn fixture_for(mode: AfterStart, seeded: bool) -> Fake {
    let f = Fake::new();
    Settings {
        after_start: mode,
        ..Settings::default()
    }
    .save(&f.paths.data.join("settings.json"))
    .unwrap();
    if seeded {
        f.seed_accounts(&accounts_file(&[account_json(
            A,
            Some("userA@example.com"),
            Some("Alpha#1111"),
            "ready",
        )]));
    }
    f
}
fn actions(f: &Fake) -> Vec<ShellRequest> {
    f.shell
        .requests()
        .into_iter()
        .filter(|r| *r != ShellRequest::TrayChanged)
        .collect()
}
fn completed(f: &Fake, core: &Core, mode: AfterStart, success: bool) {
    wait_idle(core);
    let expected = if !success {
        vec![ShellRequest::ShowWindow]
    } else {
        match mode {
            AfterStart::Stay => vec![],
            AfterStart::Tray => vec![ShellRequest::HideWindow],
            AfterStart::Close => vec![ShellRequest::Exit],
        }
    };
    let deadline = Instant::now() + Duration::from_millis(100);
    while actions(f).len() < expected.len() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(actions(f), expected, "mode={mode:?} success={success}");
}
fn on_start(f: &Fake, outcome: &'static str) {
    let sim = f.sim.clone();
    f.procs.on_start(move || {
        let run = sim.new_run();
        match outcome {
            "success" | "update" | "not_started" | "stop_failed" => {
                run.login(A);
                if outcome != "update" {
                    run.line("InstallState (prometheus): installed=1 playable=1");
                }
            }
            "password" => run.browser("LoginCredential"),
            "code" => run.browser("Challenge"),
            "wrong_account" => run.login("bbbb0002"),
            "no_answer" => {}
            _ => panic!("unknown fixture"),
        }
    });
}

#[test]
fn launcher_outcomes_apply_each_setting_only_after_the_wanted_login() {
    for mode in MODES {
        for outcome in ["success", "password", "code", "no_answer", "wrong_account"] {
            let f = fixture_for(mode, true);
            on_start(&f, outcome);
            let core = f.core();
            f.shell.clear();
            core.start_launcher(A).unwrap();
            completed(&f, &core, mode, outcome == "success");
        }
    }
}

#[test]
fn game_outcomes_apply_each_setting_only_after_game_start_and_launcher_exit() {
    for mode in MODES {
        for outcome in [
            "success",
            "password",
            "code",
            "no_answer",
            "update",
            "not_started",
            "stop_failed",
        ] {
            let f = fixture_for(mode, true);
            on_start(&f, outcome);
            if outcome == "success" || outcome == "stop_failed" {
                let p = f.procs.clone();
                f.procs.on_launch(move || {
                    p.set_game(true);
                    if outcome == "stop_failed" {
                        p.deny_stop(true);
                    }
                });
            }
            let core = f.core();
            f.shell.clear();
            core.start_game(A, false).unwrap();
            completed(&f, &core, mode, outcome == "success");
            if outcome == "success" {
                assert_eq!(core.state().launcher, noxdeck_lib::LauncherKind::Closed);
            }
        }
    }
}

#[test]
fn synchronous_stop_failures_show_the_window_for_every_setting() {
    for mode in MODES {
        for game in [false, true] {
            let f = fixture_for(mode, true);
            f.procs.set_running(true);
            f.procs.deny_stop(true);
            let core = f.core();
            f.shell.clear();
            let result = if game {
                core.start_game(A, false)
            } else {
                core.start_launcher(A)
            };
            assert!(result.is_err());
            completed(&f, &core, mode, false);
            let f = fixture_for(mode, true);
            f.procs.set_game(true);
            f.procs.deny_game_stop(true);
            let core = f.core();
            f.shell.clear();
            assert!(core.start_game(A, true).is_err());
            completed(&f, &core, mode, false);
        }
    }
}

#[test]
fn add_success_cancel_and_save_current_never_request_window_changes() {
    for mode in MODES {
        let f = fixture_for(mode, false);
        on_start(&f, "success");
        let core = f.core();
        f.shell.clear();
        core.add_account().unwrap();
        wait_idle(&core);
        assert!(actions(&f).is_empty());
        let f = fixture_for(mode, false);
        on_start(&f, "no_answer");
        let core = f.core();
        f.shell.clear();
        core.add_account().unwrap();
        wait_for(&core, "add is cancellable", |s| {
            s.operation.as_ref().is_some_and(|o| o.cancellable)
        });
        core.cancel_operation();
        wait_idle(&core);
        assert!(actions(&f).is_empty());
        let f = fixture_for(mode, false);
        f.sim.new_run().login(A);
        f.procs.set_running(true);
        let core = f.core();
        f.shell.clear();
        core.save_current().unwrap();
        assert!(actions(&f).is_empty());
    }
}
