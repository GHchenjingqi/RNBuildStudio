mod project;
mod tasks;

pub fn run() {
    tauri::Builder::default()
        .manage(tasks::TaskRegistry::default())
        .invoke_handler(tauri::generate_handler![
            tasks::run_task,
            tasks::stop_task,
            project::list_apps,
            project::unregister_app,
            project::read_app_config,
            project::write_app_config,
            project::detect_base_dir,
            project::validate_base_dir
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
