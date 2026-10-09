mod common;
use common::*;
use noxdeck_lib::{Core, settings::Language, shell::{Shell, ShellRequest}, tray};
use std::sync::{Arc, Mutex};

#[test]
fn the_window_tray_installer_and_publisher_use_the_final_name() {
    let f = Fake::new(); let core = f.core();
    for (language, expected) in [(Language::En,"Open Noxdeck"),(Language::Ru,"Открыть Noxdeck")] {
        core.set_language(Some(language));
        assert_eq!(tray::menu(&core.state())[0].label,expected);
    }
    let config: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
    assert_eq!(config["productName"], "Noxdeck");
    assert_eq!(config["app"]["windows"][0]["title"], "Noxdeck");
    assert_eq!(config["bundle"]["publisher"], "Nokusu.dev");
    assert_eq!(config["identifier"], "app.noxdeck.desktop");
    assert!(include_str!("../src/shell.rs").contains(".tooltip(\"Noxdeck\")"));
}

#[derive(Default)]
struct Opener(Mutex<Vec<String>>);
impl Shell for Opener {
    fn request(&self, _: ShellRequest) {}
    fn open_url(&self, url: &str) -> std::io::Result<()> { self.0.lock().unwrap().push(url.into()); Ok(()) }
}
#[test]
fn repository_command_hands_only_the_fixed_url_to_the_recording_opener() {
    let f=Fake::new(); let opener=Arc::new(Opener::default());
    let core=Core::new(f.paths.clone(),fast(),f.procs.clone(),Box::new(|_|{}),"test",opener.clone());
    core.open_repository().unwrap();
    assert_eq!(*opener.0.lock().unwrap(),vec!["https://github.com/nokusu-n/noxdeck"]);
    assert!(f.procs.calls().is_empty());
}
