use crate::types::{
    Activity, Carousel, CarouselStatus, Entry, Slide, SlideFeedback, SlideStatus, SlideVersion,
};
use anyhow::{anyhow, Context, Result};
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

        CREATE TABLE IF NOT EXISTS slide_versions (
            id              TEXT PRIMARY KEY,
            slide_id        TEXT NOT NULL REFERENCES slides(id) ON DELETE CASCADE,
            version_number  INTEGER NOT NULL,
            html_path       TEXT NOT NULL,
            screenshot_path TEXT,
            pdf_path        TEXT,
            created_at      TEXT NOT NULL,
            UNIQUE(slide_id, version_number)
        );

        CREATE INDEX IF NOT EXISTS idx_slide_versions_slide
            ON slide_versions (slide_id, version_number DESC);

        CREATE TABLE IF NOT EXISTS slide_feedback (
            id                TEXT PRIMARY KEY,
            slide_version_id  TEXT NOT NULL REFERENCES slide_versions(id) ON DELETE CASCADE,
            accepted          BOOLEAN NOT NULL DEFAULT FALSE,
            feedback          TEXT NOT NULL DEFAULT '',
            created_at        TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_slide_feedback_version
            ON slide_feedback (slide_version_id);
        "#,
    )
    .context("running migrations (base schema)")?;

    ensure_column(conn, "carousels", "slug", "TEXT")?;
    ensure_column(
        conn,
        "carousels",
        "status",
        "TEXT NOT NULL CHECK (status IN ('idle','generating','done','failed')) DEFAULT 'idle'",
    )?;
    ensure_column(conn, "carousels", "pdf_path", "TEXT")?;
    ensure_column(conn, "carousels", "run_dir", "TEXT")?;
    ensure_column(conn, "carousels", "run_started_at", "TEXT")?;
    ensure_column(conn, "carousels", "run_finished_at", "TEXT")?;

    ensure_column(
        conn,
        "slides",
        "status",
        "TEXT NOT NULL CHECK (status IN ('idle','queued','generating','reviewing','accepted','failed')) DEFAULT 'idle'",
    )?;
    ensure_column(conn, "slides", "last_error", "TEXT")?;

    backfill_slugs(conn)?;

    Ok(())
}

fn ensure_column(conn: &Connection, table: &str, column: &str, type_decl: &str) -> Result<()> {
    let exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info(?1) WHERE name = ?2",
            params![table, column],
            |r| r.get(0),
        )
        .with_context(|| format!("inspecting columns on {}", table))?;
    if exists == 0 {
        let stmt = format!("ALTER TABLE {} ADD COLUMN {} {}", table, column, type_decl);
        conn.execute(&stmt, [])
            .with_context(|| format!("adding column {}.{}", table, column))?;
    }
    Ok(())
}

