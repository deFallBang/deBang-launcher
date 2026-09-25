//! Automatic classic-Forge installation: downloads the official installer for
//! the newest Forge build of the requested Minecraft version and runs it
//! headless, so the user never has to import `run.sh` / `run.bat` manually.

use crate::java;
use crate::versions::{cancelled, data_root, download_file};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::sync::watch;

const MAVEN: &str = "https://maven.minecraftforge.net/net/minecraftforge/forge";

/// Latest stable Forge version for `mc`, e.g. `1.20.1-47.4.0`.
pub fn latest_forge_version(xml: &str, mc: &str) -> Option<(Vec<u32>, String)> {
    let mut best: Option<(Vec<u32>, String)> = None;
    let prefix = format!("{}-", mc.trim());
    for seg in xml.split("<version>").skip(1) {
        let v = seg.split("</version>").next().unwrap_or("").trim().to_string();
        if !v.starts_with(&prefix) {
            continue;
        }
        let build = &v[prefix.len()..];
        if build.is_empty() {
            continue;
        }
        // stable only: skip -beta/-rc builds
        if build.contains('-') {
            continue;
        }
        let nums: Vec<u32> = build
            .split('.')
            .filter_map(|x| x.parse::<u32>().ok())
            .collect();
        if nums.is_empty() {
            continue;
        }
        if best.as_ref().map(|b| nums > b.0).unwrap_or(true) {
            best = Some((nums, v.clone()));
        }
    }
    best
}

