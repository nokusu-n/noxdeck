//! Debug-only hooks for process tests; release builds contain none of these.
use crate::Core;
use std::sync::Arc;

pub fn program_test() -> bool {
    std::env::var("NOXDECK_PROGRAM_TEST").as_deref() == Ok("1")
        && std::env::var("NOXDECK_INSTANCE").is_ok_and(|s| s.starts_with("program-"))
        && std::env::var("NOXDECK_AUTOSTART_NAME").is_ok_and(|s| s.starts_with("NoxDeck-Test-"))
}

// The title is observable with the Windows API after React has painted the list.
pub const READY_SCRIPT: &str = r#"
window.addEventListener('DOMContentLoaded', () => {
  let done = false;
  const ready = () => {
    if (!done && document.querySelector('[data-testid^="account-"], [data-testid="empty-state"]')) {
      done = true;
      observer.disconnect();
      requestAnimationFrame(() => requestAnimationFrame(() => { document.title = 'Noxdeck [list ready]'; }));
    }
  };
  const observer = new MutationObserver(ready);
  observer.observe(document.body, { childList: true, subtree: true });
  ready();
});
"#;

pub fn start(core: &Arc<Core>) {
    if !program_test() {
        return;
    }
    let Ok(action) = std::env::var("NOXDECK_TEST_START") else {
        return;
    };
    let Some((kind, id)) = action.split_once(':') else {
        return;
    };
    let core = core.clone();
    let (kind, id) = (kind.to_owned(), id.to_owned());
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(1));
        let result = match kind.as_str() {
            "start" => core.start_launcher(&id),
            "game" => core.start_game(&id, false),
            _ => return,
        };
        assert!(result.is_ok(), "program test action refused: {result:?}");
    });
}