pub fn slugify(s: &str) -> String {
    let mut out = String::new();
    let mut last_dash = true;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

fn backfill_slugs(conn: &Connection) -> Result<()> {
    let mut stmt = conn
        .prepare("SELECT id, label FROM carousels WHERE slug IS NULL OR slug = ''")
        .context("preparing slug backfill scan")?;
    let rows: Vec<(String, String)> = stmt
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
        .context("scanning carousels for slug backfill")?
        .collect::<rusqlite::Result<Vec<_>>>()
        .context("collecting carousels for slug backfill")?;
    drop(stmt);

    for (id, label) in rows {
        let base = slugify(&label);
        let base = if base.is_empty() { "carousel".to_string() } else { base };
        let slug = unique_slug(conn, &base, Some(&id))?;
        conn.execute(
            "UPDATE carousels SET slug = ?2 WHERE id = ?1",
            params![id, slug],
        )
        .context("backfilling slug")?;
    }
    Ok(())
}

fn unique_slug(conn: &Connection, base: &str, exclude_id: Option<&str>) -> Result<String> {
    let mut candidate = base.to_string();
    let mut suffix = 1u32;
    loop {
        let count: i64 = match exclude_id {
            Some(id) => conn.query_row(
                "SELECT COUNT(*) FROM carousels WHERE slug = ?1 AND id != ?2",
                params![candidate, id],
                |r| r.get(0),
            )?,
            None => conn.query_row(
                "SELECT COUNT(*) FROM carousels WHERE slug = ?1",
                params![candidate],
                |r| r.get(0),
            )?,
        };
        if count == 0 {
            return Ok(candidate);
        }
        suffix += 1;
        candidate = format!("{}-{}", base, suffix);
        if suffix > 9999 {
            return Err(anyhow!("could not find unique slug for {}", base));
        }
    }
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

const CAROUSEL_COLUMNS: &str =
    "id, label, slug, status, pdf_path, run_dir, run_started_at, run_finished_at, created_at, updated_at";

fn map_carousel_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Carousel> {
    let id: String = row.get(0)?;
    let label: String = row.get(1)?;
    let slug: Option<String> = row.get(2)?;
    let status: String = row.get(3)?;
    let pdf_path: Option<String> = row.get(4)?;
    let run_dir: Option<String> = row.get(5)?;
    let run_started_at: Option<String> = row.get(6)?;
    let run_finished_at: Option<String> = row.get(7)?;
    let created_at: String = row.get(8)?;
    let updated_at: String = row.get(9)?;
    let parse_dt = |s: &str| {
        DateTime::parse_from_rfc3339(s)
            .map(|d| d.with_timezone(&Utc))
            .map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })
    };
    let parse_dt_opt = |s: Option<String>| match s {
        Some(s) => parse_dt(&s).map(Some),
        None => Ok(None),
    };
    Ok(Carousel {
        id,
        label,
        slug,
        status: CarouselStatus::from_str(&status).unwrap_or(CarouselStatus::Idle),
        pdf_path,
        run_dir,
        run_started_at: parse_dt_opt(run_started_at)?,
        run_finished_at: parse_dt_opt(run_finished_at)?,
        created_at: parse_dt(&created_at)?,
        updated_at: parse_dt(&updated_at)?,
    })
}

pub fn list_carousels(conn: &Connection) -> Result<Vec<Carousel>> {
    let sql = format!(
        "SELECT {} FROM carousels ORDER BY updated_at DESC",
        CAROUSEL_COLUMNS
    );
    let mut stmt = conn.prepare(&sql).context("preparing list_carousels")?;
    let rows = stmt
        .query_map([], map_carousel_row)
        .context("querying carousels")?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.context("reading carousel row")?);
    }
    Ok(out)
}

pub fn get_carousel(conn: &Connection, id: &str) -> Result<Option<Carousel>> {
    let sql = format!("SELECT {} FROM carousels WHERE id = ?1", CAROUSEL_COLUMNS);
    conn.query_row(&sql, params![id], map_carousel_row)
        .optional()
        .context("getting carousel")
}

pub fn create_carousel(conn: &Connection, label: &str) -> Result<Carousel> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let base = slugify(label);
    let base = if base.is_empty() { "carousel".to_string() } else { base };
    let slug = unique_slug(conn, &base, None)?;
    conn.execute(
        r#"
        INSERT INTO carousels (id, label, slug, status, created_at, updated_at)
        VALUES (?1, ?2, ?3, 'idle', ?4, ?4)
        "#,
        params![id, label, slug, now],
    )
    .context("inserting carousel")?;
    get_carousel(conn, &id)?.ok_or_else(|| anyhow!("carousel disappeared after insert"))
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

pub fn set_carousel_status(
    conn: &Connection,
    id: &str,
    status: CarouselStatus,
) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE carousels SET status = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, status.as_str(), now],
    )
    .context("setting carousel status")?;
    Ok(())
}

pub fn set_carousel_run_started(
    conn: &Connection,
    id: &str,
    run_dir: &str,
) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        r#"
        UPDATE carousels
        SET status = 'generating',
            run_dir = ?2,
            run_started_at = ?3,
            run_finished_at = NULL,
            pdf_path = NULL,
            updated_at = ?3
        WHERE id = ?1
        "#,
        params![id, run_dir, now],
    )
    .context("setting carousel run started")?;
    Ok(())
}

