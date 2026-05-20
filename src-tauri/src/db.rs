use crate::types::{
    Activity, Carousel, CarouselStatus, Entry, Orientation, Slide, SlideFeedback, SlideStatus,
    SlideVersion,
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
    // Per-carousel model overrides for the bun driver's two agents. NULL
    // means "use the driver's DEFAULT_MODEL" — see scripts/generate-carousel.ts.
    ensure_column(conn, "carousels", "impl_model", "TEXT")?;
    ensure_column(conn, "carousels", "manager_model", "TEXT")?;
    // Per-carousel canvas orientation. NOT NULL with DEFAULT 'vertical'
    // means existing rows backfill to vertical (the legacy 1080×1350) and
    // inserts that don't specify orientation pick up the default — so the
    // ~25 test sites that do `create_carousel(&conn, "label")` keep
    // working without churn.
    ensure_column(
        conn,
        "carousels",
        "orientation",
        "TEXT NOT NULL CHECK (orientation IN ('vertical','landscape')) DEFAULT 'vertical'",
    )?;

    ensure_column(
        conn,
        "slides",
        "status",
        "TEXT NOT NULL CHECK (status IN ('idle','queued','generating','reviewing','accepted','failed')) DEFAULT 'idle'",
    )?;
    ensure_column(conn, "slides", "last_error", "TEXT")?;
    ensure_column(conn, "slides", "title", "TEXT")?;

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
    "id, label, slug, status, pdf_path, run_dir, run_started_at, run_finished_at, created_at, updated_at, impl_model, manager_model, orientation";

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
    let impl_model: Option<String> = row.get(10)?;
    let manager_model: Option<String> = row.get(11)?;
    let orientation_raw: String = row.get(12)?;
    // Defensive fallback: the CHECK constraint should make any other
    // value impossible, but reading the column shouldn't panic if a
    // future migration ever loosens the constraint.
    let orientation = Orientation::from_str(&orientation_raw).unwrap_or(Orientation::Vertical);
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
        impl_model,
        manager_model,
        orientation,
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
    create_carousel_with_orientation(conn, label, Orientation::Vertical)
}

