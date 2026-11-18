//! Хранилище: одна таблица `entries`, одна запись в день.

use anyhow::{Context, Result, ensure};
use chrono::NaiveDate;
use rusqlite::{Connection, OptionalExtension, params};
use std::path::{Path, PathBuf};

use crate::entry::{DATE_FMT, Entry, parse_date};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS entries (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  date        TEXT NOT NULL UNIQUE,
  sleep_hours REAL,
  mood        INTEGER NOT NULL,
  note        TEXT,
  created_at  TEXT NOT NULL,
  updated_at  TEXT NOT NULL
);
";

/// `%APPDATA%\Opora\opora.db`
pub fn default_path() -> Result<PathBuf> {
    let appdata = std::env::var_os("APPDATA").context("переменная APPDATA не задана")?;
    Ok(PathBuf::from(appdata).join("Opora").join("opora.db"))
}

/// Открывает базу, создавая папку и таблицу при первом запуске.
pub fn open(path: &Path) -> Result<Connection> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("не удалось создать папку {}", dir.display()))?;
    }

    let conn = Connection::open(path)
        .with_context(|| format!("не удалось открыть базу {}", path.display()))?;

    // WAL нужен, чтобы MCP-сервер мог читать базу, пока приложение её пишет.
    let mode: String = conn.query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))?;
    ensure!(mode.eq_ignore_ascii_case("wal"), "WAL не включился: {mode}");

    conn.execute_batch(SCHEMA)?;
    Ok(conn)
}

pub fn open_default() -> Result<Connection> {
    open(&default_path()?)
}

/// Создаёт запись или обновляет уже существующую за эту дату.
pub fn save(conn: &Connection, entry: &Entry) -> Result<()> {
    entry.validate()?;
    let now = chrono::Local::now().to_rfc3339();

    conn.execute(
        "INSERT INTO entries (date, sleep_hours, mood, note, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?5)
         ON CONFLICT(date) DO UPDATE SET
           sleep_hours = excluded.sleep_hours,
           mood        = excluded.mood,
           note        = excluded.note,
           updated_at  = excluded.updated_at",
        params![
            entry.date_str(),
            entry.sleep_hours,
            entry.mood,
            entry.note,
            now
        ],
    )?;
    Ok(())
}

pub fn load(conn: &Connection, date: NaiveDate) -> Result<Option<Entry>> {
    let mut stmt = conn.prepare(
        "SELECT date, sleep_hours, mood, COALESCE(note, '') FROM entries WHERE date = ?1",
    )?;
    let row = stmt
        .query_row([date.format(DATE_FMT).to_string()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<f32>>(1)?,
                row.get::<_, u8>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .optional()?;

    let Some((date_text, sleep_hours, mood, note)) = row else {
        return Ok(None);
    };

    Ok(Some(Entry {
        date: parse_date(&date_text)?,
        sleep_hours,
        mood,
        note,
    }))
}