pub fn set_carousel_run_finished(
    conn: &Connection,
    id: &str,
    status: CarouselStatus,
    pdf_path: Option<&str>,
) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        r#"
        UPDATE carousels
        SET status = ?2,
            run_finished_at = ?3,
            pdf_path = ?4,
            updated_at = ?3
        WHERE id = ?1
        "#,
        params![id, status.as_str(), now, pdf_path],
    )
    .context("setting carousel run finished")?;
    Ok(())
}

const SLIDE_COLUMNS: &str =
    "id, carousel_id, order_index, content, status, last_error, created_at, updated_at";

fn map_slide_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Slide> {
    let id: String = row.get(0)?;
    let carousel_id: String = row.get(1)?;
    let order_index: i64 = row.get(2)?;
    let content: String = row.get(3)?;
    let status: String = row.get(4)?;
    let last_error: Option<String> = row.get(5)?;
    let created_at: String = row.get(6)?;
    let updated_at: String = row.get(7)?;
    let parse_dt = |s: &str| {
        DateTime::parse_from_rfc3339(s)
            .map(|d| d.with_timezone(&Utc))
            .map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })
    };
    Ok(Slide {
        id,
        carousel_id,
        order_index,
        content,
        status: SlideStatus::from_str(&status).unwrap_or(SlideStatus::Idle),
        last_error,
        created_at: parse_dt(&created_at)?,
        updated_at: parse_dt(&updated_at)?,
    })
}

pub fn list_slides(conn: &Connection, carousel_id: &str) -> Result<Vec<Slide>> {
    let sql = format!(
        "SELECT {} FROM slides WHERE carousel_id = ?1 ORDER BY order_index ASC",
        SLIDE_COLUMNS
    );
    let mut stmt = conn.prepare(&sql).context("preparing list_slides")?;
    let rows = stmt
        .query_map(params![carousel_id], map_slide_row)
        .context("querying slides")?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.context("reading slide row")?);
    }
    Ok(out)
}

pub fn get_slide(conn: &Connection, id: &str) -> Result<Option<Slide>> {
    let sql = format!("SELECT {} FROM slides WHERE id = ?1", SLIDE_COLUMNS);
    conn.query_row(&sql, params![id], map_slide_row)
        .optional()
        .context("getting slide")
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
        INSERT INTO slides (id, carousel_id, order_index, content, status, created_at, updated_at)
        VALUES (?1, ?2, ?3, '', 'idle', ?4, ?4)
        "#,
        params![id, carousel_id, next_index, now],
    )
    .context("inserting slide")?;
    get_slide(conn, &id)?.ok_or_else(|| anyhow!("slide disappeared after insert"))
}

pub fn set_slide_status(
    conn: &Connection,
    id: &str,
    status: SlideStatus,
    last_error: Option<&str>,
) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE slides SET status = ?2, last_error = ?3, updated_at = ?4 WHERE id = ?1",
        params![id, status.as_str(), last_error, now],
    )
    .context("setting slide status")?;
    Ok(())
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

const SLIDE_VERSION_COLUMNS: &str =
    "id, slide_id, version_number, html_path, screenshot_path, pdf_path, created_at";

fn map_slide_version_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SlideVersion> {
    let id: String = row.get(0)?;
    let slide_id: String = row.get(1)?;
    let version_number: i64 = row.get(2)?;
    let html_path: String = row.get(3)?;
    let screenshot_path: Option<String> = row.get(4)?;
    let pdf_path: Option<String> = row.get(5)?;
    let created_at: String = row.get(6)?;
    let created_at = DateTime::parse_from_rfc3339(&created_at)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                Box::new(e),
            )
        })?;
    Ok(SlideVersion {
        id,
        slide_id,
        version_number,
        html_path,
        screenshot_path,
        pdf_path,
        created_at,
    })
}

pub fn insert_slide_version(
    conn: &Connection,
    slide_id: &str,
    html_path: &str,
) -> Result<SlideVersion> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let next_version: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(version_number) + 1, 1) FROM slide_versions WHERE slide_id = ?1",
            params![slide_id],
            |row| row.get(0),
        )
        .context("computing next version_number")?;
    conn.execute(
        r#"
        INSERT INTO slide_versions
            (id, slide_id, version_number, html_path, created_at)
        VALUES (?1, ?2, ?3, ?4, ?5)
        "#,
        params![id, slide_id, next_version, html_path, now],
    )
    .context("inserting slide version")?;
    Ok(SlideVersion {
        id,
        slide_id: slide_id.to_string(),
        version_number: next_version,
        html_path: html_path.to_string(),
        screenshot_path: None,
        pdf_path: None,
        created_at: parse_rfc3339(&now)?,
    })
}

