use crate::instances;
use crate::versions::{data_root, download_file};
use serde::Serialize;
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, State};
use crate::LauncherState;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PrepEvent {
    phase: String,
    done: u64,
    total: u64,
}

fn report(app: Option<&AppHandle>, phase: &str, done: u64, total: u64) {
    let Some(app) = app else { return };
    let _ = app.emit(
        "prep://progress",
        PrepEvent {
            phase: phase.into(),
            done,
            total,
        },
    );
}

fn sys_log(app: Option<&AppHandle>, line: String) {
    let Some(app) = app else { return };
    let _ = app.emit(
        "game://log",
        crate::launch::LogEvent {
            time: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            line,
            stream: "launcher".into(),
        },
    );
}

pub struct PackFile {
    pub path: String,
    pub url: Option<String>,
    pub size: Option<u64>,
    pub client: bool,
}

/// Supports Modrinth mrpack v1/v2 (`pack_version`), and CurseForge-flavoured
/// indexes (`formatVersion`/`game`/`dependencies`) that some packs ship.
pub fn parse_pack(vj: &Value) -> Result<(String, String, Vec<PackFile>), String> {
    let mc = vj["minecraft"]["version"]
        .as_str()
        .or_else(|| vj["dependencies"]["minecraft"].as_str())
        .or_else(|| vj["base_game_version"].as_str())
        .ok_or("в сборке нет версии Minecraft")?;
    let mc = mc.split('-').next().unwrap_or(mc).to_string();

    let loader = vj["minecraft"]
        .get("mod_loader")
        .and_then(|l| l["project"].as_str())
        .or_else(|| vj["loaders"].get(0).and_then(|l| l["id"].as_str()))
        .or_else(|| {
            vj["dependencies"]
                .as_object()
                .and_then(|d| d.keys().find(|k| k.contains("fabric") || k.contains("forge")).map(String::as_str))
        })
        .map(|p| {
            let pl = p.to_lowercase();
            if pl.contains("neoforge") {
                "NeoForge"
            } else if pl.contains("fabric") {
                "Fabric"
            } else if pl.contains("forge") {
                "Forge"
            } else {
                "Vanilla"
            }
        })
        .unwrap_or("Vanilla")
        .to_string();

    let mut files = Vec::new();
    if let Some(arr) = vj["files"].as_array() {
        for f in arr {
            let Some(path) = f["path"].as_str() else { continue };
            if instances::safe_join(std::path::Path::new("/"), path).is_err() {
                sys_log(None, format!("⚠ пропущен небезопасный путь в индексе: {}", path));
                continue;
            }
            let url = f["downloads"]
                .as_array()
                .and_then(|a| {
                    a.iter().find_map(|x| match x {
                        Value::String(s) => Some(s.clone()),
                        Value::Object(o) => o["url"].as_str().map(String::from),
                        _ => None,
                    })
                });
            let size = f["file_size"]
                .as_u64()
                .or_else(|| f["fileSize"].as_u64())
                .or_else(|| {
                    f["downloads"].as_array().and_then(|a| a.first()).and_then(|x| {
                        x.get("file_size").and_then(|v| v.as_u64())
                    })
                });
            let client = f["env"]["client"].as_str().unwrap_or("required") != "unsupported";
            files.push(PackFile {
                path: path.to_string(),
                url,
                size,
                client,
            });
        }
    }
    Ok((mc, loader, files))
}

#[tauri::command]
pub async fn install_modpack(
    app: AppHandle,
    state: State<'_, LauncherState>,
    url: String,
    filename: String,
    name: String,
) -> Result<String, String> {
    if !url.starts_with("https://cdn.modrinth.com/") && !PathBuf::from(&url).exists() {
        return Err("Разрешён https-URL с cdn.modrinth.com или локальный файл".into());
    }
    let _ = state.cancel.send(false);
    install_pack(Some(&app), url, filename, name, Some(state.cancel.subscribe())).await
}

