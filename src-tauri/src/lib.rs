pub mod cli;
pub mod commands;
pub mod config;
pub mod db;
pub mod generation;
pub mod sources;
pub mod types;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(commands::GenerationProcesses::default())
        .invoke_handler(tauri::generate_handler![
            commands::fetch_today_activities,
            commands::load_today_entry,
            commands::save_entry,
            commands::load_config,
            commands::save_config,
            commands::list_carousels,
            commands::get_carousel,
            commands::create_carousel,
            commands::rename_carousel,
            commands::delete_carousel,
            commands::duplicate_carousel,
            commands::update_carousel_models,
            commands::update_carousel_orientation,
            commands::list_slides,
            commands::create_slide,
            commands::update_slide_content,
            commands::update_slide_title,
            commands::delete_slide,
            commands::reorder_slides,
            commands::list_slide_versions,
            commands::list_slide_feedback,
            commands::generate_carousel_pdf,
            commands::cancel_generation,
            commands::read_run_log_tail,
            commands::open_pdf,
            commands::open_path,
            commands::read_slide_screenshot_data_url,
            commands::read_slide_html,
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
