//! CurseForge support.
//!
//! The CurseForge API requires a personal API key (https://www.curseforge.com/minecraft),
//! so the user provides it once in Settings; it is stored with 0600 permissions
//! inside the launcher data directory. An empty key falls back to the
//! `CURSEFORGE_API_KEY` environment variable, which is handy for packaging.

use crate::versions::{data_root, download_file};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;

pub const API: &str = "https://api.curseforge.com/v1";
/// Minecraft
pub const GAME_ID: u32 = 432;
/// Mods / Mod Packs / Resource Packs / Shaders
pub const CLASS_MOD: u32 = 6;
pub const CLASS_MODPACK: u32 = 4471;
pub const CLASS_RESOURCEPACK: u32 = 12;
pub const CLASS_SHADER: u32 = 6556;

fn key_path() -> PathBuf {
    data_root().join("curseforge.key")
}

pub fn stored_key() -> String {
    fs::read_to_string(key_path())
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

pub fn effective_key() -> String {
    let s = stored_key();
    if !s.is_empty() {
        return s;
    }
    std::env::var("CURSEFORGE_API_KEY").unwrap_or_default()
}

pub fn save_key(key: &str) -> Result<(), String> {
    let k = key.trim();
    if k.is_empty() {
        let _ = fs::remove_file(key_path());
        return Ok(());
    }
    if !k
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err("ключ CurseForge содержит недопустимые символы".into());
    }
    fs::write(key_path(), k).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(key_path(), fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// `xx…xx` for display purposes.
pub fn mask_key(key: &str) -> String {
    let n = key.chars().count();
    if n < 8 {
        return "*".repeat(n);
    }
    let head: String = key.chars().take(4).collect();
    let tail: String = key.chars().skip(n - 4).collect();
    format!("{}…{}", head, tail)
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyStatus {
    pub configured: bool,
    pub masked: String,
    pub source: String,
}

#[tauri::command]
pub fn curseforge_key_status() -> KeyStatus {
    let stored = stored_key();
    let effective = effective_key();
    KeyStatus {
        configured: !effective.is_empty(),
        masked: mask_key(&effective),
        source: if stored.is_empty() { "env".into() } else { "settings".into() },
    }
}

#[tauri::command]
pub fn curseforge_key_save(key: String) -> Result<KeyStatus, String> {
    save_key(&key)?;
    Ok(curseforge_key_status())
}

fn client(key: &str) -> Result<reqwest::Client, String> {
    if key.trim().is_empty() {
        return Err(
            "нужен ключ CurseForge API: вставь его в Настройки → CurseForge (ссылка: curseforge.com/minecraft)"
                .into(),
        );
    }
    reqwest::Client::builder()
        .user_agent("deBang-Launcher/1.3")
        .default_headers({
            let mut m = reqwest::header::HeaderMap::new();
            m.insert(
                "x-api-key",
                reqwest::header::HeaderValue::from_str(key.trim())
                    .map_err(|e| format!("неверный ключ: {}", e))?,
            );
            m.insert(
                "Accept",
                reqwest::header::HeaderValue::from_static("application/json"),
            );
            m
        })
        .build()
        .map_err(|e| e.to_string())
}

async fn get_json(key: &str, path: &str) -> Result<Value, String> {
    let c = client(key)?;
    c.get(format!("{}{}", API, path))
        .send()
        .await
        .map_err(|e| format!("CurseForge API: {}", e))?
        .error_for_status()
        .map_err(|e| format!("CurseForge API HTTP {}", e))?
        .json()
        .await
        .map_err(|e| e.to_string())
}

/// Substring match on `index` (0 = relevance, 1 = popularity, 2 = last updated).
#[tauri::command]
pub async fn curseforge_search(
    key: String,
    search_filter: String,
    class_id: u32,
    game_version: Option<String>,
    page: Option<u32>,
) -> Result<Value, String> {
    let key = if key.trim().is_empty() {
        effective_key()
    } else {
        key
    };
    let page = page.unwrap_or(0);
    let mut q = format!(
        "/mods/search?gameId={}&classId={}&pageSize=30&page={}&index=0",
        GAME_ID, class_id, page
    );
    if !search_filter.trim().is_empty() {
        q.push_str(&format!(
            "&searchFilter={}",
            urlencode(search_filter.trim())
        ));
    }
    if let Some(v) = game_version.filter(|v| !v.trim().is_empty()) {
        q.push_str(&format!("&gameVersion={}", urlencode(v.trim())));
    }
    get_json(&key, &q).await
}

/// All published files of a project, newest first.
#[tauri::command]
pub async fn curseforge_files(
    key: String,
    project_id: u32,
) -> Result<Value, String> {
    let key = if key.trim().is_empty() {
        effective_key()
    } else {
        key
    };
    get_json(&key, &format!("/mods/{}/files", project_id)).await
}

#[tauri::command]
pub async fn curseforge_download_url(key: String, file_id: u32) -> Result<String, String> {
    let key = if key.trim().is_empty() {
        effective_key()
    } else {
        key
    };
    let c = client(&key)?;
    let v: Value = c
        .get(format!("{}/mods/{}/download", API, file_id))
        .send()
        .await
        .map_err(|e| format!("CurseForge API: {}", e))?
        .error_for_status()
        .map_err(|e| format!("CurseForge API HTTP {}", e))?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    v["data"]
        .as_str()
        .map(String::from)
        .ok_or_else(|| "CurseForge не вернул ссылку на файл".into())
}

/// Downloads a CurseForge file into an instance subfolder (mods,
/// resourcepacks, shaderpacks) — same UX as the Modrinth catalog.
#[tauri::command]
pub async fn curseforge_download_file(
    state: tauri::State<'_, crate::LauncherState>,
    key: String,
    instance_id: String,
    file_id: u32,
    filename: String,
    sub: Option<String>,
) -> Result<String, String> {
    let _ = state.cancel.send(false);
    let cancel = state.cancel.subscribe();
    let dir = crate::instances::instance_dir(&instance_id)?;
    let sub = sub.unwrap_or_else(|| "mods".to_string());
    if !matches!(sub.as_str(), "mods" | "resourcepacks" | "shaderpacks") {
        return Err("недопустимая целевая папка".into());
    }
    let safe: String = filename
        .chars()
        .map(|c| if c.is_alphanumeric() || ".-_".contains(c) { c } else { '_' })
        .collect();
    if safe.is_empty() {
        return Err("пустое имя файла".into());
    }
    let target_dir = dir.join(&sub);
    fs::create_dir_all(&target_dir).map_err(|e| e.to_string())?;
    let target = target_dir.join(&safe);
    let key = effective_or(key);
    let url = curseforge_download_url(key.clone(), file_id).await?;
    let c = client(&key)?;
    download_file(&c, &url, &target, None, Some(&cancel)).await?;
    Ok(target.to_string_lossy().to_string())
}

// ---------- modpack manifests ----------

/// The bits we need from a CurseForge `manifest.json`.
#[derive(Debug, PartialEq)]
pub struct ParsedManifest {
    pub mc_version: String,
    pub loader: String,
    /// Exact Forge build, e.g. `11.15.1.2318` — pinned so the launcher never
    /// swaps the pack onto a different Forge.
    pub forge_version: Option<String>,
    pub file_count: usize,
}

/// Parses a CurseForge manifest into what the launcher needs: Minecraft
/// version, loader and the list of (projectID, fileID) downloads.
pub fn parse_manifest(v: &Value) -> Result<ParsedManifest, String> {
    let mc_version = v["minecraft"]["version"]
        .as_str()
        .ok_or("в manifest.json нет minecraft.version")?
        .to_string();
    let mod_loaders = v["minecraft"]["modLoaders"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let loader_id = mod_loaders
        .iter()
        .find(|m| m["primary"].as_bool().unwrap_or(false))
        .or_else(|| mod_loaders.first())
        .and_then(|m| m["id"].as_str())
        .map(String::from)
        .unwrap_or_default();
    let loader = match loader_id.split('-').next().unwrap_or("") {
        "forge" => "Forge",
        "neoforge" => "NeoForge",
        "fabric" => "Fabric",
        "quilt" => "Forge", // Quilt packs run through Forge launchers
        "" => "Vanilla",
        other => other,
    }
    .to_string();
    // `forge-11.15.1.2318` / `neoforge-20.4.190` / `fabric-0.15.11`
    let forge_version = if loader == "Forge" {
        loader_id
            .split_once('-')
            .map(|(_, build)| build.to_string())
            .filter(|b| !b.is_empty() && b.chars().next().is_some_and(|c| c.is_ascii_digit()))
    } else {
        None
    };
    let file_count = v["files"]
        .as_array()
        .map(|a| a.len())
        .unwrap_or(0);
    if file_count == 0 {
        return Err("в manifest.json нет файлов".into());
    }
    Ok(ParsedManifest {
        mc_version,
        loader,
        forge_version,
        file_count,
    })
}

fn file_entries(v: &Value) -> Vec<(u64, u64)> {
    v["files"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|f| {
                    let p = f["projectID"].as_u64()?;
                    let id = f["fileID"].as_u64()?;
                    Some((p, id))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Installs a CurseForge modpack: downloads `manifest.json`, then every
/// required file, and extracts the `overrides/` folder into a new instance.
#[tauri::command]
pub async fn curseforge_install_modpack(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::LauncherState>,
    key: String,
    file_id: u32,
    name: String,
) -> Result<String, String> {
    let _ = state.cancel.send(false);
    let cancel = state.cancel.subscribe();
    install_modpack(Some(&app), &effective_or(key), file_id, name, Some(cancel)).await
}

fn effective_or(key: String) -> String {
    if key.trim().is_empty() {
        effective_key()
    } else {
        key
    }
}

pub async fn install_modpack(
    app: Option<&tauri::AppHandle>,
    key: &str,
    file_id: u32,
    name: String,
    cancel: Option<tokio::sync::watch::Receiver<bool>>,
) -> Result<String, String> {
    let key = effective_or(key.to_string());
    let root = data_root().join("curseforge");
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let manifest_url = curseforge_download_url(key.clone(), file_id).await?;
    let c = client(&key)?;
    let manifest_path = root.join(format!("{}.json", file_id));
    download_file(&c, &manifest_url, &manifest_path, None, cancel.as_ref()).await?;
    let raw = fs::read_to_string(&manifest_path).map_err(|e| e.to_string())?;
    let v: Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    let info = parse_manifest(&v)?;

    let clean = name.split(['-', '_', '.']).next().unwrap_or(&name).to_string();
    let inst = crate::instances::create_instance_pinned(
        if clean.is_empty() { name.clone() } else { clean },
        info.mc_version.clone(),
        info.loader.clone(),
        info.forge_version.clone(),
    )?;
    let inst_dir = PathBuf::from(&inst.dir);
    let total = file_entries(&v).len() as u64;
    crate::launch::emit_prep(app, "modpack", 0, total);
    let outcome: Result<u64, String> = async {
        let mut done = 0u64;
        for (project, fid) in file_entries(&v) {
            if crate::versions::cancelled(&cancel) {
                return Err("установка сборки отменена".into());
            }
            let url = match curseforge_download_url(key.clone(), fid as u32).await {
                Ok(u) => u,
                Err(e) => {
                    crate::launch::log_line(
                        app,
                        format!("⚠ проект {} файл {}: {}", project, fid, e),
                    );
                    done += 1;
                    crate::launch::emit_prep(app, "modpack", done, total);
                    continue;
                }
            };
            // CurseForge has no path in the manifest: infer from the file name
            let fname = url
                .rsplit('/')
                .next()
                .unwrap_or("file.jar")
                .split('?')
                .next()
                .unwrap_or("file.jar")
                .to_string();
            let is_zip = fname.ends_with(".zip");
            let sub = if is_zip { "" } else { "mods" };
            let dest = if sub.is_empty() {
                inst_dir.join(&fname)
            } else {
                inst_dir.join(sub).join(&fname)
            };
            if let Some(par) = dest.parent() {
                fs::create_dir_all(par).map_err(|e| e.to_string())?;
            }
            download_file(&c, &url, &dest, None, cancel.as_ref())
                .await
                .map_err(|e| format!("файл проекта {}: {}", project, e))?;
            done += 1;
            crate::launch::emit_prep(app, "modpack", done, total);
        }

        // overrides/ folder (files list with isServer=false marks extras)
        let mut extracted = 0u64;
        if let Some(arr) = v["overrides"].as_array() {
            for o in arr {
                let Some(o_path) = o["path"].as_str() else { continue };
                let Some(of) = o["downloadUrl"].as_str() else { continue };
                if !of.starts_with("https://") {
                    continue;
                }
                let safe = crate::instances::safe_join(&inst_dir, o_path)
                    .map_err(|e| format!("небезопасный путь override: {}", e))?;
                if let Some(par) = safe.parent() {
                    fs::create_dir_all(par).map_err(|e| e.to_string())?;
                }
                download_file(&c, of, &safe, None, cancel.as_ref()).await?;
                extracted += 1;
            }
        }
        Ok(extracted)
    }
    .await;

    match outcome {
        Ok(extra) => {
            crate::launch::emit_prep(app, "done", 1, 1);
            crate::launch::log_line(
                app,
                format!(
                    "✔ сборка CurseForge установлена: версия «{}», MC {}, {} (файлов: {} + overrides: {})",
                    inst.config.name, info.mc_version, info.loader, total, extra
                ),
            );
            Ok(inst.config.id)
        }
        Err(e) => {
            let _ = fs::remove_dir_all(&inst_dir);
            crate::launch::emit_prep(app, "done", 1, 1);
            crate::launch::log_line(app, format!("✖ установка сборки прервана: {}", e));
            Err(e)
        }
    }
}

fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_forge_manifest() {
        let v: Value = serde_json::json!({
            "minecraft": {
                "version": "1.20.1",
                "modLoaders": [{ "id": "forge-47.2.0", "primary": true }]
            },
            "files": [
                { "projectID": 238222, "fileID": 4717888, "required": true },
                { "projectID": 306612, "fileID": 4717890, "required": true }
            ],
            "overrides": []
        });
        let p = parse_manifest(&v).unwrap();
        assert_eq!(p.mc_version, "1.20.1");
        assert_eq!(p.loader, "Forge");
        assert_eq!(p.file_count, 2);
        assert_eq!(file_entries(&v).len(), 2);
    }

    #[test]
    fn pins_legacy_forge_build() {
        let raw = r#"{
            "minecraft": {"version": "1.8.9", "modLoaders": [{"id": "forge-11.15.1.2318", "primary": true}]},
            "files": [{"projectID": 1, "fileID": 2}]
        }"#;
        let p = parse_manifest(&serde_json::from_str(raw).unwrap()).unwrap();
        assert_eq!(p.mc_version, "1.8.9");
        assert_eq!(p.loader, "Forge");
        assert_eq!(p.forge_version.as_deref(), Some("11.15.1.2318"));
    }

    #[test]
    fn parses_fabric_manifest_without_primary() {
        let v: Value = serde_json::json!({
            "minecraft": { "version": "1.21.1", "modLoaders": [{ "id": "fabric-0.16.0" }] },
            "files": [{ "projectID": 1, "fileID": 2 }]
        });
        let p = parse_manifest(&v).unwrap();
        assert_eq!(p.loader, "Fabric");
        assert_eq!(p.mc_version, "1.21.1");
    }

    #[test]
    fn rejects_broken_manifest() {
        let v = serde_json::json!({ "minecraft": {}, "files": [] });
        assert!(parse_manifest(&v).is_err());
        let v2 = serde_json::json!({ "minecraft": { "version": "1.20.1" } });
        assert!(parse_manifest(&v2).is_err());
    }

    #[test]
    fn urlencoding() {
        assert_eq!(urlencode("1.20.1"), "1.20.1");
        assert_eq!(urlencode("a b"), "a%20b");
        assert_eq!(urlencode("fabric"), "fabric");
    }

    #[test]
    fn masks_key() {
        assert_eq!(mask_key(""), "");
        assert_eq!(mask_key("short"), "*****");
        assert_eq!(mask_key("abcdefgh1234"), "abcd…1234");
    }
}
