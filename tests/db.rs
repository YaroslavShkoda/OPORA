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

    let table: String = conn
        .query_row(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'items'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(table, "items");
}

#[test]
fn items_roundtrip_and_update() {
    let dir = tempfile::tempdir().unwrap();
    let conn = db::open(&dir.path().join("opora.db")).unwrap();

    let note_id = db::insert_item(&conn, "notes", "Идея", "Текст заметки", false).unwrap();
    let task_id = db::insert_item(&conn, "tasks", "", "Купить хлеб", false).unwrap();

    let rows = db::items(&conn).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows[0],
        db::StoredItem {
            id: note_id,
            section: "notes".to_string(),
            title: "Идея".to_string(),
            text: "Текст заметки".to_string(),
            done: false,
        }
    );
    assert_eq!(rows[1].id, task_id);
    assert_eq!(rows[1].section, "tasks");
    assert_eq!(rows[1].text, "Купить хлеб");

    db::update_item(&conn, note_id, "Новая идея", "Новый текст", false).unwrap();
    db::update_item(&conn, task_id, "", "Купить хлеб", true).unwrap();

    let rows = db::items(&conn).unwrap();
    assert_eq!(rows[0].title, "Новая идея");
    assert_eq!(rows[0].text, "Новый текст");
    assert!(rows[1].done);

    db::delete_item(&conn, note_id).unwrap();
    let rows = db::items(&conn).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, task_id);
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
fn recent_returns_latest_first_and_limits() {
    let dir = tempfile::tempdir().unwrap();
    let conn = db::open(&dir.path().join("opora.db")).unwrap();
    let start = NaiveDate::from_ymd_opt(2026, 10, 1).unwrap();

    for offset in 0..35 {
        let date = start + chrono::Days::new(offset);
        let entry = Entry::new(date, Some(7.0), 5, format!("день {offset}")).unwrap();
        db::save(&conn, &entry).unwrap();
    }

    let recent = db::recent(&conn, 30).unwrap();

    assert_eq!(recent.len(), 30);
    assert_eq!(recent[0].date, start + chrono::Days::new(34));
    assert_eq!(recent[29].date, start + chrono::Days::new(5));
    assert!(
        recent.windows(2).all(|pair| pair[0].date > pair[1].date),
        "список должен идти от нового к старому"
    );
}

#[test]
fn delete_removes_only_that_day() {
    let dir = tempfile::tempdir().unwrap();
    let conn = db::open(&dir.path().join("opora.db")).unwrap();

    db::save(
        &conn,
        &Entry::new(date(17), Some(6.0), 4, "семнадцатое").unwrap(),
    )
    .unwrap();
    db::save(
        &conn,
        &Entry::new(date(18), Some(8.0), 7, "восемнадцатое").unwrap(),
    )
    .unwrap();

    assert!(db::delete(&conn, date(17)).unwrap());
    assert_eq!(counted(&conn), 1);
    assert!(db::load(&conn, date(17)).unwrap().is_none());
    assert!(db::load(&conn, date(18)).unwrap().is_some());

    assert!(
        !db::delete(&conn, date(17)).unwrap(),
        "удалять нечего — false"
    );
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
