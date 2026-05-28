#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;

fn main() {
    tracing_subscriber::fmt::init();

    // OS 통합용 startup argv 파싱:
    //   qsafe-gui                          → 일반 실행
    //   qsafe-gui <file>                   → 파일 자동 라우팅 (.qs/.iso/.기타)
    //   qsafe-gui --action=pack <file>     → 압축 모달 자동 열기 + 입력 prefill
    //   qsafe-gui --action=unpack <file>   → 풀기 모달 자동 열기 + 입력 prefill
    //   qsafe-gui --action=info <file>     → 정보 모달 자동 열기
    let args = commands::StartupArgs::parse(std::env::args().skip(1));

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(args)
        .invoke_handler(tauri::generate_handler![
            commands::startup_args,
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
            commands::extract_archive_entry_to_temp,
            commands::cleanup_temp_dir,
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
            commands::write_iso_to_disk,
        ])
        .run(tauri::generate_context!())
        .expect("error while running qsafe GUI");
}
