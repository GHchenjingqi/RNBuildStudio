mod brand;
mod project;
mod tasks;

pub fn run() {
    tauri::Builder::default()
        .manage(tasks::TaskRegistry::default())
        .invoke_handler(tauri::generate_handler![
            tasks::run_task,
            tasks::stop_task,
            tasks::ensure_release_keystore,
            tasks::verify_release_keystore,
            project::list_apps,
            project::unregister_app,
            project::read_app_config,
            project::write_app_config,
            project::detect_base_dir,
            project::validate_base_dir,
            project::scan_subapps,
            project::register_app,
            project::write_adhoc_config,
            brand::pick_image,
            brand::import_brand_images
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
