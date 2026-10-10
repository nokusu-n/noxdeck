//! `settings.json`: sort order and dismissed reminders.

use crate::registry::{load, quarantine, save, Loaded};
use serde::{Deserialize, Serialize};
use std::io;
use std::path::Path;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Sort {
    LastLogin,
    Battletag,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Dismissed {
    pub remember_login: bool,
    pub logout_warning: bool,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AfterStart {
    Stay,
    #[default]
    Tray,
    Close,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Language { Ru, En, It }

fn enabled() -> bool { true }

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    pub schema: u32,
    pub sort: Sort,
    pub dismissed: Dismissed,
    #[serde(default)]
    pub after_start: AfterStart,
    #[serde(default = "enabled")]
    pub show_game_start: bool,
    #[serde(default = "enabled")]
    pub close_launcher: bool,
    #[serde(default = "enabled")]
    pub show_ratings: bool,
    #[serde(default = "enabled")]
    pub auto_ratings: bool,
    #[serde(default)]
    pub hide_emails: bool,
    #[serde(default)]
    pub language: Option<Language>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            schema: 1, sort: Sort::LastLogin, dismissed: Dismissed::default(),
            after_start: AfterStart::Tray, show_game_start: true, language: None,
            close_launcher: true,
            show_ratings: true,
            auto_ratings: true,
            hide_emails: false,
        }
    }
}

pub fn mask_email(email: &str) -> String {
    let Some((local, domain)) = email.split_once('@') else { return "****".into(); };
    let count = if local.chars().count() <= 2 { 1 } else { 2 };
    format!("{}****@{domain}", local.chars().take(count).collect::<String>())
}

impl Settings {
    pub fn resolved_language(&self, locale: &str) -> Language {
        self.language.unwrap_or_else(|| {
            let locale = locale.to_ascii_lowercase();
            if locale.starts_with("ru") { Language::Ru }
            else if locale.starts_with("it") { Language::It }
            else { Language::En }
        })
    }
    /// The stored settings, and whether a broken file was set aside to get them.
    pub fn load(path: &Path) -> (Settings, bool) {
        match load::<Settings>(path) {
            Loaded::Ok(s) if s.schema == 1 => (s, false),
            Loaded::Ok(_) => {
                quarantine(path);
                (Settings::default(), true)
            }
            Loaded::Missing => (Settings::default(), false),
            Loaded::Broken => (Settings::default(), true),
        }
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        save(path, self)
    }
}

pub fn user_locale() -> String {
    if let Ok(locale) = std::env::var("NOXDECK_LOCALE") { return locale; }
    #[link(name = "kernel32")]
    extern "system" { fn GetUserDefaultLocaleName(name: *mut u16, size: i32) -> i32; }
    let mut buffer = [0u16; 85];
    let length = unsafe { GetUserDefaultLocaleName(buffer.as_mut_ptr(), buffer.len() as i32) };
    if length > 0 { String::from_utf16_lossy(&buffer[..length as usize - 1]) } else { String::new() }
}
