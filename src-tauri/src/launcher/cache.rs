//! `CachedData.db`, table `login_cache` only: the BattleTag of an account.

use rusqlite::{Connection, OpenFlags};
use std::path::Path;

fn query(db: &Path, id_lo: i64) -> rusqlite::Result<Option<String>> {
    let conn = Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let mut stmt =
        conn.prepare("select battle_tag from login_cache where account_id_lo = ?1 limit 1")?;
    let mut rows = stmt.query([id_lo])?;
    match rows.next()? {
        Some(row) => Ok(Some(row.get(0)?)),
        None => Ok(None),
    }
}

/// The BattleTag of the account whose ID is `id` (eight hex digits), if the launcher
/// has cached one. On a lock error the file is copied and the copy is read.
pub fn battle_tag(db: &Path, id: &str) -> Option<String> {
    let id_lo = i64::from_str_radix(id, 16).ok()?;
    if !db.is_file() {
        return None;
    }
    match query(db, id_lo) {
        Ok(tag) => tag,
        Err(_) => {
            let dir = std::env::temp_dir().join(format!("noxdeck-cache-{}", std::process::id()));
            std::fs::create_dir_all(&dir).ok()?;
            let copy = dir.join("CachedData.db");
            std::fs::copy(db, &copy).ok()?;
            let tag = query(&copy, id_lo).ok().flatten();
            let _ = std::fs::remove_dir_all(&dir);
            tag
        }
    }
}
