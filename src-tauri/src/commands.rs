use crate::sources::{claude_code, git};
use crate::types::{Activity, Config, Entry};
use chrono::Local;

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

// Force full-process termination. macOS keeps Tauri apps alive after the last window
// closes (standard AppKit behavior), which conflicts with the spec's "app closes itself
// after save". Calling AppHandle::exit(0) terminates the process so the dock icon goes
// away and the next launch is a fresh state.
//
// The exit is scheduled on a detached thread with a small delay so the IPC response to
// this command can flush back to the WebView before the runtime is torn down. Calling
// app.exit(0) inline caused the WebView to briefly render a "disconnected" error page
// (user-reported as a "crash" even though the DB write completed cleanly).
#[tauri::command]
pub async fn quit_app(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;
    for (_, window) in app.webview_windows() {
        let _ = window.hide();
    }
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(60));
        app.exit(0);
    });
    Ok(())
}
