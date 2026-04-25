use crate::sources::{claude_code, git};
use crate::types::{
    Activity, Carousel, CarouselStatus, Config, Entry, Slide, SlideFeedback, SlideVersion,
};
use chrono::Local;
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
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
                "cantalog: git fetch failed for {}: {e}",
                repo.display()
            ),
        }
    }
    match claude_code::scan_projects(&cfg.claude_code.projects_dir, reference) {
        Ok(mut xs) => activities.append(&mut xs),
        Err(e) => eprintln!("cantalog: claude scan failed: {e}"),
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
pub async fn create_carousel(label: String) -> Result<Carousel, String> {
    let conn = open_db()?;
    crate::db::create_carousel(&conn, &label).map_err(|e| e.to_string())
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

fn repo_root() -> Result<PathBuf, String> {
    // CARGO_MANIFEST_DIR is /…/social-agent/src-tauri at build time.
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .map(|p| p.to_path_buf())
        .ok_or_else(|| "could not resolve repo root from CARGO_MANIFEST_DIR".to_string())
}

/// Pick the next `run-N` directory under `<base>/carousels/<slug>/`.
///
/// We scan the existing entries, take the max numeric suffix, add 1, and
/// `create_dir` (atomic — fails if it already exists). This is the only
/// place run-dir naming happens; the bun script just trusts the path it's
/// given.
fn next_run_dir(slug: &str) -> Result<PathBuf, String> {
    let base_dir = crate::config::default_base_dir().map_err(|e| e.to_string())?;
    let carousel_dir = crate::config::carousels_dir(&base_dir).join(slug);
    std::fs::create_dir_all(&carousel_dir)
        .map_err(|e| format!("creating {}: {e}", carousel_dir.display()))?;

    let mut max = 0u32;
    for entry in std::fs::read_dir(&carousel_dir)
        .map_err(|e| format!("reading {}: {e}", carousel_dir.display()))?
    {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        let Some(s) = name.to_str() else { continue };
        if let Some(rest) = s.strip_prefix("run-") {
            if let Ok(n) = rest.parse::<u32>() {
                if n > max {
                    max = n;
                }
            }
        }
    }
    let next = max + 1;
    let dir = carousel_dir.join(format!("run-{next}"));
    std::fs::create_dir(&dir).map_err(|e| {
        format!(
            "creating run dir {}: {e} (race with another driver?)",
            dir.display()
        )
    })?;
    Ok(dir)
}

/// Tee a child pipe into both `run.log` and the parent's stderr,
/// line-by-line, prefixing each line so the source is unambiguous.
///
/// Returning early on a read error is fine — the child has either exited
/// or its pipe has gone away; the watcher thread will pick that up.
fn spawn_pipe_tee(
    label: &'static str,
    pipe: impl std::io::Read + Send + 'static,
    log_path: PathBuf,
) {
    thread::spawn(move || {
        let reader = BufReader::new(pipe);
        for line_res in reader.lines() {
            let line = match line_res {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("cantalog: pipe-tee[{label}] read error: {e}");
                    return;
                }
            };
            let stamped = format!(
                "[{}] [{}] {}\n",
                chrono::Utc::now().to_rfc3339(),
                label,
                line
            );
            // Always echo to parent stderr so `tauri dev` shows it inline.
            eprint!("{stamped}");
            // And append to the run.log file for the UI to read.
            match OpenOptions::new().create(true).append(true).open(&log_path) {
                Ok(mut f) => {
                    if let Err(e) = f.write_all(stamped.as_bytes()) {
                        eprintln!(
                            "cantalog: pipe-tee[{label}] log write failed ({}): {e}",
                            log_path.display()
                        );
                    }
                }
                Err(e) => {
                    eprintln!(
                        "cantalog: pipe-tee[{label}] log open failed ({}): {e}",
                        log_path.display()
                    );
                }
            }
        }
    });
}

/// Append a freshly-stamped line to `run.log`. Used by the supervisor
/// thread to record `[exit]` lines that didn't come through a pipe.
fn append_log_line(log_path: &Path, line: &str) {
    let stamped = format!("[{}] {}\n", chrono::Utc::now().to_rfc3339(), line);
    eprint!("{stamped}");
    match OpenOptions::new().create(true).append(true).open(log_path) {
        Ok(mut f) => {
            if let Err(e) = f.write_all(stamped.as_bytes()) {
                eprintln!(
                    "cantalog: append_log_line failed ({}): {e}",
                    log_path.display()
                );
            }
        }
        Err(e) => {
            eprintln!(
                "cantalog: append_log_line open failed ({}): {e}",
                log_path.display()
            );
        }
    }
}

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

    // 2. Sanity-check inputs and pick the run dir up front.
    let root = repo_root()?;
    let script = root.join("scripts").join("generate-carousel.ts");
    if !script.exists() {
        return Err(format!("driver script not found at {}", script.display()));
    }

    let conn = open_db()?;
    let carousel = crate::db::get_carousel(&conn, &carousel_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("carousel {carousel_id} not found"))?;
    let slug = carousel
        .slug
        .clone()
        .ok_or_else(|| format!("carousel {carousel_id} has no slug — reopen the app once"))?;
    let slides = crate::db::list_slides(&conn, &carousel_id).map_err(|e| e.to_string())?;
    if slides.is_empty() {
        return Err("Carousel has no slides".into());
    }
    drop(conn);

    let run_dir = next_run_dir(&slug)?;
    let log_path = run_dir.join("run.log");
    append_log_line(
        &log_path,
        &format!(
            "[supervisor] starting bun driver for carousel \"{}\" ({} slide(s))",
            carousel.label,
            slides.len()
        ),
    );

    // Persist run start so the UI can find run.log immediately, even if bun
    // crashes before its first DB write. Also reset every slide's status to
    // 'queued' so the progress UI doesn't show stale ✓/✗ from a prior run
    // for slides the new driver hasn't reached yet.
    {
        let conn = open_db()?;
        crate::db::set_carousel_run_started(
            &conn,
            &carousel_id,
            run_dir.to_string_lossy().as_ref(),
        )
        .map_err(|e| e.to_string())?;
        crate::db::reset_slides_for_run(&conn, &carousel_id)
            .map_err(|e| e.to_string())?;
    }

    // 3. Spawn bun. Rust now owns the run-dir picker; bun just trusts the
    // path passed as argv[2].
    let mut child = Command::new("bun")
        .arg("run")
        .arg(&script)
        .arg(&carousel_id)
        .arg(&run_dir)
        .current_dir(&root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            // Mark the carousel failed so the UI doesn't sit on 'generating'.
            if let Ok(conn) = open_db() {
                let _ = crate::db::set_carousel_run_finished(
                    &conn,
                    &carousel_id,
                    CarouselStatus::Failed,
                    None,
                );
            }
            append_log_line(
                &log_path,
                &format!("[supervisor] spawn failed: {e}"),
            );
            format!("spawning bun driver: {e}")
        })?;

    // 4. Drain stdout + stderr into run.log (and into our own stderr).
    if let Some(stdout) = child.stdout.take() {
        spawn_pipe_tee("bun-stdout", stdout, log_path.clone());
    }
    if let Some(stderr) = child.stderr.take() {
        spawn_pipe_tee("bun-stderr", stderr, log_path.clone());
    }

    // 5. Park the child in the GenerationProcesses map and start the watcher.
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
    let watcher_log_path = log_path.clone();
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
/// Runs unconditionally on every exit path, including the "this can't
/// happen" branches (mutex poisoning, missing child, try_wait error).
/// Idempotent: if the carousel is already in a terminal state, this is
/// a no-op.
fn finalize_run(
    app: &tauri::AppHandle,
    carousel_id: &str,
    log_path: &Path,
    proc: &Arc<Mutex<Option<Child>>>,
    reason: &str,
) {
    append_log_line(log_path, &format!("[supervisor] [exit] {reason}"));

    // Drop the Child from the local handle so a fresh run can start.
    match proc.lock() {
        Ok(mut guard) => {
            *guard = None;
        }
        Err(e) => {
            let msg = format!("[supervisor] could not clear proc handle: {e}");
            eprintln!("{msg}");
            append_log_line(log_path, &msg);
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
            append_log_line(log_path, &msg);
        }
    }

    // ALWAYS check carousel status. If still 'generating', the bun
    // script didn't get to call set_carousel_run_finished — mark
    // failed loudly so the UI doesn't sit on a stuck status.
    let conn = match open_db() {
        Ok(c) => c,
        Err(e) => {
            let msg = format!(
                "[supervisor] open_db during finalize failed: {e} \
                 — carousel status may be stuck"
            );
            eprintln!("{msg}");
            append_log_line(log_path, &msg);
            return;
        }
    };
    let status = match crate::db::get_carousel(&conn, carousel_id) {
        Ok(Some(c)) => c.status,
        Ok(None) => {
            let msg = "[supervisor] carousel disappeared from db";
            eprintln!("{msg}");
            append_log_line(log_path, msg);
            return;
        }
        Err(e) => {
            let msg = format!("[supervisor] get_carousel during finalize: {e}");
            eprintln!("{msg}");
            append_log_line(log_path, &msg);
            return;
        }
    };
    if status == CarouselStatus::Generating {
        match crate::db::set_carousel_run_finished(
            &conn,
            carousel_id,
            CarouselStatus::Failed,
            None,
        ) {
            Ok(()) => {
                append_log_line(
                    log_path,
                    "[supervisor] marked carousel failed (status was still 'generating')",
                );
            }
            Err(e) => {
                let msg = format!(
                    "[supervisor] FAILED to mark carousel failed: {e} \
                     — UI will show 'generating' until manually fixed"
                );
                eprintln!("{msg}");
                append_log_line(log_path, &msg);
            }
        }
    } else {
        append_log_line(
            log_path,
            &format!(
                "[supervisor] carousel already in terminal state '{:?}'",
                status
            ),
        );
    }
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
            eprintln!("cantalog: cancel_generation kill: {e}");
        }
    }
    drop(guard);

    // The watcher thread will detect the exit on its next poll and flip
    // status to 'failed'. We don't have to wait for that here.
    Ok(())
}
