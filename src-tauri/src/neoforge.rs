use crate::java;
use crate::versions::{cancelled, data_root, download_file};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::sync::watch;

/// Downloads the official NeoForge installer (latest stable for this MC) and
/// runs it headless (`--installClient <cachedir>`), so all libraries, the
/// universal jar and the launch profile are produced by NeoForge itself.
/// Returns (launch profile json, root dir containing libraries/).
pub async fn ensure_neoforge<F>(
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
    let root = data_root().join("neoforge").join(mc);
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let marker = root.join(".installed-profile");
    if marker.exists() {
        if let Ok(raw) = fs::read_to_string(&marker) {
            if let Ok(v) = serde_json::from_str::<Value>(&raw) {
                return Ok((v, root));
            }
        }
    }

    // ---- latest stable NeoForge version for this MC ----
    let parts: Vec<&str> = mc.split('.').collect();
    let (major, minor) = match parts.len() {
        3.. => (
            parts[1].parse::<u32>().unwrap_or(0),
            parts[2].parse::<u32>().unwrap_or(0),
        ),
        2 => (parts[1].parse::<u32>().unwrap_or(0), 0),
        _ => return Err(format!("непонятная версия Minecraft: {}", mc)),
    };
    let prefixes = [format!("{}.{}.", major, minor)];
    let client = reqwest::Client::builder()
        .user_agent("deBang-Launcher/0.1")
        .build()
        .map_err(|e| e.to_string())?;
    let xml = client
        .get("https://maven.neoforged.net/releases/net/neoforged/neoforge/maven-metadata.xml")
        .send()
        .await
        .map_err(|e| format!("maven.neoforged.net: {}", e))?
        .error_for_status()
        .map_err(|e| format!("maven.neoforged.net HTTP {}", e))?
        .text()
        .await
        .map_err(|e| e.to_string())?;
    let mut best: Option<((u32, u32, u32), String)> = None;
    for seg in xml.split("<version>").skip(1) {
        let v = seg.split("</version>").next().unwrap_or("").trim().to_string();
        if v.matches('.').count() < 2 || v.contains('-') || !prefixes.iter().any(|p| v.starts_with(p.as_str())) {
            continue;
        }
        let nums: Vec<u32> = v.split('.').filter_map(|x| x.parse().ok()).collect();
        if nums.len() < 3 {
            continue;
        }
        let key = (nums[0], nums[1], nums[2]);
        if best.as_ref().map(|b| key > b.0).unwrap_or(true) {
            best = Some((key, v));
        }
    }
    let ver = best.ok_or_else(|| format!("NeoForge-линейка {} не найдена на maven.neoforged.net", prefixes[0].trim_end_matches('.')))?.1;
    log(format!(
        "◆ NeoForge {} для MC {} — скачиваю установщик (~20 МБ)…",
        ver, mc
    ));

    // ---- installer jar ----
    let jar = root.join(format!("neoforge-{}-installer.jar", ver));
    download_file(
        &client,
        &format!(
            "https://maven.neoforged.net/releases/net/neoforged/neoforge/{}/neoforge-{}-installer.jar",
            ver, ver
        ),
        &jar,
        None,
        cancel.as_ref(),
    )
    .await?;

    // ---- headless install ----
    let java_path = java::pick_java_path(req_java).ok_or_else(|| {
        format!(
            "для NeoForge ({}) нужна Java {} ({}+) — установите: sudo pacman -S --needed jre-openjdk",
            ver, req_java, req_java
        )
    })?;
    log("◆ запускаю установщик (это 1-3 минуты, он качает библиотеки)…".to_string());
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
        if let Err(e) = fs::write(&profiles, serde_json::to_string_pretty(&stub).unwrap_or_default()) {
            log(format!("⚠ не удалось создать launcher_profiles.json: {}", e));
        }
    }
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
        .map_err(|e| format!("запуск установщика: {}", e))?;
    // The installer takes minutes — keep it cancellable and never leave it
    // running in the background after the user pressed "отмена".
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
        if crate::versions::cancelled(cancel) {
            let _ = child.start_kill();
            let _ = child.wait().await;
            return Err("установка NeoForge отменена".into());
        }
        match child.try_wait() {
            Ok(Some(st)) => break st,
            Ok(None) => tokio::time::sleep(std::time::Duration::from_millis(200)).await,
            Err(e) => return Err(format!("установщик: {}", e)),
        }
    };
    let stdout = so_task.await.unwrap_or_default();
    let stderr = se_task.await.unwrap_or_default();
    let out = std::process::Output {
        status,
        stdout,
        stderr,
    };
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let all: Vec<&str> = text.lines().filter(|x| !x.trim().is_empty()).collect();
    let start = all.len().saturating_sub(25);
    for line in &all[start..] {
        log(format!("Installer| {}", line));
    }
    if !out.status.success() {
        return Err(format!(
            "NeoForge-установщик завершился с кодом {:?}",
            out.status.code()
        ));
    }

    // ---- locate produced launcher profile ----
    let mut profile: Option<Value> = None;
    if let Ok(vs) = fs::read_dir(root.join("versions")) {
        for e in vs.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if !name.to_lowercase().contains("neoforge") {
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
    let profile = profile.ok_or("установщик не создал launch-профиль (см. вывод выше)")?;
    fs::write(&marker, serde_json::to_string(&profile).unwrap()).ok();
    log(format!("✔ NeoForge {} готов к запуску", ver));
    Ok((profile, root))
}
