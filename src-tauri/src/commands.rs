use crate::sources::{claude_code, git};
use crate::types::{Activity, Carousel, Config, Entry, Slide};
use chrono::Local;

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

