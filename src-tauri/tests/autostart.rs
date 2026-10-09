use noxdeck_lib::shell::autostart::Entry;
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
static NEXT: AtomicU32 = AtomicU32::new(0);
struct TestEntry(Entry);
impl TestEntry {
    fn new() -> Self {
        assert!(std::env::var("NOXDECK_AUTOSTART_NAME")
            .unwrap()
            .starts_with("NoxDeck-Test-"));
        assert!(!std::env::var("NOXDECK_INSTANCE").unwrap().is_empty());
        let name = format!(
            "NoxDeck-Test-registry-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let entry = Entry::new(&name);
        assert_eq!(entry.read().unwrap(), None);
        Self(entry)
    }
}
impl Drop for TestEntry {
    fn drop(&mut self) {
        self.0.set(false, Path::new("")).unwrap();
        assert_eq!(self.0.read().unwrap(), None);
    }
}
const EXE: &str = r"D:\NoxDeck test stand-in\не существует\noxdeck.exe";

#[test]
fn set_read_and_clear_use_only_the_scoped_value_and_quote_the_path() {
    let entry = TestEntry::new();
    assert!(!Path::new(EXE).exists());
    entry.0.set(true, Path::new(EXE)).unwrap();
    assert_eq!(
        entry.0.read().unwrap().as_deref(),
        Some(format!("\"{EXE}\" --tray").as_str())
    );
    entry.0.set(false, Path::new(EXE)).unwrap();
    assert_eq!(entry.0.read().unwrap(), None);
    entry.0.set(false, Path::new(EXE)).unwrap();
}

#[test]
fn repair_updates_a_moved_executable_and_never_enables_an_absent_entry() {
    let entry = TestEntry::new();
    entry.0.repair(Path::new(EXE)).unwrap();
    assert_eq!(entry.0.read().unwrap(), None);
    entry
        .0
        .set(true, Path::new(r"D:\NoxDeck old stand-in\noxdeck.exe"))
        .unwrap();
    entry.0.repair(Path::new(EXE)).unwrap();
    assert_eq!(entry.0.read().unwrap(), Some(format!("\"{EXE}\" --tray")));
    entry.0.repair(Path::new(EXE)).unwrap();
    assert_eq!(entry.0.read().unwrap(), Some(format!("\"{EXE}\" --tray")));
}
