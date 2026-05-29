use crate::generation;
use crate::sources::{claude_code, git};
use crate::types::{
    Activity, Carousel, Config, Entry, Orientation, Slide, SlideFeedback, SlideVersion,
};
use chrono::Local;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::Manager;

fn open_db() -> Result<rusqlite::Connection, String> {
    let dir = crate::config::default_base_dir().map_err(|e| e.to_string())?;
    crate::db::open(&dir).map_err(|e| e.to_string())
}

fn today_date() -> String {
    Local::now().date_naive().format("%Y-%m-%d").to_string()
}

#[tauri::command]
pub async fn fetch_today_activities() -> Result<Vec<Activity>, String> {
    let cfg = crate::config::load_config().map_err(|e| e.to_string())?;
    let reference = Local::now();

    let mut activities: Vec<Activity> = Vec::new();
    for repo in &cfg.git.repos {
        match git::fetch_today(repo) {
            Ok(mut xs) => activities.append(&mut xs),
            Err(e) => eprintln!(
                "happycampr: git fetch failed for {}: {e}",
                repo.display()
            ),
        }
    }
    match claude_code::scan_projects(&cfg.claude_code.projects_dir, reference) {
        Ok(mut xs) => activities.append(&mut xs),
        Err(e) => eprintln!("happycampr: claude scan failed: {e}"),
    }
    activities.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    Ok(activities)
}

