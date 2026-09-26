pub mod instances;
pub mod curseforge;
pub mod forge;
pub mod jvm;
pub mod java;
mod launch;
pub mod modpack;
pub mod modrinth;
mod neoforge;
mod sysinfo;
pub mod versions;

use std::sync::Mutex;
use tauri::Manager;

pub struct LauncherState {
    pub child: Mutex<Option<tokio::process::Child>>,
    pub cancel: tokio::sync::watch::Sender<bool>,
    /// Held for the whole launch (prepare + spawn) so two clicks cannot both
    /// pass the "already running" check.
    pub launch_lock: tokio::sync::Mutex<()>,
}

/// Moves `src` into `dst`, merging directories. Returns Err on any failure —
/// the legacy directory is only deleted when everything actually moved, so a
/// failed migration can never destroy the only copy of user data.
fn merge_move(src: &std::path::Path, dst: &std::path::Path) -> Result<(), String> {
    if !dst.exists() {
        return std::fs::rename(src, dst).map_err(|e| format!("{}: {}", src.display(), e));
    }
    if src.is_dir() && dst.is_dir() {
        let rd = std::fs::read_dir(src).map_err(|e| e.to_string())?;
        for e in rd.flatten() {
            merge_move(&e.path(), &dst.join(e.file_name()))?;
        }
        return std::fs::remove_dir(src).map_err(|e| e.to_string());
    }
    if src.is_file() && dst.is_file() {
        // keep the new copy, but preserve the old one next to it
        let bak = dst.with_extension("migrated-old");
        let _ = std::fs::remove_file(&bak);
        std::fs::rename(src, bak).map_err(|e| e.to_string())?;
        return Ok(());
    }
    Err(format!(
        "конфликт типов при миграции: {} vs {}",
        src.display(),
        dst.display()
    ))
}

fn migrate_legacy_data() {
    let Ok(home) = std::env::var("HOME") else { return };
    let old = std::path::Path::new(&home).join(".local/share/cachy-mc-launcher");
    let new = std::path::Path::new(&home).join(".local/share/debang-launcher");
    if !old.exists() {
        return;
    }
    if !new.exists() {
        if let Err(e) = std::fs::rename(&old, &new) {
            eprintln!("[deBang] миграция данных не удалась: {}", e);
        }
        return;
    }
    if let Ok(entries) = std::fs::read_dir(&old) {
        for e in entries.flatten() {
            if let Err(e) = merge_move(&e.path(), &new.join(e.file_name())) {
                eprintln!("[deBang] миграция: {}", e);
            }
        }
    }
    // only clean up when the legacy directory is actually empty
    match std::fs::read_dir(&old) {
        Ok(mut rd) => {
            if rd.next().is_none() {
                if let Err(e) = std::fs::remove_dir(&old) {
                    eprintln!("[deBang] не удалось удалить старый каталог: {}", e);
                }
            }
        }
        Err(e) => eprintln!("[deBang] проверка старого каталога: {}", e),
    }
}

pub fn run() {
    // A menu entry or a script may start us without a graphical session; say so
    // clearly instead of panicking inside GTK with "Failed to initialize".
    let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
    let x11 = std::env::var_os("DISPLAY").is_some();
    if !wayland && !x11 {
        eprintln!(
            "deBang Launcher: не найден WAYLAND_DISPLAY/DISPLAY — приложению нужна графическая сессия."
        );
        eprintln!("Подсказка: запускай через ярлык меню или из терминала внутри сессии Hyprland.");
        std::process::exit(1);
    }
    migrate_legacy_data();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(LauncherState {
            child: Mutex::new(None),
            cancel: tokio::sync::watch::channel(false).0,
            launch_lock: tokio::sync::Mutex::new(()),
        })
        .setup(|app| {
            // Allow the webview to read only the launcher's own background
            // directory; the config scope can only express the Linux path.
            let dir = versions::data_root().join("backgrounds");
            let _ = app.asset_protocol_scope().allow_directory(&dir, true);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            sysinfo::get_mem_info,
            sysinfo::get_sys_info,
            java::detect_java,
            java::check_java_version,
            modrinth::modrinth_search,
            modrinth::modrinth_project_versions,
            modrinth::mojang_versions,
            curseforge::curseforge_key_status,
            curseforge::curseforge_key_save,
            curseforge::curseforge_search,
            curseforge::curseforge_files,
            curseforge::curseforge_download_url,
            curseforge::curseforge_download_file,
            curseforge::curseforge_install_modpack,
            instances::list_instances,
            instances::create_instance,
            instances::delete_instance,
            instances::download_mod,
            instances::import_run_file,
            instances::import_background,
            instances::update_instance_settings,
            instances::list_instance_mods,
            instances::toggle_instance_mod,
            instances::delete_instance_mod,
            instances::open_instance_folder,
            launch::instance_launch_plan,
            modpack::install_modpack,
            launch::launch_instance,
            launch::stop_instance,
            launch::is_game_running,
            launch::cancel_download,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