pub async fn ensure_forge<F>(
    log: &F,
    cancel: &Option<watch::Receiver<bool>>,
    mc: &str,
    req_java: u32,
) -> Result<(Value, PathBuf), String>
where
    F: Fn(String),
{
    if cancelled(cancel) {
        return Err("подготовка отменена".into());
    }
    let root = data_root().join("forge").join(mc);
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let marker = root.join(".installed-profile");
    if marker.exists() {
        if let Ok(raw) = fs::read_to_string(&marker) {
            if let Ok(v) = serde_json::from_str::<Value>(&raw) {
                log(format!("◆ Forge для MC {} уже установлен (кэш)", mc));
                return Ok((v, root));
            }
        }
    }

    let client = reqwest::Client::builder()
        .user_agent("deBang-Launcher/0.1")
        .build()
        .map_err(|e| e.to_string())?;
    let xml = client
        .get(format!("{}/maven-metadata.xml", MAVEN))
        .send()
        .await
        .map_err(|e| format!("maven.minecraftforge.net: {}", e))?
        .error_for_status()
        .map_err(|e| format!("maven.minecraftforge.net HTTP {}", e))?
        .text()
        .await
        .map_err(|e| e.to_string())?;
    let ver = latest_forge_version(&xml, mc)
        .ok_or_else(|| format!("Forge для MC {} не найден на maven.minecraftforge.net", mc))?
        .1;
    log(format!(
        "◆ Forge {} для MC {} — скачиваю установщик, он сам поставит библиотеки…",
        ver, mc
    ));

    let jar = root.join(format!("forge-{}-installer.jar", ver));
    download_file(
        &client,
        &format!("{}/{}/forge-{}-installer.jar", MAVEN, ver, ver),
        &jar,
        None,
        cancel.as_ref(),
    )
    .await?;

    let java_path = java::pick_java_path(req_java).ok_or_else(|| {
        format!(
            "для Forge {} нужна Java {} ({}+) — установите JDK: sudo pacman -S --needed jdk21-openjdk",
            ver, req_java, req_java
        )
    })?;

    // The installer refuses to run without a vanilla launcher profiles file; it
    // only reads it for the auth context, so a benign stub is enough.
    let profiles = root.join("launcher_profiles.json");
    if !profiles.exists() {
        let stub = serde_json::json!({
            "selectedProfile": "LatestRelease",
            "clientToken": "00000000-0000-0000-0000-000000000000",
            "authenticationSelected": {
                "accessToken": "0",
                "clientName": "deBang-Launcher",
                "uuid": "00000000-0000-4000-8000-000000000000",
                "username": "deBangPlayer",
                "type": "mojang",
                "userProperties": {}
            }
        });
        fs::write(&profiles, serde_json::to_string_pretty(&stub).unwrap_or_default())
            .map_err(|e| e.to_string())?;
    }

    log("◆ запускаю установщик Forge (1–3 минуты)…".to_string());
    let mut child = tokio::process::Command::new(&java_path)
        .arg("-jar")
        .arg(&jar)
        .arg("--installClient")
        .arg(&root)
        .current_dir(&root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("запуск установщика Forge: {}", e))?;
    let so = child.stdout.take();
    let se = child.stderr.take();
    let so_task = tokio::spawn(async move {
        let mut b = Vec::new();
        if let Some(mut s) = so {
            let _ = tokio::io::AsyncReadExt::read_to_end(&mut s, &mut b).await;
        }
        b
    });
    let se_task = tokio::spawn(async move {
        let mut b = Vec::new();
        if let Some(mut s) = se {
            let _ = tokio::io::AsyncReadExt::read_to_end(&mut s, &mut b).await;
        }
        b
    });
    let status = loop {
        if cancelled(cancel) {
            let _ = child.start_kill();
            let _ = child.wait().await;
            return Err("установка Forge отменена".into());
        }
        match child.try_wait() {
            Ok(Some(st)) => break st,
            Ok(None) => tokio::time::sleep(std::time::Duration::from_millis(200)).await,
            Err(e) => return Err(format!("установщик Forge: {}", e)),
        }
    };
    let stdout = so_task.await.unwrap_or_default();
    let stderr = se_task.await.unwrap_or_default();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&stdout),
        String::from_utf8_lossy(&stderr)
    );
    let all: Vec<&str> = text.lines().filter(|x| !x.trim().is_empty()).collect();
    let start = all.len().saturating_sub(20);
    for line in &all[start..] {
        log(format!("ForgeInstaller| {}", line));
    }
    if !status.success() {
        return Err(format!(
            "установщик Forge завершился с кодом {:?} — посмотрите лог выше",
            status.code()
        ));
    }

    // ---- locate produced launcher profile (forge-*.json) ----
    let mut profile: Option<Value> = None;
    if let Ok(vs) = fs::read_dir(root.join("versions")) {
        let mut candidates: Vec<_> = vs.flatten().collect();
        candidates.sort_by_key(|e| e.file_name());
        for e in candidates {
            let name = e.file_name().to_string_lossy().to_string();
            if !name.to_lowercase().contains("forge") {
                continue;
            }
            let json = e.path().join(format!("{}.json", name));
            if let Ok(raw) = fs::read_to_string(&json) {
                if let Ok(v) = serde_json::from_str::<Value>(&raw) {
                    if v.get("mainClass").is_some() {
                        profile = Some(v);
                        break;
                    }
                }
            }
        }
    }
    let profile = profile.ok_or("установщик Forge не создал launch-профиль (см. лог выше)")?;
    fs::write(&marker, serde_json::to_string(&profile).unwrap()).ok();
    log(format!("✔ Forge {} установлен автоматически", ver));
    Ok((profile, root))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_newest_stable_build() {
        let xml = "<metadata><versioning><versions>\
            <version>1.20.1-47.1.0</version>\
            <version>1.20.1-47.2.0</version>\
            <version>1.20.1-47.2.0-beta</version>\
            <version>1.20.1-47.3.0</version>\
            <version>1.19.2-43.4.0</version>\
            </versions></versioning></metadata>";
        let (_, v) = latest_forge_version(xml, "1.20.1").expect("must find");
        assert_eq!(v, "1.20.1-47.3.0");
        assert!(latest_forge_version(xml, "1.16.5").is_none());
    }

    #[test]
    fn no_build_for_unknown_mc() {
        assert!(latest_forge_version("<versions></versions>", "1.7.10").is_none());
    }
}
