use crate::types::{Activity, Carousel, Entry, Slide};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use uuid::Uuid;

pub const DB_FILENAME: &str = "db.sqlite";

pub fn open(base_dir: &Path) -> Result<Connection> {
    std::fs::create_dir_all(base_dir)
        .with_context(|| format!("creating db dir {}", base_dir.display()))?;
    let path = base_dir.join(DB_FILENAME);
    let conn = Connection::open(&path)
        .with_context(|| format!("opening sqlite at {}", path.display()))?;
    migrate(&conn)?;
    Ok(conn)
}

pub fn open_in_memory() -> Result<Connection> {
    let conn = Connection::open_in_memory().context("opening in-memory sqlite")?;
    migrate(&conn)?;
    Ok(conn)
}

pub fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        PRAGMA foreign_keys = ON;

        CREATE TABLE IF NOT EXISTS entries (
            date TEXT PRIMARY KEY,
            activities_json TEXT NOT NULL,
            thoughts TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS carousels (
            id         TEXT PRIMARY KEY,
            label      TEXT NOT NULL UNIQUE,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS slides (
            id           TEXT PRIMARY KEY,
            carousel_id  TEXT NOT NULL REFERENCES carousels(id) ON DELETE CASCADE,
            order_index  INTEGER NOT NULL,
            content      TEXT NOT NULL DEFAULT '',
            created_at   TEXT NOT NULL,
            updated_at   TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_slides_carousel ON slides (carousel_id, order_index);
        "#,
    )
    .context("running migrations")?;
    Ok(())
}

pub fn save_entry(
    conn: &Connection,
    date: &str,
    activities: &[Activity],
    thoughts: &str,
) -> Result<()> {
    let activities_json =
        serde_json::to_string(activities).context("serializing activities")?;
    let now = Utc::now().to_rfc3339();
    conn.execute(
        r#"
        INSERT INTO entries (date, activities_json, thoughts, created_at, updated_at)
        VALUES (?1, ?2, ?3, ?4, ?4)
        ON CONFLICT(date) DO UPDATE SET
            activities_json = excluded.activities_json,
            thoughts        = excluded.thoughts,
            updated_at      = excluded.updated_at
        "#,
        params![date, activities_json, thoughts, now],
    )
    .context("saving entry")?;
    Ok(())
}

pub fn load_entry(conn: &Connection, date: &str) -> Result<Option<Entry>> {
    let row = conn
        .query_row(
            r#"
            SELECT date, activities_json, thoughts, created_at, updated_at
            FROM entries
            WHERE date = ?1
            "#,
            params![date],
            |row| {
                let date: String = row.get(0)?;
                let activities_json: String = row.get(1)?;
                let thoughts: Option<String> = row.get(2)?;
                let created_at: String = row.get(3)?;
                let updated_at: String = row.get(4)?;
                Ok((date, activities_json, thoughts, created_at, updated_at))
            },
        )
        .optional()
        .context("querying entry")?;

    let Some((date, activities_json, thoughts, created_at, updated_at)) = row else {
        return Ok(None);
    };

    let activities: Vec<Activity> = serde_json::from_str(&activities_json)
        .context("deserializing activities")?;
    let created_at: DateTime<Utc> = DateTime::parse_from_rfc3339(&created_at)
        .context("parsing created_at")?
        .with_timezone(&Utc);
    let updated_at: DateTime<Utc> = DateTime::parse_from_rfc3339(&updated_at)
        .context("parsing updated_at")?
        .with_timezone(&Utc);

    Ok(Some(Entry {
        date,
        activities,
        thoughts: thoughts.unwrap_or_default(),
        created_at,
        updated_at,
    }))
}

fn parse_rfc3339(s: &str) -> Result<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(s)
        .context("parsing rfc3339 timestamp")?
        .with_timezone(&Utc))
}

pub fn list_carousels(conn: &Connection) -> Result<Vec<Carousel>> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT id, label, created_at, updated_at
            FROM carousels
            ORDER BY updated_at DESC
            "#,
        )
        .context("preparing list_carousels")?;
    let rows = stmt
        .query_map([], |row| {
            let id: String = row.get(0)?;
            let label: String = row.get(1)?;
            let created_at: String = row.get(2)?;
            let updated_at: String = row.get(3)?;
            Ok((id, label, created_at, updated_at))
        })
        .context("querying carousels")?;
    let mut out = Vec::new();
    for r in rows {
        let (id, label, created_at, updated_at) = r.context("reading carousel row")?;
        out.push(Carousel {
            id,
            label,
            created_at: parse_rfc3339(&created_at)?,
            updated_at: parse_rfc3339(&updated_at)?,
        });
    }
    Ok(out)
}