/// Create a carousel with an explicit orientation. The vertical-only
/// `create_carousel` is a thin wrapper around this — both end up in the
/// same INSERT below.
pub fn create_carousel_with_orientation(
    conn: &Connection,
    label: &str,
    orientation: Orientation,
) -> Result<Carousel> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let base = slugify(label);
    let base = if base.is_empty() { "carousel".to_string() } else { base };
    let slug = unique_slug(conn, &base, None)?;
    conn.execute(
        r#"
        INSERT INTO carousels (id, label, slug, status, orientation, created_at, updated_at)
        VALUES (?1, ?2, ?3, 'idle', ?4, ?5, ?5)
        "#,
        params![id, label, slug, orientation.as_str(), now],
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

/// Set or clear the per-carousel model overrides.
///
/// `None` (or whitespace-only `Some`) clears the column to NULL — meaning
/// "use the driver's DEFAULT_MODEL". Same trim/empty→NULL pattern as
/// `update_slide_title` so that an empty string from the UI doesn't lock
/// in a literal-empty model id.
pub fn update_carousel_models(
    conn: &Connection,
    id: &str,
    impl_model: Option<&str>,
    manager_model: Option<&str>,
) -> Result<()> {
    fn normalize(s: Option<&str>) -> Option<String> {
        s.and_then(|t| {
            let trimmed = t.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
    }
    let impl_norm = normalize(impl_model);
    let manager_norm = normalize(manager_model);
    let now = Utc::now().to_rfc3339();
    conn.execute(
        r#"
        UPDATE carousels
        SET impl_model = ?2, manager_model = ?3, updated_at = ?4
        WHERE id = ?1
        "#,
        params![id, impl_norm, manager_norm, now],
    )
    .context("updating carousel models")?;
    Ok(())
}

/// Set the carousel's canvas orientation. Used by the editor's
/// orientation dropdown — `update_carousel_models` is the precedent for
/// per-carousel mutable settings.
pub fn update_carousel_orientation(
    conn: &Connection,
    id: &str,
    orientation: Orientation,
) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        r#"
        UPDATE carousels
        SET orientation = ?2, updated_at = ?3
        WHERE id = ?1
        "#,
        params![id, orientation.as_str(), now],
    )
    .context("updating carousel orientation")?;
    Ok(())
}

pub fn delete_carousel(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM carousels WHERE id = ?1", params![id])
        .context("deleting carousel")?;
    Ok(())
}

/// Duplicate a carousel and its slides into a fresh carousel row.
///
/// Behaviour:
/// - Label: `"{source.label} copy"`. If that's already taken (UNIQUE
///   constraint would fail) we append the first 4 chars of the source
///   id: `"{source.label} copy {abcd}"`. If that *also* collides, error
///   out — at that point the user is intentionally cloning the same
///   carousel many times and should rename one of them first.
/// - Slug: regenerated from the new label via the existing `unique_slug`
///   helper. Starts fresh; we don't carry the source's slug.
/// - Run state: `status = 'idle'`, `pdf_path`, `run_dir`, `run_started_at`,
///   `run_finished_at` all start NULL. The duplicate gets a clean run
///   history.
/// - Models: `impl_model` and `manager_model` ARE copied (the user is
///   most likely cloning to retry-with-tweaks; preserving model picks
///   matches that intent).
/// - Slides: each source slide becomes a new row with a fresh uuid,
///   preserving `order_index`, `content`, and `title`. `status` resets
///   to `'idle'` and `last_error` to NULL.
/// - `slide_versions` and `slide_feedback` are NOT copied. The duplicate
///   gets a clean run history; the prior accepted PDFs would point at
///   the source carousel's run directories anyway.
///
/// All writes happen inside a single transaction so a partial failure
/// rolls back cleanly — we never leave a half-duplicated carousel.
pub fn duplicate_carousel(conn: &mut Connection, id: &str) -> Result<Carousel> {
    let new_id = Uuid::new_v4().to_string();
    {
        let tx = conn.transaction().context("starting duplicate transaction")?;
        let now = Utc::now().to_rfc3339();

        // 1. Load the source carousel (Transaction derefs to Connection).
        let source = get_carousel(&tx, id)?
            .ok_or_else(|| anyhow!("carousel {id} not found"))?;

        // 2. Compose a non-colliding label.
        let primary_label = format!("{} copy", source.label);
        let exists = |label: &str, tx: &rusqlite::Transaction<'_>| -> Result<bool> {
            let count: i64 = tx
                .query_row(
                    "SELECT COUNT(*) FROM carousels WHERE label = ?1",
                    params![label],
                    |r| r.get(0),
                )
                .context("checking duplicate label collision")?;
            Ok(count > 0)
        };
        let new_label = if !exists(&primary_label, &tx)? {
            primary_label
        } else {
            let suffix: String = source.id.chars().take(4).collect();
            let fallback_label = format!("{} copy {}", source.label, suffix);
            if exists(&fallback_label, &tx)? {
                return Err(anyhow!(
                    "could not pick a non-colliding label for duplicate of {id}; \
                     rename an existing copy and try again"
                ));
            }
            fallback_label
        };

        // 3. Slug from the new label, scoped against the existing slugs.
        let base = slugify(&new_label);
        let base = if base.is_empty() { "carousel".to_string() } else { base };
        let new_slug = unique_slug(&tx, &base, None)?;

        // 4. Insert the new carousel, copying model overrides + orientation.
        //    Orientation carries over because a duplicate is meant for
        //    "retry-with-tweaks" and the canvas size is part of what the
        //    user is iterating on.
        tx.execute(
            r#"
            INSERT INTO carousels
                (id, label, slug, status, pdf_path, run_dir, run_started_at,
                 run_finished_at, impl_model, manager_model, orientation,
                 created_at, updated_at)
            VALUES (?1, ?2, ?3, 'idle', NULL, NULL, NULL, NULL, ?4, ?5, ?6, ?7, ?7)
            "#,
            params![
                new_id,
                new_label,
                new_slug,
                source.impl_model,
                source.manager_model,
                source.orientation.as_str(),
                now
            ],
        )
        .context("inserting duplicate carousel")?;

        // 5. Copy slides, preserving order_index/content/title only. New
        //    uuids; status resets to 'idle'.
        let mut stmt = tx
            .prepare(
                "SELECT order_index, content, title FROM slides \
                 WHERE carousel_id = ?1 ORDER BY order_index ASC",
            )
            .context("preparing slide copy scan")?;
        let rows = stmt
            .query_map(params![id], |row| {
                let order_index: i64 = row.get(0)?;
                let content: String = row.get(1)?;
                let title: Option<String> = row.get(2)?;
                Ok((order_index, content, title))
            })
            .context("scanning source slides")?
            .collect::<rusqlite::Result<Vec<_>>>()
            .context("collecting source slides")?;
        drop(stmt);

        for (order_index, content, title) in rows {
            let slide_id = Uuid::new_v4().to_string();
            tx.execute(
                r#"
                INSERT INTO slides
                    (id, carousel_id, order_index, content, title, status,
                     last_error, created_at, updated_at)
                VALUES (?1, ?2, ?3, ?4, ?5, 'idle', NULL, ?6, ?6)
                "#,
                params![slide_id, new_id, order_index, content, title, now],
            )
            .context("inserting duplicated slide")?;
        }

        tx.commit().context("committing duplicate transaction")?;
    }

    get_carousel(conn, &new_id)?
        .ok_or_else(|| anyhow!("duplicate carousel disappeared after insert"))
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

/// Reset every slide in a carousel to `queued` with no `last_error`.
///
/// Called at run-start so the progress UI doesn't show stale per-slide
/// statuses from a previous run (e.g. a slide that was accepted last time
/// would otherwise stay ✓ accepted while the new run's driver hasn't
/// reached it yet).
pub fn reset_slides_for_run(conn: &Connection, carousel_id: &str) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        r#"
        UPDATE slides
        SET status = 'queued',
            last_error = NULL,
            updated_at = ?2
        WHERE carousel_id = ?1
        "#,
        params![carousel_id, now],
    )
    .context("resetting slides for run")?;
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
    "id, carousel_id, order_index, content, title, status, last_error, created_at, updated_at";

