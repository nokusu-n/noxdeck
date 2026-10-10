mod common;
use common::*;
use noxdeck_lib::{settings::{mask_email, Settings}, registry::Registry, tray};

#[test]
fn mask_has_a_fixed_length_and_handles_short_unicode_and_invalid_values() {
    for (email, expected) in [
        ("kestrel.mail@example.com", "ke****@example.com"),
        ("ab@example.com", "a****@example.com"), ("a@example.com", "a****@example.com"),
        ("abc@example.com", "ab****@example.com"), ("@example.com", "****@example.com"),
        ("ёжик@example.com", "ёж****@example.com"), ("ёж@example.com", "ё****@example.com"),
        ("not-an-email", "****"), ("", "****"),
    ] { assert_eq!(mask_email(email), expected); }
}

#[test]
fn state_and_tray_mask_immediately_but_storage_and_start_keep_the_real_email() {
    let f = Fake::new();
    let email = "placeholder@example.com";
    f.seed_accounts(&accounts_file(&[
        account_json("aaaa0001", Some(email), None, "ready"),
        account_json("aaaa0002", Some("second@example.com"), Some("ExamplePlayer#12345"), "ready"),
        account_json("aaaa0003", None, None, "ready"),
    ]));
    let core = f.core();
    for hidden in [true, false] {
        core.set_hide_emails(hidden);
        let state = core.state();
        let expected = if hidden { "pl****@example.com" } else { email };
        assert_eq!(account(&state, "aaaa0001").email.as_deref(), Some(expected));
        assert_eq!(tray::menu(&state)[1].label, expected);
        assert_eq!(tray::menu(&state)[2].label, "ExamplePlayer#12345");
        assert!(!tray::menu(&state)[3].enabled);
        if hidden { assert!(!serde_json::to_string(&state).unwrap().contains(email)); }
        assert_eq!(Registry::load(&f.paths.data.join("accounts.json")).0.find("aaaa0001").unwrap().email.as_deref(), Some(email));
        assert_eq!(f.core().state().settings.hide_emails, hidden);
    }
    core.set_hide_emails(true);
    let sim = f.sim.clone();
    f.procs.on_start(move || sim.new_run().login("aaaa0001"));
    core.start_launcher("aaaa0001").unwrap();
    wait_idle(&core);
    assert_eq!(f.sim.list().as_deref(), Some(email));
}

#[test]
fn old_settings_default_off_and_both_values_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("settings.json");
    std::fs::write(&file, r#"{"schema":1,"sort":"last_login","dismissed":{"remember_login":false,"logout_warning":false}}"#).unwrap();
    let (mut settings, reset) = Settings::load(&file);
    assert!(!reset && !settings.hide_emails);
    for value in [true, false] {
        settings.hide_emails = value;
        settings.save(&file).unwrap();
        assert_eq!(Settings::load(&file).0.hide_emails, value);
    }
}