pub fn update_slide_version_renders(
    conn: &Connection,
    id: &str,
    screenshot_path: Option<&str>,
    pdf_path: Option<&str>,
) -> Result<()> {
    conn.execute(
        r#"
        UPDATE slide_versions
        SET screenshot_path = ?2, pdf_path = ?3
        WHERE id = ?1
        "#,
        params![id, screenshot_path, pdf_path],
    )
    .context("updating slide version renders")?;
    Ok(())
}

pub fn list_slide_versions(conn: &Connection, slide_id: &str) -> Result<Vec<SlideVersion>> {
    let sql = format!(
        "SELECT {} FROM slide_versions WHERE slide_id = ?1 ORDER BY version_number DESC",
        SLIDE_VERSION_COLUMNS
    );
    let mut stmt = conn.prepare(&sql).context("preparing list_slide_versions")?;
    let rows = stmt
        .query_map(params![slide_id], map_slide_version_row)
        .context("querying slide_versions")?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.context("reading slide_version row")?);
    }
    Ok(out)
}

pub fn get_latest_slide_version(
    conn: &Connection,
    slide_id: &str,
) -> Result<Option<SlideVersion>> {
    let sql = format!(
        "SELECT {} FROM slide_versions WHERE slide_id = ?1 ORDER BY version_number DESC LIMIT 1",
        SLIDE_VERSION_COLUMNS
    );
    conn.query_row(&sql, params![slide_id], map_slide_version_row)
        .optional()
        .context("getting latest slide_version")
}

const SLIDE_FEEDBACK_COLUMNS: &str =
    "id, slide_version_id, accepted, feedback, created_at";

fn map_slide_feedback_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SlideFeedback> {
    let id: String = row.get(0)?;
    let slide_version_id: String = row.get(1)?;
    let accepted: bool = row.get(2)?;
    let feedback: String = row.get(3)?;
    let created_at: String = row.get(4)?;
    let created_at = DateTime::parse_from_rfc3339(&created_at)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                Box::new(e),
            )
        })?;
    Ok(SlideFeedback {
        id,
        slide_version_id,
        accepted,
        feedback,
        created_at,
    })
}

pub fn insert_slide_feedback(
    conn: &Connection,
    slide_version_id: &str,
    accepted: bool,
    feedback: &str,
) -> Result<SlideFeedback> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    conn.execute(
        r#"
        INSERT INTO slide_feedback
            (id, slide_version_id, accepted, feedback, created_at)
        VALUES (?1, ?2, ?3, ?4, ?5)
        "#,
        params![id, slide_version_id, accepted, feedback, now],
    )
    .context("inserting slide feedback")?;
    Ok(SlideFeedback {
        id,
        slide_version_id: slide_version_id.to_string(),
        accepted,
        feedback: feedback.to_string(),
        created_at: parse_rfc3339(&now)?,
    })
}

pub fn list_slide_feedback(
    conn: &Connection,
    slide_version_id: &str,
) -> Result<Vec<SlideFeedback>> {
    let sql = format!(
        "SELECT {} FROM slide_feedback WHERE slide_version_id = ?1 ORDER BY created_at ASC",
        SLIDE_FEEDBACK_COLUMNS
    );
    let mut stmt = conn.prepare(&sql).context("preparing list_slide_feedback")?;
    let rows = stmt
        .query_map(params![slide_version_id], map_slide_feedback_row)
        .context("querying slide_feedback")?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.context("reading slide_feedback row")?);
    }
    Ok(out)
}

