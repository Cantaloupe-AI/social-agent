pub mod commands;
pub mod config;
pub mod db;
pub mod sources;
pub mod types;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::fetch_today_activities,
            commands::load_today_entry,
            commands::save_entry,
            commands::load_config,
            commands::save_config,
            commands::quit_app,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
