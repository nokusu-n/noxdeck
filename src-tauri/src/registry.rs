//! `accounts.json`: the list of accounts the user added, and the small JSON-file
//! helpers `settings.rs` shares.

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) enum Loaded<T> {
    Ok(T),
    Missing,
    Broken,
}

/// Read a JSON file. One that cannot be parsed is renamed to `<name>.broken`.
pub(crate) fn load<T: DeserializeOwned>(path: &Path) -> Loaded<T> {
    let Ok(bytes) = fs::read(path) else {
        return Loaded::Missing;
    };
    match serde_json::from_slice(&bytes) {
        Ok(v) => Loaded::Ok(v),
        Err(_) => {
            quarantine(path);
            Loaded::Broken
        }
    }
}

pub(crate) fn quarantine(path: &Path) {
    let mut broken = path.as_os_str().to_owned();
    broken.push(".broken");
    let _ = fs::remove_file(&broken);
    let _ = fs::rename(path, broken);
}

/// Write to a temporary file beside the target, then rename over it.
pub(crate) fn save<T: Serialize>(path: &Path, value: &T) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(value).map_err(io::Error::other)?)?;
    fs::rename(&tmp, path)
}

/// Current UTC time as `2026-10-09T01:13:12Z`.
pub fn now_iso() -> String {
    iso_time(SystemTime::now())
}

pub fn iso_time(time: SystemTime) -> String {
    let secs = time.duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LoginState {
    Ready,
    PasswordWanted,
    CodeWanted,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Account {
    #[serde(default)]
    pub ratings: crate::ratings::Ratings,
    pub id: String,
    pub email: Option<String>,
    pub battletag: Option<String>,
    pub login_state: LoginState,
    pub last_login_at: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Registry {
    pub schema: u32,
    pub accounts: Vec<Account>,
}

impl Default for Registry {
    fn default() -> Self {
        Registry { schema: 1, accounts: Vec::new() }
    }
}

fn valid(r: &Registry) -> bool {
    r.schema == 1
        && r.accounts.iter().enumerate().all(|(i, a)| {
            a.id.len() == 8
                && a.ratings.valid()
                && a.id.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f'))
                && r.accounts[..i].iter().all(|b| b.id != a.id)
        })
}

impl Registry {
    /// The stored list, and whether a broken file was set aside to get it.
    pub fn load(path: &Path) -> (Registry, bool) {
        match load::<Registry>(path) {
            Loaded::Ok(r) if valid(&r) => (r, false),
            Loaded::Ok(_) => {
                quarantine(path);
                (Registry::default(), true)
            }
            Loaded::Missing => (Registry::default(), false),
            Loaded::Broken => (Registry::default(), true),
        }
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        save(path, self)
    }

    pub fn find(&self, id: &str) -> Option<&Account> {
        self.accounts.iter().find(|a| a.id == id)
    }

    pub fn find_mut(&mut self, id: &str) -> Option<&mut Account> {
        self.accounts.iter_mut().find(|a| a.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_time_has_the_documented_shape() {
        let t = now_iso();
        assert_eq!(t.len(), 20);
        assert!(t.ends_with('Z') && &t[10..11] == "T");
    }
}