#[tauri::command]
pub async fn load_today_entry() -> Result<Option<Entry>, String> {
    let dir = crate::config::default_base_dir().map_err(|e| e.to_string())?;
    let conn = crate::db::open(&dir).map_err(|e| e.to_string())?;
    crate::db::load_entry(&conn, &today_date()).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn save_entry(
    activities: Vec<Activity>,
    thoughts: String,
) -> Result<(), String> {
    let dir = crate::config::default_base_dir().map_err(|e| e.to_string())?;
    let conn = crate::db::open(&dir).map_err(|e| e.to_string())?;
    crate::db::save_entry(&conn, &today_date(), &activities, &thoughts)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn load_config() -> Result<Config, String> {
    crate::config::load_config().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn save_config(config: Config) -> Result<(), String> {
    crate::config::save_config(&config).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_carousels() -> Result<Vec<Carousel>, String> {
    let conn = open_db()?;
    crate::db::list_carousels(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_carousel(id: String) -> Result<Option<Carousel>, String> {
    let conn = open_db()?;
    crate::db::get_carousel(&conn, &id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn create_carousel(
    label: String,
    orientation: Option<String>,
) -> Result<Carousel, String> {
    let conn = open_db()?;
    let orientation = match orientation.as_deref() {
        Some(s) => Orientation::from_str(s)
            .ok_or_else(|| format!("invalid orientation: {s}"))?,
        None => Orientation::Vertical,
    };
    crate::db::create_carousel_with_orientation(&conn, &label, orientation)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn rename_carousel(id: String, label: String) -> Result<(), String> {
    let conn = open_db()?;
    crate::db::rename_carousel(&conn, &id, &label).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_carousel(id: String) -> Result<(), String> {
    let conn = open_db()?;
    crate::db::delete_carousel(&conn, &id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn duplicate_carousel(id: String) -> Result<Carousel, String> {
    let mut conn = open_db()?;
    crate::db::duplicate_carousel(&mut conn, &id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn update_carousel_models(
    id: String,
    impl_model: Option<String>,
    manager_model: Option<String>,
) -> Result<(), String> {
    let conn = open_db()?;
    crate::db::update_carousel_models(
        &conn,
        &id,
        impl_model.as_deref(),
        manager_model.as_deref(),
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn update_carousel_orientation(
    id: String,
    orientation: String,
) -> Result<(), String> {
    let conn = open_db()?;
    let orientation = Orientation::from_str(&orientation)
        .ok_or_else(|| format!("invalid orientation: {orientation}"))?;
    crate::db::update_carousel_orientation(&conn, &id, orientation).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_slides(carousel_id: String) -> Result<Vec<Slide>, String> {
    let conn = open_db()?;
    crate::db::list_slides(&conn, &carousel_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn create_slide(carousel_id: String) -> Result<Slide, String> {
    let conn = open_db()?;
    crate::db::create_slide(&conn, &carousel_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn update_slide_content(id: String, content: String) -> Result<(), String> {
    let conn = open_db()?;
    crate::db::update_slide_content(&conn, &id, &content).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn update_slide_title(
    id: String,
    title: Option<String>,
) -> Result<(), String> {
    let conn = open_db()?;
    crate::db::update_slide_title(&conn, &id, title.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_slide(id: String) -> Result<(), String> {
    let conn = open_db()?;
    crate::db::delete_slide(&conn, &id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn reorder_slides(
    carousel_id: String,
    ordered_ids: Vec<String>,
) -> Result<(), String> {
    let mut conn = open_db()?;
    crate::db::reorder_slides(&mut conn, &carousel_id, &ordered_ids)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_slide_versions(slide_id: String) -> Result<Vec<SlideVersion>, String> {
    let conn = open_db()?;
    crate::db::list_slide_versions(&conn, &slide_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_slide_feedback(
    slide_version_id: String,
) -> Result<Vec<SlideFeedback>, String> {
    let conn = open_db()?;
    crate::db::list_slide_feedback(&conn, &slide_version_id).map_err(|e| e.to_string())
}

/// Per-carousel handle to the bun driver subprocess.
///
/// The watcher thread polls `try_wait()` under the same lock that
/// `cancel_generation` uses to call `kill()` — so a cancel raced against
/// a natural exit is harmless: whoever wins the lock sees the truth.
pub struct ProcHandle {
    pub inner: Arc<Mutex<Option<Child>>>,
}

#[derive(Default)]
pub struct GenerationProcesses(pub Mutex<HashMap<String, ProcHandle>>);

#[tauri::command]
pub async fn generate_carousel_pdf(
    app: tauri::AppHandle,
    carousel_id: String,
) -> Result<(), String> {
    // 1. Refuse if a run is already in flight for this carousel.
    {
        let state = app.state::<GenerationProcesses>();
        let mut map = state.0.lock().map_err(|e| e.to_string())?;
        if let Some(handle) = map.get(&carousel_id) {
            let inner = handle.inner.lock().map_err(|e| e.to_string())?;
            if inner.is_some() {
                return Err("Generation is already running for this carousel".into());
            }
            // Stale entry from a previous run that finished; drop it.
            drop(inner);
            map.remove(&carousel_id);
        }
    }

    // 2. Validate inputs, pick the run dir, and persist run-start state.
    //    This is the Tauri-free core shared with `happycampr-carousels-cli`.
    let base_dir = crate::config::default_base_dir().map_err(|e| e.to_string())?;
    let root = generation::repo_root()?;
    let ctx = generation::prepare_run(&base_dir, &root, &carousel_id)?;

    // 3. Spawn bun. `spawn_driver` tees stdout/stderr into run.log and, on
    //    spawn failure, marks the carousel failed before returning Err.
    let child = generation::spawn_driver(
        &base_dir,
        &root,
        &carousel_id,
        &ctx.run_dir,
        &ctx.log_path,
    )?;

    // 4. Park the child in the GenerationProcesses map and start the watcher.
    let proc = Arc::new(Mutex::new(Some(child)));
    {
        let state = app.state::<GenerationProcesses>();
        let mut map = state.0.lock().map_err(|e| e.to_string())?;
        map.insert(
            carousel_id.clone(),
            ProcHandle {
                inner: proc.clone(),
            },
        );
    }

    let app_handle = app.clone();
    let watcher_carousel_id = carousel_id.clone();
    let watcher_log_path = ctx.log_path.clone();
    let watcher_proc = proc.clone();
    thread::spawn(move || {
        // Every exit path from this thread funnels through finalize_run:
        // labelled-break out of the poll loop with a reason string, then
        // run cleanup unconditionally. No silent returns.
        // (Per CLAUDE.md §conventions/errors: supervisor threads MUST flip
        // the observable state they own on every exit path.)
        let exit_reason: String = 'wait: loop {
            thread::sleep(Duration::from_millis(150));
            let mut guard = match watcher_proc.lock() {
                Ok(g) => g,
                Err(e) => break 'wait format!("mutex poisoned: {e}"),
            };
            let child = match guard.as_mut() {
                Some(c) => c,
                None => break 'wait "child handle missing".to_string(),
            };
            match child.try_wait() {
                Ok(Some(s)) => break 'wait format!("bun driver exited with {s}"),
                Ok(None) => continue,
                Err(e) => break 'wait format!("try_wait error: {e}"),
            }
        };

        finalize_run(
            &app_handle,
            &watcher_carousel_id,
            &watcher_log_path,
            &watcher_proc,
            &exit_reason,
        );
    });

    Ok(())
}

/// Single point of cleanup for the supervisor thread.
///
/// Runs unconditionally on every watcher exit path, including the "this
/// can't happen" branches (mutex poisoning, missing child, try_wait error).
/// Clears the Tauri-side handle/map so a fresh run can start, then
/// delegates the DB crash-finalize safety net to `generation::finalize`,
/// which is the one place that decision lives (shared with the CLI).
/// Idempotent: if the carousel already reached a terminal state, the DB
/// half is a no-op.
fn finalize_run(
    app: &tauri::AppHandle,
    carousel_id: &str,
    log_path: &Path,
    proc: &Arc<Mutex<Option<Child>>>,
    reason: &str,
) {
    // Drop the Child from the local handle so a fresh run can start.
    match proc.lock() {
        Ok(mut guard) => {
            *guard = None;
        }
        Err(e) => {
            let msg = format!("[supervisor] could not clear proc handle: {e}");
            eprintln!("{msg}");
            generation::append_log_line(log_path, &msg);
        }
    }

    // Remove from the GenerationProcesses map so a re-run isn't blocked.
    match app.state::<GenerationProcesses>().0.lock() {
        Ok(mut map) => {
            map.remove(carousel_id);
        }
        Err(e) => {
            let msg = format!("[supervisor] could not remove from process map: {e}");
            eprintln!("{msg}");
            generation::append_log_line(log_path, &msg);
        }
    }

    // DB safety net (shared with the CLI): if the bun driver died before
    // writing a terminal status, flip 'generating' → 'failed'. We ALWAYS
    // call generation::finalize so the `[exit]` line is written on every
    // path; if default_base_dir fails (macOS-impossible) we surface it and
    // pass None so finalize still logs the exit + notes the skipped DB net
    // — restoring the original "log-first, always finalize" invariant.
    let base_dir = match crate::config::default_base_dir() {
        Ok(d) => Some(d),
        Err(e) => {
            let msg = format!(
                "[supervisor] default_base_dir during finalize failed: {e}"
            );
            eprintln!("{msg}");
            generation::append_log_line(log_path, &msg);
            None
        }
    };
    generation::finalize(base_dir.as_deref(), carousel_id, log_path, reason);
}

/// Read the last `max_lines` lines of the carousel's `run.log`.
///
/// Returns an empty string if the carousel has no `run_dir` yet (no run
/// has started) or the file is missing.
#[tauri::command]
pub async fn read_run_log_tail(
    carousel_id: String,
    max_lines: usize,
) -> Result<String, String> {
    let conn = open_db()?;
    let carousel = match crate::db::get_carousel(&conn, &carousel_id)
        .map_err(|e| e.to_string())?
    {
        Some(c) => c,
        None => return Err(format!("carousel {carousel_id} not found")),
    };
    let Some(run_dir) = carousel.run_dir else {
        return Ok(String::new());
    };
    let log_path = PathBuf::from(run_dir).join("run.log");
    if !log_path.exists() {
        return Ok(String::new());
    }
    let contents = std::fs::read_to_string(&log_path)
        .map_err(|e| format!("reading {}: {e}", log_path.display()))?;
    if max_lines == 0 {
        return Ok(contents);
    }
    let lines: Vec<&str> = contents.lines().collect();
    let start = lines.len().saturating_sub(max_lines);
    Ok(lines[start..].join("\n"))
}

#[tauri::command]
pub async fn open_pdf(path: String) -> Result<(), String> {
    let p = std::path::Path::new(&path);
    if !p.exists() {
        return Err(format!("PDF not found: {path}"));
    }
    Command::new("open")
        .arg(p)
        .spawn()
        .map_err(|e| format!("opening pdf: {e}"))?;
    Ok(())
}

#[tauri::command]
pub async fn open_path(path: String) -> Result<(), String> {
    let p = std::path::Path::new(&path);
    if !p.exists() {
        return Err(format!("Path not found: {path}"));
    }
    Command::new("open")
        .arg(p)
        .spawn()
        .map_err(|e| format!("opening path: {e}"))?;
    Ok(())
}

#[tauri::command]
pub async fn read_slide_screenshot_data_url(
    slide_id: String,
) -> Result<Option<String>, String> {
    let conn = open_db()?;
    let latest = crate::db::get_latest_slide_version(&conn, &slide_id)
        .map_err(|e| e.to_string())?;
    let Some(version) = latest else { return Ok(None) };
    let Some(path) = version.screenshot_path else { return Ok(None) };
    let bytes = std::fs::read(&path)
        .map_err(|e| format!("reading screenshot {path}: {e}"))?;
    use base64::Engine as _;
    let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(Some(format!("data:image/png;base64,{encoded}")))
}

#[tauri::command]
pub async fn read_slide_html(slide_id: String) -> Result<Option<String>, String> {
    let conn = open_db()?;
    let latest = crate::db::get_latest_slide_version(&conn, &slide_id)
        .map_err(|e| e.to_string())?;
    let Some(version) = latest else { return Ok(None) };
    let html = std::fs::read_to_string(&version.html_path)
        .map_err(|e| format!("reading html {}: {e}", version.html_path))?;
    Ok(Some(html))
}

#[tauri::command]
pub async fn cancel_generation(
    app: tauri::AppHandle,
    carousel_id: String,
) -> Result<(), String> {
    // Look up the process handle without removing it — the watcher thread
    // owns the lifecycle and will reap on next try_wait(). If there's no
    // handle, the run already finished (done OR failed); cancel is a no-op
    // in that case. Do NOT defensively flip status here — that would turn a
    // completed 'done' into 'failed'.
    let state = app.state::<GenerationProcesses>();
    let map = state.0.lock().map_err(|e| e.to_string())?;
    let Some(handle) = map.get(&carousel_id) else {
        return Ok(());
    };
    let inner = handle.inner.clone();
    drop(map);

    // Send SIGKILL via Child::kill while we hold the inner lock.
    let mut guard = inner.lock().map_err(|e| e.to_string())?;
    if let Some(child) = guard.as_mut() {
        if let Err(e) = child.kill() {
            // ESRCH (already exited) is harmless; anything else is worth a line
            // but don't fail the cancel — the watcher will still reap.
            eprintln!("happycampr: cancel_generation kill: {e}");
        }
    }
    drop(guard);

    // The watcher thread will detect the exit on its next poll and flip
    // status to 'failed'. We don't have to wait for that here.
    Ok(())
}
