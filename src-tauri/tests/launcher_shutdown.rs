mod common;
use common::*;
use noxdeck_lib::{launcher::ProcessError, Refusal, Timing};

#[test]
fn start_reports_observed_stop_failure_and_changes_no_config() {
    for (error, refusal) in [(ProcessError::AccessDenied, Refusal::LauncherNotClosable), (ProcessError::TimedOut, Refusal::LauncherCloseTimeout), (ProcessError::System(87), Refusal::LauncherCloseTimeout)] {
        let f = Fake::new();
        f.seed_accounts(&accounts_file(&[account_json("aaaa0001", Some("placeholder@example.com"), None, "ready")]));
        f.procs.set_running(true);
        f.procs.fail_stop(error);
        let before = f.config_bytes();
        let core = f.core();
        assert_eq!(core.start_launcher("aaaa0001"), Err(refusal));
        assert_eq!(f.config_bytes(), before);
        assert!(!f.procs.calls().iter().any(|call| call == "start_launcher"));
        assert!(core.state().operation.is_none());
    }
    assert_eq!(Timing::default().stop, std::time::Duration::from_secs(30));
}