pub fn create_carousel(conn: &Connection, label: &str) -> Result<Carousel> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    conn.execute(
        r#"
        INSERT INTO carousels (id, label, created_at, updated_at)
        VALUES (?1, ?2, ?3, ?3)
        "#,
        params![id, label, now],
    )
    .context("inserting carousel")?;
    Ok(Carousel {
        id,
        label: label.to_string(),
        created_at: parse_rfc3339(&now)?,
        updated_at: parse_rfc3339(&now)?,
    })
}

pub fn rename_carousel(conn: &Connection, id: &str, new_label: &str) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        r#"
        UPDATE carousels
        SET label = ?2, updated_at = ?3
        WHERE id = ?1
        "#,
        params![id, new_label, now],
    )
    .context("renaming carousel")?;
    Ok(())
}

pub fn delete_carousel(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM carousels WHERE id = ?1", params![id])
        .context("deleting carousel")?;
    Ok(())
}

pub fn list_slides(conn: &Connection, carousel_id: &str) -> Result<Vec<Slide>> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT id, carousel_id, order_index, content, created_at, updated_at
            FROM slides
            WHERE carousel_id = ?1
            ORDER BY order_index ASC
            "#,
        )
        .context("preparing list_slides")?;
    let rows = stmt
        .query_map(params![carousel_id], |row| {
            let id: String = row.get(0)?;
            let carousel_id: String = row.get(1)?;
            let order_index: i64 = row.get(2)?;
            let content: String = row.get(3)?;
            let created_at: String = row.get(4)?;
            let updated_at: String = row.get(5)?;
            Ok((id, carousel_id, order_index, content, created_at, updated_at))
        })
        .context("querying slides")?;
    let mut out = Vec::new();
    for r in rows {
        let (id, carousel_id, order_index, content, created_at, updated_at) =
            r.context("reading slide row")?;
        out.push(Slide {
            id,
            carousel_id,
            order_index,
            content,
            created_at: parse_rfc3339(&created_at)?,
            updated_at: parse_rfc3339(&updated_at)?,
        });
    }
    Ok(out)
}

pub fn create_slide(conn: &Connection, carousel_id: &str) -> Result<Slide> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let next_index: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(order_index) + 1, 0) FROM slides WHERE carousel_id = ?1",
            params![carousel_id],
            |row| row.get(0),
        )
        .context("computing next order_index")?;
    conn.execute(
        r#"
        INSERT INTO slides (id, carousel_id, order_index, content, created_at, updated_at)
        VALUES (?1, ?2, ?3, '', ?4, ?4)
        "#,
        params![id, carousel_id, next_index, now],
    )
    .context("inserting slide")?;
    Ok(Slide {
        id,
        carousel_id: carousel_id.to_string(),
        order_index: next_index,
        content: String::new(),
        created_at: parse_rfc3339(&now)?,
        updated_at: parse_rfc3339(&now)?,
    })
}

pub fn update_slide_content(conn: &Connection, id: &str, content: &str) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        r#"
        UPDATE slides
        SET content = ?2, updated_at = ?3
        WHERE id = ?1
        "#,
        params![id, content, now],
    )
    .context("updating slide content")?;
    Ok(())
}

pub fn delete_slide(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM slides WHERE id = ?1", params![id])
        .context("deleting slide")?;
    Ok(())
}

