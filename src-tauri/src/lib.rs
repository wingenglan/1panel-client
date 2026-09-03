#![allow(clippy::result_large_err)]

mod app;
mod commands;
mod domain;
mod errors;
mod infra;
mod security;

use tauri::Manager;
use tracing_subscriber::EnvFilter;

/** 初始化每次启动覆盖的本地应用日志文件；无法创建日志文件时回退到标准错误输出。 */
fn initialize_logging(app: &tauri::AppHandle) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let log_file = app.path().app_log_dir().ok().and_then(|directory| {
        std::fs::create_dir_all(&directory).ok()?;
        let path = directory.join("1panel-client.log");
        std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)
            .ok()
    });
    if let Some(file) = log_file {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_target(false)
            .with_ansi(false)
            .without_time()
            .with_writer(file)
            .try_init();
    } else {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_target(false)
            .without_time()
            .try_init();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
/// 启动桌面应用并注册前端命令；初始化失败时将脱敏原因写入本地日志，便于诊断安装版退出。
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            initialize_logging(app.handle());
            let state = tauri::async_runtime::block_on(app::AppState::initialize(app.handle()))
                .inspect_err(|error| {
                    tracing::error!(code = error.code, details = ?error.details, "应用初始化失败");
                })?;
            tracing::info!("本地数据库及应用状态初始化完成");
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_servers,
            commands::diagnose_server_topology,
            commands::list_shortcuts,
            commands::save_shortcut,
            commands::delete_shortcut,
            commands::restore_default_shortcuts,
            commands::use_shortcut,
            commands::get_server,
            commands::list_server_groups,
            commands::create_server_group,
            commands::save_server,
            commands::duplicate_server,
            commands::delete_server,
            commands::connection_state,
            commands::connect_server,
            commands::reconnect_server,
            commands::trust_host_key,
            commands::disconnect_server,
            commands::get_system_overview,
            commands::get_overview_memo,
            commands::save_overview_memo,
            commands::get_metric_history,
            commands::save_task,
            commands::list_tasks,
            commands::clear_finished_tasks,
            commands::open_terminal,
            commands::write_terminal,
            commands::resize_terminal,
            commands::close_terminal,
            commands::list_remote_directory,
            commands::read_remote_text,
            commands::read_remote_image_preview,
            commands::read_remote_tail,
            commands::save_remote_text,
            commands::save_remote_text_privileged,
            commands::create_remote_entry,
            commands::rename_remote_entry,
            commands::remove_remote_entry,
            commands::chmod_remote,
            commands::create_remote_symlink,
            commands::copy_move_remote,
            commands::upload_remote,
            commands::download_remote,
            commands::cancel_transfer,
            commands::cancel_command_task,
            commands::get_operations,
            commands::terminate_process,
            commands::manage_service,
            commands::get_service_detail,
            commands::get_service_logs,
            commands::get_storage,
            commands::storage_action,
            commands::get_logs,
            commands::follow_logs,
            commands::export_servers,
            commands::import_servers,
            commands::export_diagnostics,
            commands::list_audit_events,
            commands::export_full_backup,
            commands::import_full_backup,
            commands::list_backup_accounts,
            commands::save_backup_account,
            commands::delete_backup_account,
            commands::test_backup_account,
            commands::upload_backup_artifact,
            commands::list_tools,
            commands::get_tool_install_plan,
            commands::install_tool,
            commands::get_nginx,
            commands::test_nginx_config,
            commands::probe_nginx_backend,
            commands::save_nginx_proxy,
            commands::get_docker,
            commands::get_docker_events,
            commands::docker_container_action,
            commands::docker_container_logs,
            commands::docker_container_inspect,
            commands::docker_container_stats,
            commands::docker_container_top,
            commands::docker_container_exec,
            commands::docker_container_follow_logs,
            commands::docker_resource_action,
            commands::docker_image_action,
            commands::docker_prune,
            commands::docker_resource_inspect,
            commands::docker_compose_action,
            commands::docker_compose_create,
            commands::docker_compose_save_yaml,
            commands::docker_compose_details,
            commands::docker_compose_logs,
            commands::docker_pull_image,
            commands::docker_build_image,
            commands::docker_run_container,
            commands::get_databases,
            commands::database_action,
            commands::backup_database,
            commands::restore_database,
            commands::database_user_action,
            commands::get_database_privileges,
            commands::get_database_privilege_diagnostic,
            commands::database_engine_action,
            commands::get_database_install_plan,
            commands::install_database_engine,
            commands::get_redis_data,
            commands::redis_diagnostic,
            commands::redis_data_action,
            commands::redis_value_action,
            commands::redis_complex_action,
            commands::redis_transfer_action,
            commands::redis_migration_action,
            commands::get_cronjobs,
            commands::export_cronjobs,
            commands::import_cronjobs,
            commands::save_cronjob,
            commands::cronjob_action,
            commands::get_cronjob_history,
            commands::clear_cronjob_history,
            commands::get_cron_notification_settings,
            commands::save_cron_notification_settings,
            commands::get_cron_offline_scheduler_settings,
            commands::save_cron_offline_scheduler_settings,
            commands::get_security,
            commands::firewall_rule_action,
            commands::save_ssh_security,
            commands::get_websites,
            commands::get_certificate_renewal_plan,
            commands::save_website,
            commands::website_action,
            commands::website_nginx_service,
            commands::website_certificate_action,
            commands::bind_website_certificate,
            commands::get_php_install_plan,
            commands::install_php_runtime,
            commands::get_advanced,
            commands::probe_http_monitor,
            commands::get_http_monitors,
            commands::save_http_monitor,
            commands::delete_http_monitor,
            commands::run_http_monitor,
            commands::get_http_monitor_history,
            commands::get_waf_rules,
            commands::get_waf_rule_sources,
            commands::get_waf_templates,
            commands::get_waf_alerts,
            commands::get_waf_alert_settings,
            commands::save_waf_alert_settings,
            commands::clear_waf_alert_history,
            commands::waf_rule_action,
            commands::waf_template_action,
            commands::waf_rule_source_action,
            commands::get_app_catalog,
            commands::get_app_detail,
            commands::get_appstore_settings,
            commands::save_appstore_settings,
            commands::generate_appstore_mirror,
            commands::clear_appstore_cache,
            commands::get_installed_apps,
            commands::get_app_health,
            commands::app_update_preview,
            commands::get_app_environment,
            commands::save_app_environment,
            commands::install_app,
            commands::app_action,
            commands::list_ai_providers,
            commands::save_ai_provider,
            commands::delete_ai_provider,
            commands::list_ai_conversations,
            commands::save_ai_conversation,
            commands::delete_ai_conversation,
            commands::clear_ai_conversations,
            commands::ai_models,
            commands::ai_chat,
            commands::ai_chat_stream,
            commands::ai_agent,
            commands::list_ai_mcp_servers,
            commands::save_ai_mcp_server,
            commands::delete_ai_mcp_server,
            commands::probe_ai_mcp_server,
        ])
        .run(tauri::generate_context!())
        .expect("fatal application bootstrap error");
}
