use crate::sources::{claude_code, git};
use crate::types::{
    Activity, Carousel, Config, Entry, Slide, SlideFeedback, SlideVersion,
};
use chrono::Local;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
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

/// Tracks one bun driver subprocess per carousel id so we can cancel it later.
#[derive(Default)]
pub struct GenerationProcesses(pub Mutex<HashMap<String, Child>>);

fn repo_root() -> Result<PathBuf, String> {
    // CARGO_MANIFEST_DIR is /…/social-agent/src-tauri at build time.
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .map(|p| p.to_path_buf())
        .ok_or_else(|| "could not resolve repo root from CARGO_MANIFEST_DIR".to_string())
}

#[tauri::command]
pub async fn generate_carousel_pdf(
    app: tauri::AppHandle,
    carousel_id: String,
) -> Result<(), String> {
    // Refuse if already running.
    {
        let state = app.state::<GenerationProcesses>();
        let mut map = state.0.lock().map_err(|e| e.to_string())?;
        if let Some(child) = map.get_mut(&carousel_id) {
            // Reap if it has already exited.
            match child.try_wait() {
                Ok(Some(_)) => {
                    map.remove(&carousel_id);
                }
                Ok(None) => {
                    return Err("Generation is already running for this carousel".into());
                }
                Err(e) => return Err(format!("checking child status: {e}")),
            }
        }
    }

    let root = repo_root()?;
    let script = root.join("scripts").join("generate-carousel.ts");
    if !script.exists() {
        return Err(format!(
            "driver script not found at {}",
            script.display()
        ));
    }

    let child = Command::new("bun")
        .arg("run")
        .arg(&script)
        .arg(&carousel_id)
        .current_dir(&root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("spawning bun driver: {e}"))?;

    let state = app.state::<GenerationProcesses>();
    let mut map = state.0.lock().map_err(|e| e.to_string())?;
    map.insert(carousel_id, child);
    Ok(())
}

#[tauri::command]
pub async fn cancel_generation(
    app: tauri::AppHandle,
    carousel_id: String,
) -> Result<(), String> {
    let state = app.state::<GenerationProcesses>();
    let mut map = state.0.lock().map_err(|e| e.to_string())?;
    if let Some(mut child) = map.remove(&carousel_id) {
        let _ = child.kill();
        let _ = child.wait();
        // Best-effort: also flip carousel status to failed so the UI doesn't
        // hang on 'generating' if the driver never updated it.
        if let Ok(conn) = open_db() {
            let _ = crate::db::set_carousel_run_finished(
                &conn,
                &carousel_id,
                crate::types::CarouselStatus::Failed,
                None,
            );
        }
    }
    Ok(())
}