pub fn reorder_slide(conn: &Connection, id: &str, new_index: i64) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        r#"
        UPDATE slides
        SET order_index = ?2, updated_at = ?3
        WHERE id = ?1
        "#,
        params![id, new_index, now],
    )
    .context("reordering slide")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Activity, ActivitySource};
    use chrono::TimeZone;

    fn sample_activities() -> Vec<Activity> {
        vec![
            Activity {
                id: "abc123".into(),
                source: ActivitySource::GitCommit,
                timestamp: Utc.with_ymd_and_hms(2026, 4, 19, 14, 32, 0).unwrap(),
                summary: "Fix date rollover bug".into(),
                chip: "cantalog".into(),
                included: true,
            },
            Activity {
                id: "session-1".into(),
                source: ActivitySource::ClaudeCodeSession,
                timestamp: Utc.with_ymd_and_hms(2026, 4, 19, 10, 12, 0).unwrap(),
                summary: "How do I set up Tauri v2?".into(),
                chip: "cantalog".into(),
                included: false,
            },
        ]
    }

    #[test]
    fn migrate_creates_entries_table() {
        let conn = open_in_memory().unwrap();
        let name: String = conn
            .query_row(
                "SELECT name FROM sqlite_master WHERE type='table' AND name='entries'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(name, "entries");
    }

    #[test]
    fn save_and_load_roundtrip() {
        let conn = open_in_memory().unwrap();
        let activities = sample_activities();
        save_entry(&conn, "2026-04-19", &activities, "Shipped DB layer.").unwrap();

        let loaded = load_entry(&conn, "2026-04-19").unwrap().expect("entry");
        assert_eq!(loaded.date, "2026-04-19");
        assert_eq!(loaded.activities, activities);
        assert_eq!(loaded.thoughts, "Shipped DB layer.");
    }

    #[test]
    fn load_returns_none_when_missing() {
        let conn = open_in_memory().unwrap();
        assert!(load_entry(&conn, "1999-12-31").unwrap().is_none());
    }

    #[test]
    fn overwrite_preserves_created_at() {
        let conn = open_in_memory().unwrap();
        save_entry(&conn, "2026-04-19", &[], "first").unwrap();
        let first = load_entry(&conn, "2026-04-19").unwrap().unwrap();

        std::thread::sleep(std::time::Duration::from_millis(15));
        save_entry(&conn, "2026-04-19", &sample_activities(), "second").unwrap();
        let second = load_entry(&conn, "2026-04-19").unwrap().unwrap();

        assert_eq!(first.created_at, second.created_at);
        assert!(second.updated_at >= first.updated_at);
        assert_eq!(second.thoughts, "second");
        assert_eq!(second.activities.len(), 2);
    }

    #[test]
    fn migrate_is_idempotent() {
        let conn = open_in_memory().unwrap();
        migrate(&conn).unwrap();
        migrate(&conn).unwrap();
        save_entry(&conn, "2026-04-19", &[], "ok").unwrap();
        assert!(load_entry(&conn, "2026-04-19").unwrap().is_some());
    }

    #[test]
    fn carousels_roundtrip() {
        let conn = open_in_memory().unwrap();
        let a = create_carousel(&conn, "Launch week").unwrap();
        let b = create_carousel(&conn, "Weekly recap").unwrap();
        assert_ne!(a.id, b.id);

        let listed = list_carousels(&conn).unwrap();
        assert_eq!(listed.len(), 2);
        assert!(listed.iter().any(|c| c.label == "Launch week"));

        rename_carousel(&conn, &a.id, "Launch week v2").unwrap();
        let after = list_carousels(&conn).unwrap();
        assert!(after.iter().any(|c| c.id == a.id && c.label == "Launch week v2"));

        delete_carousel(&conn, &b.id).unwrap();
        let remaining = list_carousels(&conn).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, a.id);
    }

    #[test]
    fn carousel_label_unique() {
        let conn = open_in_memory().unwrap();
        create_carousel(&conn, "Same").unwrap();
        let err = create_carousel(&conn, "Same");
        assert!(err.is_err(), "expected unique constraint error");
    }

    #[test]
    fn slides_roundtrip() {
        let conn = open_in_memory().unwrap();
        let c = create_carousel(&conn, "Deck").unwrap();

        let s1 = create_slide(&conn, &c.id).unwrap();
        let s2 = create_slide(&conn, &c.id).unwrap();
        let s3 = create_slide(&conn, &c.id).unwrap();
        assert_eq!(s1.order_index, 0);
        assert_eq!(s2.order_index, 1);
        assert_eq!(s3.order_index, 2);

        update_slide_content(&conn, &s2.id, "hello world").unwrap();
        let listed = list_slides(&conn, &c.id).unwrap();
        assert_eq!(listed.len(), 3);
        assert_eq!(listed[0].id, s1.id);
        assert_eq!(listed[1].content, "hello world");

        delete_slide(&conn, &s1.id).unwrap();
        let after = list_slides(&conn, &c.id).unwrap();
        assert_eq!(after.len(), 2);
        assert_eq!(after[0].id, s2.id);
    }

    #[test]
    fn delete_carousel_cascades_slides() {
        let conn = open_in_memory().unwrap();
        let c = create_carousel(&conn, "Doomed").unwrap();
        create_slide(&conn, &c.id).unwrap();
        create_slide(&conn, &c.id).unwrap();
        assert_eq!(list_slides(&conn, &c.id).unwrap().len(), 2);

        delete_carousel(&conn, &c.id).unwrap();
        let orphan_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM slides", [], |r| r.get(0))
            .unwrap();
        assert_eq!(orphan_count, 0);
    }

    #[test]
    fn migrate_is_idempotent_with_new_tables() {
        let conn = open_in_memory().unwrap();
        migrate(&conn).unwrap();
        migrate(&conn).unwrap();
        let c = create_carousel(&conn, "idem").unwrap();
        create_slide(&conn, &c.id).unwrap();
        assert_eq!(list_carousels(&conn).unwrap().len(), 1);
        assert_eq!(list_slides(&conn, &c.id).unwrap().len(), 1);
    }

}