pub async fn install_pack(
    app: Option<&AppHandle>,
    url: String,
    filename: String,
    name: String,
    cancel: Option<tokio::sync::watch::Receiver<bool>>,
) -> Result<String, String> {
    let root = data_root();
    let dl_dir = root.join("modpacks");
    fs::create_dir_all(&dl_dir).ok();
    let safe: String = filename
        .chars()
        .map(|c| if c.is_alphanumeric() || ".-_".contains(c) { c } else { '_' })
        .collect();
    let pack_path = dl_dir.join(&safe);

    let c = crate::versions::http_client("deBang-Launcher/1.3");
    report(app, "mrpack", 0, 2);
    if PathBuf::from(&url).exists() {
        fs::copy(&url, &pack_path).map_err(|e| e.to_string())?;
        sys_log(app, format!("◆ установка сборки из файла {}", safe));
    } else {
        let _ = fs::remove_file(&pack_path); // never trust a stale copy
        sys_log(app, format!("⬇ скачивание сборки {}", safe));
        download_file(&c, &url, &pack_path, None, cancel.as_ref()).await?;
    }

    let bytes = fs::read(&pack_path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let vj: Value = {
        let mut s = String::new();
        let mut found = false;
        for cand in ["modrinth.index.json", "version.json", "manifest.json"] {
            if let Ok(mut f) = zip.by_name(cand) {
                std::io::Read::read_to_string(&mut f, &mut s).map_err(|e| e.to_string())?;
                found = true;
                break;
            }
        }
        if !found {
            return Err("в сборке нет modrinth.index.json / version.json — повреждённый .mrpack".into());
        }
        serde_json::from_str(&s).map_err(|e| e.to_string())?
    };
    let (mc, loader, files) = parse_pack(&vj)?;

    sys_log(
        app,
        format!("◆ сборка под MC {} ({}): {} файлов, установка…", mc, loader, files.len()),
    );
    let inst = instances::create_instance(name, mc, loader.clone())?;
    let inst_dir = PathBuf::from(&inst.dir);

    let total = files.len() as u64;
    report(app, "modpack", 0, total);
    let outcome: Result<u64, String> = async {
        let mut done = 0u64;
        for f in files.iter() {
            if crate::versions::cancelled(&cancel) {
                return Err("установка сборки отменена".into());
            }
            done += 1;
            report(app, "modpack", done, total);
            if !f.client {
                continue;
            }
            let Some(url0) = &f.url else {
                sys_log(app, format!("⚠ у {} нет ссылки — пропуск", f.path));
                continue;
            };
            if !url0.starts_with("https://") {
                sys_log(app, format!("⚠ пропущен {} (не https)", f.path));
                continue;
            }
            let dest = instances::safe_join(&inst_dir, &f.path)
                .map_err(|e| format!("небезопасный путь в индексе: {}", e))?;
            if let Some(par) = dest.parent() {
                fs::create_dir_all(par).map_err(|e| e.to_string())?;
            }
            download_file(&c, url0, &dest, f.size, cancel.as_ref())
                .await
                .map_err(|e| format!("не удалось скачать {}: {}", f.path, e))?;
        }

        let names: Vec<String> = zip.file_names().map(|s| s.to_string()).collect();
        let mut extracted = 0u64;
        for n in names {
            if crate::versions::cancelled(&cancel) {
                return Err("установка сборки отменена".into());
            }
            let Some(rest) = ["overrides/", "client-overrides/", "resources/"]
                .iter()
                .find_map(|p| n.strip_prefix(p))
            else {
                continue;
            };
            if rest.is_empty() {
                continue;
            }
            let out = match instances::safe_join(&inst_dir, rest) {
                Ok(o) => o,
                Err(_) => continue,
            };
            if n.ends_with('/') {
                fs::create_dir_all(&out).ok();
                continue;
            }
            let mut e = zip.by_name(&n).map_err(|e| e.to_string())?;
            if let Some(par) = out.parent() {
                fs::create_dir_all(par).map_err(|e| e.to_string())?;
            }
            let mut f = fs::File::create(&out).map_err(|e| e.to_string())?;
            std::io::copy(&mut e, &mut f).map_err(|e| e.to_string())?;
            extracted += 1;
        }
        Ok(extracted)
    }
    .await;
    let extracted = match outcome {
        Ok(n) => n,
        Err(e) => {
            let _ = fs::remove_dir_all(&inst_dir);
            report(app, "done", 1, 1);
            sys_log(app, format!("✖ установка сборки прервана: {}", e));
            return Err(e);
        }
    };
    report(app, "done", 1, 1);
    sys_log(
        app,
        format!(
            "✔ сборка установлена: версия '{}', {} файлов + {} override-файлов. {}",
            inst.config.name,
            total,
            extracted,
            if loader == "Forge" {
                "Авто-бутстрап поддерживает Vanilla/Fabric/NeoForge/Forge 1.13+ — для остальных импортируйте run-скрипт."
            } else {
                "Версия готова к запуску по Play."
            }
        ),
    );
    Ok(inst.config.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curseforge_flavoured_index() {
        let vj: Value = serde_json::json!({
            "formatVersion": 1, "game": "minecraft", "versionId": "15.0.0",
            "dependencies": { "fabric-loader": "0.19.5", "minecraft": "1.20.1" },
            "files": [{
                "path": "mods/BetterGrassify.jar",
                "env": { "client": "required", "server": "required" },
                "downloads": ["https://cdn.modrinth.com/data/x/versions/y/z.jar"],
                "fileSize": 89214
            }]
        });
        let (mc, loader, files) = parse_pack(&vj).unwrap();
        assert_eq!(mc, "1.20.1");
        assert_eq!(loader, "Fabric");
        assert!(files[0].url.as_deref().unwrap().starts_with("https://"));
        assert_eq!(files[0].size, Some(89214));
    }

    #[test]
    fn modrinth_v2_index() {
        let vj: Value = serde_json::json!({
            "pack_version": 2,
            "minecraft": { "version": "1.20.1", "mod_loader": { "project": "fabric-loader", "version": "0.15.11" } },
            "files": [{
                "path": "mods/fabric-api.jar",
                "env": { "client": "required", "server": "required" },
                "downloads": [{ "url": "https://cdn.modrinth.com/a/b.jar", "file_size": 123 }],
                "file_size": 123
            }]
        });
        let (mc, loader, files) = parse_pack(&vj).unwrap();
        assert_eq!(mc, "1.20.1");
        assert_eq!(loader, "Fabric");
        assert_eq!(files[0].url.as_deref(), Some("https://cdn.modrinth.com/a/b.jar"));
    }

    #[test]
    fn legacy_v1_index_with_client_and_neoforge() {
        let vj: Value = serde_json::json!({
            "pack_version": 1,
            "base_game_version": "1.21.4",
            "loaders": [{ "id": "neoforge", "version": "21.4.60" }],
            "files": [{ "path": "mods/x.jar", "env": { "client": "unsupported", "server": "required" }, "downloads": ["https://cdn.modrinth.com/x.jar"] }]
        });
        let (mc, loader, files) = parse_pack(&vj).unwrap();
        assert_eq!(mc, "1.21.4");
        assert_eq!(loader, "NeoForge");
        assert!(!files[0].client);
    }
}
