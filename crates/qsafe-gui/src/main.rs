#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;

fn main() {
    tracing_subscriber::fmt::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::about,
            commands::file_info,
            commands::identity_generate,
            commands::identity_show,
            commands::mnemonic_generate,
            commands::mnemonic_verify,
            commands::default_identity_path,
            commands::pack_one,
            commands::pack_path,
            commands::pack_to_zip,
            commands::unpack_qsafe,
            commands::qsafe_info,
            commands::list_external_archive,
            commands::extract_external_archive,
            commands::list_drives,
            commands::home_dir,
            commands::list_directory,
            commands::current_dir,
            commands::delete_path,
            commands::open_with_associated,
            commands::pack_path_ext,
            commands::unpack_qsafe_ext,
            commands::md5_of_file,
            commands::iso_mount,
            commands::iso_unmount,
            commands::list_writable_disks,
        ])
        .run(tauri::generate_context!())
        .expect("error while running qsafe GUI");
}
