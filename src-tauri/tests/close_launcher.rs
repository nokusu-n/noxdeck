mod common;
use common::*;
use noxdeck_lib::{settings::{Settings, AfterStart}, shell::ShellRequest, Phase};
use std::sync::Arc;

const ID: &str = "aaaa0001";
fn seeded(close: bool, after: AfterStart) -> Fake {
    let f = Fake::new();
    f.seed_accounts(&accounts_file(&[account_json(ID, Some("placeholder@example.com"), None, "ready")]));
    Settings { close_launcher: close, after_start: after, ..Settings::default() }.save(&f.paths.data.join("settings.json")).unwrap();
    f
}
fn login(run: &Run, playable: bool) {
    run.login(ID);
    run.line(if playable { "InstallState (prometheus): installed=1 playable=1" } else { "InstallState (prometheus): installed=1 playable=0" });
}

#[test]
fn close_launcher_defaults_on_for_old_settings_and_the_command_round_trips() {
    let f=Fake::new();
    std::fs::write(f.paths.data.join("settings.json"),r#"{"schema":1,"sort":"last_login","dismissed":{"remember_login":false,"logout_warning":false},"after_start":"stay","show_game_start":true,"language":"en"}"#).unwrap();
    assert!(Settings::default().close_launcher);
    let core=f.core(); assert!(core.state().settings.close_launcher);
    for value in [false,true] {
        core.set_close_launcher(value);
        assert_eq!(core.state().settings.close_launcher,value);
        assert_eq!(Settings::load(&f.paths.data.join("settings.json")).0.close_launcher,value);
        assert_eq!(f.core().state().settings.close_launcher,value);
    }
}

#[test]
fn success_covers_close_on_off_launcher_stays_exits_and_reuse() {
    for close in [true,false] { for exits in [false,true] { for reuse in [false,true] {
        let f=seeded(close,AfterStart::Stay);
        let before=f.config_bytes();
        if reuse { login(&f.sim.new_run(),true); f.procs.set_running(true); }
        else { let sim=f.sim.clone(); f.procs.on_start(move || login(&sim.new_run(),true)); }
        let procs=Arc::clone(&f.procs);
        f.procs.on_launch(move || { procs.set_game(true); if exits { procs.set_running(false); } });
        let core=f.core(); core.start_game(ID,false).unwrap(); wait_idle(&core);
        let state=core.state(); let calls=f.procs.calls();
        assert!(state.game_running,"close={close}, exits={exits}, reuse={reuse}");
        assert_eq!(state.accounts[0].last_outcome,Some("game_started"));
        assert_eq!(calls.iter().any(|c|c=="stop_launcher"),close && !exits);
        assert_eq!(calls.iter().any(|c|c=="hide_windows"),!reuse);
        assert!(!calls.iter().any(|c|c=="show_windows"),"successful launch must not activate a window");
        assert_eq!(calls.iter().filter(|c|c.as_str()=="show_windows_no_activate").count(),usize::from(!close && !reuse && !exits));
        assert_eq!(state.launcher==noxdeck_lib::LauncherKind::LoggedIn,!close && !exits);
        if !close && !exits { assert!(!f.procs.is_hidden()); }
        if reuse { assert_eq!(f.config_bytes(),before); assert!(!calls.iter().any(|c|c=="start_launcher")); }
        let closing=f.states.lock().unwrap().iter().any(|s|s.operation.as_ref().is_some_and(|o|o.phase==Phase::ClosingLauncher));
        assert_eq!(closing,close);
    } } }
}

#[test]
fn failures_still_reveal_the_launcher_for_either_setting() {
    for close in [true,false] { for playable in [false,true] {
        let f=seeded(close,AfterStart::Close);
        let sim=f.sim.clone(); f.procs.on_start(move ||login(&sim.new_run(),playable));
        let core=f.core(); core.start_game(ID,false).unwrap(); wait_idle(&core);
        assert_eq!(core.state().accounts[0].last_outcome,Some(if playable {"game_not_started"}else{"game_needs_update"}));
        let calls=f.procs.calls();
        assert_eq!(calls.iter().filter(|c|c.as_str()=="show_windows").count(),1);
        assert!(!calls.iter().any(|c|c=="show_windows_no_activate"));
        assert!(!f.procs.is_hidden());
        wait_for(&core,"failed start shows Noxdeck", |_|f.shell.requests().contains(&ShellRequest::ShowWindow));
    } }
}

#[test]
fn keeping_the_launcher_still_applies_each_after_start_setting() {
    for (after,request) in [(AfterStart::Stay,None),(AfterStart::Tray,Some(ShellRequest::HideWindow)),(AfterStart::Close,Some(ShellRequest::Exit))] {
        let f=seeded(false,after); let sim=f.sim.clone(); f.procs.on_start(move ||login(&sim.new_run(),true));
        let procs=f.procs.clone(); f.procs.on_launch(move ||procs.set_game(true));
        let core=f.core(); core.start_game(ID,false).unwrap(); wait_idle(&core);
        if let Some(request)=request { wait_for(&core,"after-start shell action", |_|f.shell.requests().contains(&request)); }
        assert_eq!(f.shell.requests().into_iter().filter(|r|*r!=ShellRequest::TrayChanged).collect::<Vec<_>>(),request.into_iter().collect::<Vec<_>>());
    }
}
