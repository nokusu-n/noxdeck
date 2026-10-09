mod common;
use common::*;
use noxdeck_lib::{settings::{Settings, Sort}, shell::ShellRequest};

#[test]
fn a_state_change_updates_the_tray_but_an_unchanged_poll_does_not() {
    let fake = Fake::new();
    // Keep this publish-deduplication check independent of background refresh.
    Settings { auto_ratings: false, ..Settings::default() }.save(&fake.paths.data.join("settings.json")).unwrap();
    let core = fake.core();
    assert_eq!(fake.shell.requests(), vec![ShellRequest::TrayChanged]);
    fake.shell.clear();
    core.publish();
    assert!(fake.shell.requests().is_empty());
    core.set_sort(Sort::Battletag);
    assert_eq!(fake.shell.requests(), vec![ShellRequest::TrayChanged]);
    fake.shell.clear();
    core.set_sort(Sort::Battletag);
    assert!(fake.shell.requests().is_empty());
}
