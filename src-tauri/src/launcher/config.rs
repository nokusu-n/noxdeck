//! `Battle.net.config`: find the install, read and replace `SavedAccountNames`.

use super::{Processes, CLIENT_EXE, LAUNCHER_EXE};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, PartialEq, Eq)]
pub enum ConfigError {
    /// The config has no `SavedAccountNames` value to replace.
    KeyMissing,
    /// A launcher process runs; the config is not ours to touch.
    LauncherRunning,
    Io(String),
}

fn read_json(config: &Path) -> Option<serde_json::Value> {
    let bytes = fs::read(config).ok()?;
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(&bytes);
    serde_json::from_slice(bytes).ok()
}

/// The install directory: `Path` of the one top-level object that has it, provided
/// both launcher executables are there.
pub fn find_install(config: &Path) -> Option<PathBuf> {
    let json = read_json(config)?;
    for (_, v) in json.as_object()? {
        if let Some(p) = v.get("Path").and_then(|p| p.as_str()) {
            let dir = PathBuf::from(p);
            if dir.join(LAUNCHER_EXE).is_file() && dir.join(CLIENT_EXE).is_file() {
                return Some(dir);
            }
        }
    }
    None
}

/// The whole comma-separated value; `None` when the key is missing.
pub fn read_saved_names(config: &Path) -> Option<String> {
    let json = read_json(config)?;
    json.get("Client")?.get("SavedAccountNames")?.as_str().map(str::to_owned)
}

/// The first e-mail of the saved list, if any.
pub fn first_saved_name(config: &Path) -> Option<String> {
    let all = read_saved_names(config)?;
    let first = all.split(',').next()?.trim();
    (!first.is_empty()).then(|| first.to_owned())
}

/// One copy of the original config, made before the first write and never replaced.
pub fn ensure_backup(config: &Path, data: &Path) -> io::Result<()> {
    let target = data.join("backup").join("Battle.net.config.original");
    if target.exists() {
        return Ok(());
    }
    fs::create_dir_all(target.parent().unwrap())?;
    fs::copy(config, &target).map(|_| ())
}

fn escape(value: &str) -> String {
    let mut out = String::new();
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Replace the text between the quotes of `"SavedAccountNames": "..."`; every other
/// byte stays as it is.
fn replace_value(text: &str, value: &str) -> Option<String> {
    const KEY: &str = "\"SavedAccountNames\"";
    let key = text.find(KEY)? + KEY.len();
    let rest = &text[key..];
    let colon = rest.find(':')?;
    if !rest[..colon].trim().is_empty() {
        return None;
    }
    let after = &rest[colon + 1..];
    let open = after.find('"')?;
    if !after[..open].trim().is_empty() {
        return None;
    }
    let body = &after[open + 1..];
    let mut end = None;
    let mut escaped = false;
    for (i, c) in body.char_indices() {
        if escaped {
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == '"' {
            end = Some(i);
            break;
        }
    }
    let end = end?;
    let start = key + colon + 1 + open + 1;
    let mut out = String::with_capacity(text.len() + value.len());
    out.push_str(&text[..start]);
    out.push_str(&escape(value));
    out.push_str(&text[start + end..]);
    Some(out)
}

/// The only write path to the launcher's files. Backs the original up first,
/// refuses while a launcher process runs, and checks the result parses with the
/// wanted value, putting the old bytes back otherwise.
pub fn write_saved_names(
    config: &Path,
    data: &Path,
    install: &Path,
    processes: &dyn Processes,
    value: &str,
) -> Result<(), ConfigError> {
    let io_err = |e: io::Error| ConfigError::Io(e.to_string());
    let original = fs::read(config).map_err(io_err)?;
    let text = String::from_utf8(original.clone()).map_err(|e| ConfigError::Io(e.to_string()))?;
    let updated = replace_value(&text, value).ok_or(ConfigError::KeyMissing)?;
    ensure_backup(config, data).map_err(io_err)?;
    if processes.launcher_running(install) {
        return Err(ConfigError::LauncherRunning);
    }
    fs::write(config, updated.as_bytes()).map_err(io_err)?;
    if read_saved_names(config).as_deref() != Some(value) {
        fs::write(config, &original).map_err(io_err)?;
        return Err(ConfigError::Io("the written value did not read back".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_only_the_value() {
        let text = "{\r\n  \"Client\": {\r\n    \"SavedAccountNames\": \"a@x,b@x\",\r\n    \"AutoLogin\": \"true\"\r\n  }\r\n}\r\n";
        let out = replace_value(text, "c@x").unwrap();
        assert_eq!(out, text.replace("a@x,b@x", "c@x"));
        let empty = replace_value(text, "").unwrap();
        assert!(empty.contains("\"SavedAccountNames\": \"\","));
    }

    #[test]
    fn missing_key_is_none() {
        assert!(replace_value("{\"Client\":{}}", "x").is_none());
    }
}
