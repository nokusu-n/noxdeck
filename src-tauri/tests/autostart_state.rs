mod common;
use common::*;
use noxdeck_lib::{
    shell::{Shell, ShellRequest},
    Core, Refusal,
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

#[derive(Default)]
struct Startup {
    present: AtomicBool,
    fail: AtomicBool,
}
impl Shell for Startup {
    fn request(&self, _: ShellRequest) {}
    fn autostart(&self) -> std::io::Result<bool> {
        Ok(self.present.load(Ordering::SeqCst))
    }
    fn set_autostart(&self, value: bool) -> std::io::Result<()> {
        if self.fail.load(Ordering::SeqCst) {
            return Err(std::io::ErrorKind::PermissionDenied.into());
        }
        self.present.store(value, Ordering::SeqCst);
        Ok(())
    }
}
fn core(f: &Fake, startup: &Arc<Startup>) -> Arc<Core> {
    Core::new(
        f.paths.clone(),
        fast(),
        f.procs.clone(),
        Box::new(|_| {}),
        "test",
        startup.clone(),
    )
}
#[test]
fn startup_state_reads_the_shell_and_never_persists_a_claim_in_settings() {
    let f = Fake::new();
    let startup = Arc::new(Startup::default());
    startup.present.store(true, Ordering::SeqCst);
    let core = core(&f, &startup);
    assert!(core.state().settings.autostart);
    core.set_autostart(false).unwrap();
    assert!(!core.state().settings.autostart);
    core.set_autostart(true).unwrap();
    assert!(core.state().settings.autostart);
    assert!(!f.paths.data.join("settings.json").exists());
    startup.present.store(false, Ordering::SeqCst);
    core.tick();
    assert!(!core.state().settings.autostart);
}
#[test]
fn a_failed_registration_returns_a_reason_and_keeps_the_actual_presence() {
    let f = Fake::new();
    let startup = Arc::new(Startup::default());
    startup.fail.store(true, Ordering::SeqCst);
    let core = core(&f, &startup);
    assert_eq!(core.set_autostart(true), Err(Refusal::AutostartFailed));
    assert!(!core.state().settings.autostart);
}
