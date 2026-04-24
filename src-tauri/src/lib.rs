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
            commands::list_carousels,
            commands::create_carousel,
            commands::rename_carousel,
            commands::delete_carousel,
            commands::list_slides,
            commands::create_slide,
            commands::update_slide_content,
            commands::delete_slide,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            // macOS default is to keep the app alive after the last window closes.
            // That's wrong for a single-window capture app: closing the window means
            // "I'm done". Exit the process so the next launch is a clean reboot.
            if let tauri::RunEvent::WindowEvent {
                event: tauri::WindowEvent::Destroyed,
                ..
            } = event
            {
                app_handle.exit(0);
            }
        });
}
