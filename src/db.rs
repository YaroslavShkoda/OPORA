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

CREATE TABLE IF NOT EXISTS items (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  section     TEXT NOT NULL,
  title       TEXT NOT NULL DEFAULT '',
  text        TEXT NOT NULL DEFAULT '',
  done        INTEGER NOT NULL DEFAULT 0,
  created_at  TEXT NOT NULL,
  updated_at  TEXT NOT NULL
);
";

/// Строка заметки, дела или цели. `section` — строка, чтобы `db` не зависел
/// от перечисления разделов из интерфейса.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredItem {
    pub id: i64,
    pub section: String,
    pub title: String,
    pub text: String,
    pub done: bool,
}

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

fn row_columns(row: &rusqlite::Row<'_>) -> rusqlite::Result<(String, Option<f32>, u8, String)> {
    Ok((
        row.get::<_, String>(0)?,
        row.get::<_, Option<f32>>(1)?,
        row.get::<_, u8>(2)?,
        row.get::<_, String>(3)?,
    ))
}

fn entry_from_columns(
    (date_text, sleep_hours, mood, note): (String, Option<f32>, u8, String),
) -> Result<Entry> {
    Ok(Entry {
        date: parse_date(&date_text)?,
        sleep_hours,
        mood,
        note,
    })
}

pub fn load(conn: &Connection, date: NaiveDate) -> Result<Option<Entry>> {
    let mut stmt = conn.prepare(
        "SELECT date, sleep_hours, mood, COALESCE(note, '') FROM entries WHERE date = ?1",
    )?;
    let row = stmt
        .query_row([date.format(DATE_FMT).to_string()], row_columns)
        .optional()?;

    row.map(entry_from_columns).transpose()
}

/// Удаляет запись за день. `true`, если строка была.
pub fn delete(conn: &Connection, date: NaiveDate) -> Result<bool> {
    let removed = conn.execute(
        "DELETE FROM entries WHERE date = ?1",
        [date.format(DATE_FMT).to_string()],
    )?;
    Ok(removed > 0)
}

/// Последние `limit` записей, новые сверху.
pub fn recent(conn: &Connection, limit: usize) -> Result<Vec<Entry>> {
    let mut stmt = conn.prepare(
        "SELECT date, sleep_hours, mood, COALESCE(note, '') FROM entries
         ORDER BY date DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map([limit as i64], row_columns)?;

    let mut entries = Vec::new();
    for row in rows {
        entries.push(entry_from_columns(row?)?);
    }
    Ok(entries)
}

/// Все заметки, дела и цели в порядке создания.
pub fn items(conn: &Connection) -> Result<Vec<StoredItem>> {
    let mut stmt = conn.prepare(
        "SELECT id, section, COALESCE(title, ''), COALESCE(text, ''), done
         FROM items ORDER BY id",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(StoredItem {
            id: row.get(0)?,
            section: row.get(1)?,
            title: row.get(2)?,
            text: row.get(3)?,
            done: row.get(4)?,
        })
    })?;

    let mut items = Vec::new();
    for row in rows {
        items.push(row?);
    }
    Ok(items)
}

/// Создаёт строку и возвращает её id.
pub fn insert_item(
    conn: &Connection,
    section: &str,
    title: &str,
    text: &str,
    done: bool,
) -> Result<i64> {
    let now = chrono::Local::now().to_rfc3339();
    conn.execute(
        "INSERT INTO items (section, title, text, done, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
        params![section, title, text, done, now],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn update_item(conn: &Connection, id: i64, title: &str, text: &str, done: bool) -> Result<()> {
    let now = chrono::Local::now().to_rfc3339();
    conn.execute(
        "UPDATE items SET title = ?1, text = ?2, done = ?3, updated_at = ?4 WHERE id = ?5",
        params![title, text, done, now, id],
    )?;
    Ok(())
}

pub fn delete_item(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM items WHERE id = ?1", [id])?;
    Ok(())
}