fn map_slide_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Slide> {
    let id: String = row.get(0)?;
    let carousel_id: String = row.get(1)?;
    let order_index: i64 = row.get(2)?;
    let content: String = row.get(3)?;
    // The legacy slides schema declares `title TEXT NOT NULL DEFAULT ''`,
    // so DBs migrated before that changed return `Some("")` for slides
    // with no user-set title (every imported/CLI-created slide). Honour
    // the documented contract (see `update_slide_title`): an empty or
    // whitespace-only title means "auto-derive from content" — i.e. None.
    // Normalising here fixes every read path (CLI status, Tauri, frontend)
    // and both schema variants without a data migration.
    let title: Option<String> = row
        .get::<_, Option<String>>(4)?
        .filter(|t| !t.trim().is_empty());
    let status: String = row.get(5)?;
    let last_error: Option<String> = row.get(6)?;
    let created_at: String = row.get(7)?;
    let updated_at: String = row.get(8)?;
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
        title,
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

/// Set or clear the user-edited slide title.
///
/// `None` clears the column to NULL — meaning "auto-derive from content"
/// to the frontend. `Some("")` is treated the same as `None` so that a
/// user clearing the input reverts to the default rather than locking in
/// an empty string.
pub fn update_slide_title(
    conn: &Connection,
    id: &str,
    title: Option<&str>,
) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    let normalized = title.and_then(|t| {
        let trimmed = t.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    });
    conn.execute(
        r#"
        UPDATE slides
        SET title = ?2, updated_at = ?3
        WHERE id = ?1
        "#,
        params![id, normalized, now],
    )
    .context("updating slide title")?;
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

/// Replace the entire ordering of a carousel's slides in one transaction.
///
/// `ordered_ids[i]` becomes the slide at order_index = i. Every id must
/// belong to the carousel; any slide id not in `ordered_ids` is left
/// untouched (which would leave it in a stale spot — the caller should
/// always pass every slide the carousel has). All updates run in a single
/// transaction so a partial failure doesn't leave order_index half-shuffled.
pub fn reorder_slides(
    conn: &mut Connection,
    carousel_id: &str,
    ordered_ids: &[String],
) -> Result<()> {
    let tx = conn.transaction().context("starting reorder transaction")?;
    let now = Utc::now().to_rfc3339();
    for (idx, id) in ordered_ids.iter().enumerate() {
        let rows = tx
            .execute(
                r#"
                UPDATE slides
                SET order_index = ?2, updated_at = ?3
                WHERE id = ?1 AND carousel_id = ?4
                "#,
                params![id, idx as i64, now, carousel_id],
            )
            .with_context(|| format!("reordering slide {id} to {idx}"))?;
        if rows == 0 {
            return Err(anyhow!(
                "slide {id} not found in carousel {carousel_id}"
            ));
        }
    }
    tx.commit().context("committing reorder transaction")?;
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
    fn reorder_slides_rotates_indexes() {
        let mut conn = open_in_memory().unwrap();
        let c = create_carousel(&conn, "reorder").unwrap();
        let s1 = create_slide(&conn, &c.id).unwrap();
        let s2 = create_slide(&conn, &c.id).unwrap();
        let s3 = create_slide(&conn, &c.id).unwrap();
        // Sanity: initial order is creation order.
        assert_eq!(s1.order_index, 0);
        assert_eq!(s2.order_index, 1);
        assert_eq!(s3.order_index, 2);

        // New order: s3, s1, s2.
        let new_order = vec![s3.id.clone(), s1.id.clone(), s2.id.clone()];
        reorder_slides(&mut conn, &c.id, &new_order).unwrap();

        let listed = list_slides(&conn, &c.id).unwrap();
        assert_eq!(listed[0].id, s3.id);
        assert_eq!(listed[0].order_index, 0);
        assert_eq!(listed[1].id, s1.id);
        assert_eq!(listed[1].order_index, 1);
        assert_eq!(listed[2].id, s2.id);
        assert_eq!(listed[2].order_index, 2);
    }

    #[test]
    fn reorder_slides_rejects_foreign_id() {
        let mut conn = open_in_memory().unwrap();
        let a = create_carousel(&conn, "a").unwrap();
        let b = create_carousel(&conn, "b").unwrap();
        let a_slide = create_slide(&conn, &a.id).unwrap();
        let b_slide = create_slide(&conn, &b.id).unwrap();
        // Trying to reorder carousel A while including a slide from B
        // must fail and leave both carousels untouched.
        let bad = vec![a_slide.id.clone(), b_slide.id.clone()];
        let res = reorder_slides(&mut conn, &a.id, &bad);
        assert!(res.is_err());
        // a_slide still at order_index 0, b_slide untouched.
        let a_listed = list_slides(&conn, &a.id).unwrap();
        let b_listed = list_slides(&conn, &b.id).unwrap();
        assert_eq!(a_listed.len(), 1);
        assert_eq!(a_listed[0].order_index, 0);
        assert_eq!(b_listed.len(), 1);
        assert_eq!(b_listed[0].order_index, 0);
    }

    #[test]
    fn slide_title_default_and_roundtrip() {
        let conn = open_in_memory().unwrap();
        let c = create_carousel(&conn, "title-deck").unwrap();
        let s = create_slide(&conn, &c.id).unwrap();
        // Default: no user-set title.
        assert!(s.title.is_none());

        update_slide_title(&conn, &s.id, Some("Where Opus 4.7 Excels")).unwrap();
        let after = get_slide(&conn, &s.id).unwrap().unwrap();
        assert_eq!(after.title.as_deref(), Some("Where Opus 4.7 Excels"));

        // Whitespace-only / empty input clears back to NULL (auto-derive).
        update_slide_title(&conn, &s.id, Some("   ")).unwrap();
        let cleared_blank = get_slide(&conn, &s.id).unwrap().unwrap();
        assert!(cleared_blank.title.is_none());

        update_slide_title(&conn, &s.id, Some("Edited")).unwrap();
        update_slide_title(&conn, &s.id, None).unwrap();
        let cleared_none = get_slide(&conn, &s.id).unwrap().unwrap();
        assert!(cleared_none.title.is_none());
    }

    #[test]
    fn legacy_empty_string_title_maps_to_none() {
        // Regression: production DBs migrated under the old
        // `title TEXT NOT NULL DEFAULT ''` schema store '' (not NULL) for
        // every imported/CLI-created slide. map_slide_row must treat that
        // (and whitespace-only) as "no title", matching the documented
        // update_slide_title contract — otherwise the slide list / CLI
        // status render a blank label instead of auto-deriving from
        // content. Simulate the legacy value with a raw write.
        let conn = open_in_memory().unwrap();
        let c = create_carousel(&conn, "legacy-title").unwrap();
        let s = create_slide(&conn, &c.id).unwrap();
        update_slide_content(&conn, &s.id, "# Slide 1 :: plate-chart\nbody").unwrap();

        for raw in ["", "   ", "\t\n"] {
            conn.execute(
                "UPDATE slides SET title = ?2 WHERE id = ?1",
                params![s.id, raw],
            )
            .unwrap();
            let got = get_slide(&conn, &s.id).unwrap().unwrap();
            assert!(
                got.title.is_none(),
                "raw title {raw:?} should map to None, got {:?}",
                got.title
            );
            let listed = list_slides(&conn, &c.id).unwrap();
            assert!(listed[0].title.is_none(), "list_slides path for {raw:?}");
        }
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

    #[test]
    fn duplicate_carousel_copies_slides_and_clears_run_state() {
        let mut conn = open_in_memory().unwrap();
        let src = create_carousel(&conn, "Source").unwrap();
        update_carousel_models(
            &conn,
            &src.id,
            Some("claude-sonnet-4-6"),
            Some("claude-opus-4-7"),
        )
        .unwrap();
        // Pretend a run finished on the source so we can verify the
        // duplicate doesn't carry the run state over.
        set_carousel_run_started(&conn, &src.id, "/tmp/run-1").unwrap();
        set_carousel_run_finished(
            &conn,
            &src.id,
            CarouselStatus::Done,
            Some("/tmp/run-1/source.pdf"),
        )
        .unwrap();

        let s1 = create_slide(&conn, &src.id).unwrap();
        update_slide_content(&conn, &s1.id, "first").unwrap();
        update_slide_title(&conn, &s1.id, Some("Hello")).unwrap();
        let s2 = create_slide(&conn, &src.id).unwrap();
        update_slide_content(&conn, &s2.id, "second").unwrap();
        let s3 = create_slide(&conn, &src.id).unwrap();
        update_slide_content(&conn, &s3.id, "third").unwrap();
        // A version + feedback row that should NOT carry over.
        let v = insert_slide_version(&conn, &s1.id, "/tmp/v1.html").unwrap();
        insert_slide_feedback(&conn, &v.id, true, "lgtm").unwrap();

        let dup = duplicate_carousel(&mut conn, &src.id).unwrap();
        assert_eq!(dup.label, "Source copy");
        assert_ne!(dup.id, src.id);
        assert_eq!(dup.status, CarouselStatus::Idle);
        assert!(dup.pdf_path.is_none(), "pdf_path must reset");
        assert!(dup.run_dir.is_none(), "run_dir must reset");
        assert!(dup.run_started_at.is_none(), "run_started_at must reset");
        assert!(dup.run_finished_at.is_none(), "run_finished_at must reset");
        assert_eq!(dup.impl_model.as_deref(), Some("claude-sonnet-4-6"));
        assert_eq!(dup.manager_model.as_deref(), Some("claude-opus-4-7"));
        // Slug must be fresh, not reused from source.
        assert!(dup.slug.is_some());
        assert_ne!(dup.slug, src.slug);

        let dup_slides = list_slides(&conn, &dup.id).unwrap();
        assert_eq!(dup_slides.len(), 3);
        assert_eq!(dup_slides[0].order_index, 0);
        assert_eq!(dup_slides[0].content, "first");
        assert_eq!(dup_slides[0].title.as_deref(), Some("Hello"));
        assert_eq!(dup_slides[0].status, SlideStatus::Idle);
        assert!(dup_slides[0].last_error.is_none());
        assert_eq!(dup_slides[1].content, "second");
        assert_eq!(dup_slides[2].content, "third");
        // Slide ids are fresh — not reused from the source.
        let src_ids: std::collections::HashSet<_> = list_slides(&conn, &src.id)
            .unwrap()
            .into_iter()
            .map(|s| s.id)
            .collect();
        for d in &dup_slides {
            assert!(!src_ids.contains(&d.id), "slide id must be fresh");
        }
        // No slide_versions or slide_feedback for any duplicated slide.
        for d in &dup_slides {
            assert!(
                list_slide_versions(&conn, &d.id).unwrap().is_empty(),
                "duplicate slides must have no carried-over versions"
            );
        }
        // Source slide_versions still intact (we didn't damage them).
        assert_eq!(list_slide_versions(&conn, &s1.id).unwrap().len(), 1);

        // A second duplicate should append the id-suffix to dodge the
        // unique-label constraint that "Source copy" now occupies.
        let dup2 = duplicate_carousel(&mut conn, &src.id).unwrap();
        assert!(dup2.label.starts_with("Source copy "));
        assert_ne!(dup2.label, dup.label);
    }

    #[test]
    fn carousel_orientation_default_vertical_and_roundtrip() {
        let conn = open_in_memory().unwrap();

        // Default path: `create_carousel` (no orientation arg) yields a
        // vertical carousel — this is what every legacy test relies on.
        let v = create_carousel(&conn, "vertical-deck").unwrap();
        assert_eq!(v.orientation, Orientation::Vertical);

        // Explicit landscape via the with_orientation helper.
        let l = create_carousel_with_orientation(&conn, "wide-deck", Orientation::Landscape)
            .unwrap();
        assert_eq!(l.orientation, Orientation::Landscape);

        // Toggle back and forth.
        update_carousel_orientation(&conn, &v.id, Orientation::Landscape).unwrap();
        let v_after = get_carousel(&conn, &v.id).unwrap().unwrap();
        assert_eq!(v_after.orientation, Orientation::Landscape);

        update_carousel_orientation(&conn, &l.id, Orientation::Vertical).unwrap();
        let l_after = get_carousel(&conn, &l.id).unwrap().unwrap();
        assert_eq!(l_after.orientation, Orientation::Vertical);
    }

    #[test]
    fn carousel_orientation_check_constraint_rejects_garbage() {
        let conn = open_in_memory().unwrap();
        let c = create_carousel(&conn, "checkme").unwrap();
        let res = conn.execute(
            "UPDATE carousels SET orientation = 'square' WHERE id = ?1",
            params![c.id],
        );
        assert!(
            res.is_err(),
            "CHECK constraint should reject orientations outside ('vertical','landscape')"
        );
    }

    #[test]
    fn duplicate_carousel_copies_orientation() {
        let mut conn = open_in_memory().unwrap();
        let src =
            create_carousel_with_orientation(&conn, "source-wide", Orientation::Landscape).unwrap();
        let dup = duplicate_carousel(&mut conn, &src.id).unwrap();
        assert_eq!(dup.orientation, Orientation::Landscape);
    }

    #[test]
    fn carousel_models_default_null_and_roundtrip() {
        let conn = open_in_memory().unwrap();
        let c = create_carousel(&conn, "modeled").unwrap();
        // Default: NULL/None for both models — driver falls back to DEFAULT_MODEL.
        assert!(c.impl_model.is_none());
        assert!(c.manager_model.is_none());

        update_carousel_models(
            &conn,
            &c.id,
            Some("claude-sonnet-4-6"),
            Some("claude-opus-4-7"),
        )
        .unwrap();
        let after = get_carousel(&conn, &c.id).unwrap().unwrap();
        assert_eq!(after.impl_model.as_deref(), Some("claude-sonnet-4-6"));
        assert_eq!(after.manager_model.as_deref(), Some("claude-opus-4-7"));

        // Empty / whitespace strings normalize back to NULL (auto-default).
        update_carousel_models(&conn, &c.id, Some(""), Some("   ")).unwrap();
        let cleared = get_carousel(&conn, &c.id).unwrap().unwrap();
        assert!(cleared.impl_model.is_none());
        assert!(cleared.manager_model.is_none());

        // None also clears to NULL.
        update_carousel_models(&conn, &c.id, Some("claude-opus-4-7"), Some("claude-opus-4-7")).unwrap();
        update_carousel_models(&conn, &c.id, None, None).unwrap();
        let none_cleared = get_carousel(&conn, &c.id).unwrap().unwrap();
        assert!(none_cleared.impl_model.is_none());
        assert!(none_cleared.manager_model.is_none());
    }
}
