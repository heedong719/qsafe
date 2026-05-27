// Prevent the Windows release build from opening a console window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;

fn main() {
    tracing_subscriber::fmt::init();

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::about,
            commands::file_info,
            commands::identity_generate,
            commands::identity_show,
        ])
        .run(tauri::generate_context!())
        .expect("error while running qsafe GUI");
}
