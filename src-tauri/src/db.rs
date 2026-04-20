use crate::types::{Activity, Entry};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

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
        CREATE TABLE IF NOT EXISTS entries (
            date TEXT PRIMARY KEY,
            activities_json TEXT NOT NULL,
            thoughts TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
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
}
