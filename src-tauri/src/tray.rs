//! The menu is derived from the state; no desktop API or stored menu state.
use crate::{
    lock, settings::Language, shell::ShellRequest, Ask, Core, LauncherKind, Refusal, State,
};
use std::sync::Arc;

#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub struct TrayItem {
    pub id: String,
    pub label: String,
    pub enabled: bool,
}

pub fn menu(state: &State) -> Vec<TrayItem> {
    let ru = state.lang == Language::Ru;
    let mut items = vec![TrayItem {
        id: "open".into(),
        label: if ru {
            "Открыть Noxdeck"
        } else {
            "Open Noxdeck"
        }
        .into(),
        enabled: true,
    }];
    let mut accounts: Vec<_> = state.accounts.iter().collect();
    accounts.sort_by(|a, b| b.last_login_at.cmp(&a.last_login_at));
    for a in accounts.into_iter().take(5) {
        let mut label = a
            .battletag
            .as_deref()
            .or(a.email.as_deref())
            .unwrap_or("—")
            .to_owned();
        if a.is_in_game {
            label.push_str(if ru {
                " — в игре"
            } else {
                " — in game"
            });
        } else if a.is_current && !state.settings.show_game_start {
            label.push_str(if ru {
                " — в Battle.net"
            } else {
                " — in Battle.net"
            });
        }
        items.push(TrayItem {
            id: format!("acc-{}", a.id),
            label,
            enabled: state.operation.is_none()
                && state.launcher != LauncherKind::NotFound
                && a.email.is_some()
                && !a.is_in_game
                && (state.settings.show_game_start || !a.is_current),
        });
    }
    items.push(TrayItem {
        id: "exit".into(),
        label: if ru { "Выход" } else { "Exit" }.into(),
        enabled: true,
    });
    items
}

pub fn click(core: &Arc<Core>, item: &str) {
    match item {
        "open" => core.shell.request(ShellRequest::ShowWindow),
        "exit" => core.shell.request(ShellRequest::Exit),
        _ => {
            let Some(id) = item.strip_prefix("acc-") else {
                return;
            };
            let result = if core.state().settings.show_game_start {
                core.start_game(id, false)
            } else {
                core.start_launcher(id)
            };
            if let Err(refusal) = result {
                if refusal == Refusal::GameRunning {
                    lock(&core.inner).ask = Some(Ask::CloseGame {
                        account_id: id.into(),
                    });
                    core.publish();
                }
                core.shell.request(ShellRequest::ShowWindow);
            }
        }
    }
}

impl Core {
    pub fn clear_ask(&self) {
        lock(&self.inner).ask = None;
        self.publish();
    }
}
