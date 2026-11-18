use chrono::NaiveDate;
use opora::db;
use opora::entry::Entry;

fn date(day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, day).unwrap()
}

fn counted(conn: &rusqlite::Connection) -> i64 {
    conn.query_row("SELECT COUNT(*) FROM entries", [], |row| row.get(0))
        .unwrap()
}

#[test]
fn open_creates_dir_and_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested").join("opora.db");

    let conn = db::open(&path).unwrap();
    drop(conn);

    assert!(path.exists(), "файл базы не создан");
}

#[test]
fn open_is_idempotent_and_enables_wal() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("opora.db");

    db::open(&path).unwrap();
    let conn = db::open(&path).unwrap();

    let mode: String = conn
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .unwrap();
    assert_eq!(mode.to_lowercase(), "wal");

    let table: String = conn
        .query_row(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'entries'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(table, "entries");
}

#[test]
fn load_returns_none_for_empty_day() {
    let dir = tempfile::tempdir().unwrap();
    let conn = db::open(&dir.path().join("opora.db")).unwrap();

    assert!(db::load(&conn, date(18)).unwrap().is_none());
}

#[test]
fn save_then_load_returns_same_values() {
    let dir = tempfile::tempdir().unwrap();
    let conn = db::open(&dir.path().join("opora.db")).unwrap();
    let entry = Entry::new(date(18), Some(7.5), 6, "работал, устал").unwrap();

    db::save(&conn, &entry).unwrap();

    assert_eq!(db::load(&conn, date(18)).unwrap(), Some(entry));
}

#[test]
fn second_save_overwrites_same_day() {
    let dir = tempfile::tempdir().unwrap();
    let conn = db::open(&dir.path().join("opora.db")).unwrap();

    db::save(
        &conn,
        &Entry::new(date(18), Some(7.5), 6, "первый").unwrap(),
    )
    .unwrap();
    let created: String = conn
        .query_row("SELECT created_at FROM entries", [], |row| row.get(0))
        .unwrap();

    db::save(&conn, &Entry::new(date(18), None, 3, "второй").unwrap()).unwrap();

    assert_eq!(counted(&conn), 1, "за день должна остаться одна запись");
    let loaded = db::load(&conn, date(18)).unwrap().unwrap();
    assert_eq!(loaded.mood, 3);
    assert_eq!(loaded.note, "второй");
    assert_eq!(loaded.sleep_hours, None);

    let created_after: String = conn
        .query_row("SELECT created_at FROM entries", [], |row| row.get(0))
        .unwrap();
    assert_eq!(created, created_after, "created_at не должен меняться");
}

#[test]
fn days_are_independent() {
    let dir = tempfile::tempdir().unwrap();
    let conn = db::open(&dir.path().join("opora.db")).unwrap();

    db::save(&conn, &Entry::new(date(17), Some(6.0), 4, "").unwrap()).unwrap();
    db::save(&conn, &Entry::new(date(18), Some(8.0), 7, "").unwrap()).unwrap();

    assert_eq!(counted(&conn), 2);
    assert_eq!(db::load(&conn, date(17)).unwrap().unwrap().mood, 4);
    assert_eq!(db::load(&conn, date(18)).unwrap().unwrap().mood, 7);
}
