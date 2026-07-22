#![allow(linker_messages)] // MSVC reports normal import-library creation on stdout.

pub mod ai;
mod commands;
mod ssh;
mod state;
pub mod sync;
pub mod vault;

use log::LevelFilter;
use state::AppState;
use tauri::Manager;
use tauri_plugin_log::{RotationStrategy, Target, TargetKind};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Keep the current 10 MiB file plus two rotated files, capped at 30 MiB total.
    let exe_path = std::env::current_exe().expect("Failed to get exe path");
    let app_dir = exe_path
        .parent()
        .expect("Failed to get exe parent")
        .join("data");
    std::fs::create_dir_all(&app_dir).expect("Failed to create data dir");
    let log_dir = app_dir.join("logs");
    std::fs::create_dir_all(&log_dir).expect("Failed to create log dir");
    ai::log::initialize(log_dir.join("ai.log"));

    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(LevelFilter::Info)
                .level_for("russh", LevelFilter::Warn)
                .level_for("russh_sftp", LevelFilter::Warn)
                .rotation_strategy(RotationStrategy::KeepSome(2))
                .max_file_size(10 * 1024 * 1024)
                .targets([
                    Target::new(TargetKind::Stdout),
                    Target::new(TargetKind::Folder {
                        path: log_dir,
                        file_name: Some("app".into()),
                    }),
                ])
                .build(),
        )
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(move |app| {
            let app_state = AppState::new(app_dir);
            app.manage(app_state);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::ai::get_ai_config_status,
            commands::ai::save_ai_config,
            commands::ai::test_ai_connection,
            commands::ai::delete_ai_config,
            commands::ai::list_ai_agents,
            commands::ai::save_ai_agent,
            commands::ai::delete_ai_agent,
            commands::ai::start_ai_task,
            commands::ai::confirm_ai_risk_action,
            commands::ai::decide_ai_action,
            commands::ai::cancel_ai_task,
            commands::vault::init_vault,
            commands::vault::get_vault_md5,
            commands::sync::get_sync_status,
            commands::sync::enable_github_gist_sync,
            commands::sync::enable_gitee_snippet_sync,
            commands::sync::upload_sync_vault,
            commands::sync::download_sync_vault,
            commands::sync::set_auto_sync,
            commands::sync::update_local_sync_password,
            commands::sync::change_sync_password,
            commands::sync::resolve_sync_conflict,
            commands::sync::disable_sync,
            commands::sync::delete_remote_sync_vault,
            commands::vault::list_profiles,
            commands::vault::get_profile,
            commands::vault::create_profile,
            commands::vault::update_profile,
            commands::vault::refresh_profile_info,
            commands::vault::delete_profile,
            commands::vault::list_keys,
            commands::vault::create_key,
            commands::vault::update_key,
            commands::vault::delete_key,
            commands::ssh::connect_ssh,
            commands::ssh::disconnect_ssh,
            commands::ssh::write_ssh_data,
            commands::ssh::resize_ssh,
            commands::ssh::list_sessions,
            commands::sftp::open_sftp_window,
            commands::sftp::sftp_get_home_directory,
            commands::sftp::sftp_list_files,
            commands::sftp::sftp_create_directory,
            commands::sftp::sftp_rename,
            commands::sftp::sftp_delete,
            commands::sftp::sftp_set_permissions,
            commands::sftp::sftp_compress_tar_gz,
            commands::sftp::sftp_extract_tar_gz,
            commands::sftp::sftp_upload_file,
            commands::sftp::sftp_download_file,
            commands::sftp::sftp_local_file_exists,
            commands::sftp::get_default_download_directory,
            commands::sftp::get_server_stats,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