pub fn get_latest_slide_feedback(
    conn: &Connection,
    slide_version_id: &str,
) -> Result<Option<SlideFeedback>> {
    let sql = format!(
        "SELECT {} FROM slide_feedback WHERE slide_version_id = ?1 ORDER BY created_at DESC LIMIT 1",
        SLIDE_FEEDBACK_COLUMNS
    );
    conn.query_row(&sql, params![slide_version_id], map_slide_feedback_row)
        .optional()
        .context("getting latest slide_feedback")
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

    #[test]
    fn slugify_handles_common_inputs() {
        assert_eq!(slugify("Manifests for Agents"), "manifests-for-agents");
        assert_eq!(slugify("  Hello,  world!  "), "hello-world");
        assert_eq!(slugify("ALL_CAPS"), "all-caps");
        assert_eq!(slugify(""), "");
        assert_eq!(slugify("__--!!"), "");
        assert_eq!(slugify("v0.1 prototype"), "v0-1-prototype");
    }

    #[test]
    fn create_carousel_assigns_slug_and_status() {
        let conn = open_in_memory().unwrap();
        let c = create_carousel(&conn, "Launch Week").unwrap();
        assert_eq!(c.slug.as_deref(), Some("launch-week"));
        assert_eq!(c.status, CarouselStatus::Idle);
        assert!(c.run_dir.is_none());
        assert!(c.pdf_path.is_none());
    }

    #[test]
    fn create_carousel_disambiguates_slug_collisions() {
        let conn = open_in_memory().unwrap();
        let a = create_carousel(&conn, "Repeat").unwrap();
        // Same slug source, different label — second one should get -2 suffix.
        let b = create_carousel(&conn, "REPEAT").unwrap();
        assert_eq!(a.slug.as_deref(), Some("repeat"));
        assert_eq!(b.slug.as_deref(), Some("repeat-2"));
    }

    #[test]
    fn carousel_status_transitions() {
        let conn = open_in_memory().unwrap();
        let c = create_carousel(&conn, "deck").unwrap();
        set_carousel_run_started(&conn, &c.id, "/tmp/run-1").unwrap();
        let after_start = get_carousel(&conn, &c.id).unwrap().unwrap();
        assert_eq!(after_start.status, CarouselStatus::Generating);
        assert_eq!(after_start.run_dir.as_deref(), Some("/tmp/run-1"));
        assert!(after_start.run_started_at.is_some());

        set_carousel_run_finished(
            &conn,
            &c.id,
            CarouselStatus::Done,
            Some("/tmp/run-1/deck.pdf"),
        )
        .unwrap();
        let after_finish = get_carousel(&conn, &c.id).unwrap().unwrap();
        assert_eq!(after_finish.status, CarouselStatus::Done);
        assert_eq!(after_finish.pdf_path.as_deref(), Some("/tmp/run-1/deck.pdf"));
        assert!(after_finish.run_finished_at.is_some());
    }

    #[test]
    fn slide_status_with_last_error() {
        let conn = open_in_memory().unwrap();
        let c = create_carousel(&conn, "deck2").unwrap();
        let s = create_slide(&conn, &c.id).unwrap();
        assert_eq!(s.status, SlideStatus::Idle);
        assert!(s.last_error.is_none());

        set_slide_status(&conn, &s.id, SlideStatus::Failed, Some("boom")).unwrap();
        let after = get_slide(&conn, &s.id).unwrap().unwrap();
        assert_eq!(after.status, SlideStatus::Failed);
        assert_eq!(after.last_error.as_deref(), Some("boom"));

        set_slide_status(&conn, &s.id, SlideStatus::Accepted, None).unwrap();
        let cleared = get_slide(&conn, &s.id).unwrap().unwrap();
        assert_eq!(cleared.status, SlideStatus::Accepted);
        assert!(cleared.last_error.is_none());
    }

    #[test]
    fn slide_versions_increment_per_slide() {
        let conn = open_in_memory().unwrap();
        let c = create_carousel(&conn, "v-deck").unwrap();
        let s1 = create_slide(&conn, &c.id).unwrap();
        let s2 = create_slide(&conn, &c.id).unwrap();

        let s1_v1 = insert_slide_version(&conn, &s1.id, "/tmp/s1/v1.html").unwrap();
        let s1_v2 = insert_slide_version(&conn, &s1.id, "/tmp/s1/v2.html").unwrap();
        let s2_v1 = insert_slide_version(&conn, &s2.id, "/tmp/s2/v1.html").unwrap();

        assert_eq!(s1_v1.version_number, 1);
        assert_eq!(s1_v2.version_number, 2);
        // Versioning is per-slide, not global.
        assert_eq!(s2_v1.version_number, 1);

        let listed = list_slide_versions(&conn, &s1.id).unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].version_number, 2); // newest first
        assert_eq!(listed[1].version_number, 1);

        let latest = get_latest_slide_version(&conn, &s1.id).unwrap().unwrap();
        assert_eq!(latest.id, s1_v2.id);
    }

    #[test]
    fn slide_version_render_paths_update() {
        let conn = open_in_memory().unwrap();
        let c = create_carousel(&conn, "render-deck").unwrap();
        let s = create_slide(&conn, &c.id).unwrap();
        let v = insert_slide_version(&conn, &s.id, "/tmp/v1.html").unwrap();
        assert!(v.screenshot_path.is_none());
        assert!(v.pdf_path.is_none());

        update_slide_version_renders(
            &conn,
            &v.id,
            Some("/tmp/v1.png"),
            Some("/tmp/v1.pdf"),
        )
        .unwrap();
        let after = get_latest_slide_version(&conn, &s.id).unwrap().unwrap();
        assert_eq!(after.screenshot_path.as_deref(), Some("/tmp/v1.png"));
        assert_eq!(after.pdf_path.as_deref(), Some("/tmp/v1.pdf"));
    }

    #[test]
    fn slide_feedback_roundtrip_and_boolean_storage() {
        let conn = open_in_memory().unwrap();
        let c = create_carousel(&conn, "fb-deck").unwrap();
        let s = create_slide(&conn, &c.id).unwrap();
        let v = insert_slide_version(&conn, &s.id, "/tmp/v1.html").unwrap();

        let f1 = insert_slide_feedback(&conn, &v.id, false, "missing rule").unwrap();
        assert!(!f1.accepted);
        assert_eq!(f1.feedback, "missing rule");

        std::thread::sleep(std::time::Duration::from_millis(5));
        let f2 = insert_slide_feedback(&conn, &v.id, true, "").unwrap();
        assert!(f2.accepted);

        let listed = list_slide_feedback(&conn, &v.id).unwrap();
        assert_eq!(listed.len(), 2);
        // ASC by created_at — older first.
        assert_eq!(listed[0].id, f1.id);
        assert_eq!(listed[1].id, f2.id);

        let latest = get_latest_slide_feedback(&conn, &v.id).unwrap().unwrap();
        assert_eq!(latest.id, f2.id);
        assert!(latest.accepted);
    }

    #[test]
    fn slide_versions_cascade_on_slide_delete() {
        let conn = open_in_memory().unwrap();
        let c = create_carousel(&conn, "cascade").unwrap();
        let s = create_slide(&conn, &c.id).unwrap();
        let v = insert_slide_version(&conn, &s.id, "/tmp/v1.html").unwrap();
        insert_slide_feedback(&conn, &v.id, false, "fix").unwrap();

        delete_slide(&conn, &s.id).unwrap();
        let version_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM slide_versions", [], |r| r.get(0))
            .unwrap();
        let feedback_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM slide_feedback", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version_count, 0);
        assert_eq!(feedback_count, 0);
    }

    #[test]
    fn carousel_status_check_constraint_rejects_garbage() {
        let conn = open_in_memory().unwrap();
        let c = create_carousel(&conn, "check").unwrap();
        let res = conn.execute(
            "UPDATE carousels SET status = 'whatever' WHERE id = ?1",
            params![c.id],
        );
        assert!(res.is_err(), "CHECK constraint should reject unknown status");
    }

    #[test]
    fn slide_status_check_constraint_rejects_garbage() {
        let conn = open_in_memory().unwrap();
        let c = create_carousel(&conn, "check2").unwrap();
        let s = create_slide(&conn, &c.id).unwrap();
        let res = conn.execute(
            "UPDATE slides SET status = 'wrong' WHERE id = ?1",
            params![s.id],
        );
        assert!(res.is_err(), "CHECK constraint should reject unknown slide status");
    }
}
