use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::sync::watch;

pub(crate) fn cancelled(rx: &Option<watch::Receiver<bool>>) -> bool {
    rx.as_ref().map(|r| *r.borrow()).unwrap_or(false)
}

async fn wait_cancel(mut rx: watch::Receiver<bool>) {
    while !*rx.borrow_and_update() {
        if rx.changed().await.is_err() {
            std::future::pending::<()>().await;
        }
    }
}

pub struct Prepared {
    pub java_major: u32,
    pub main_class: String,
    pub jvm: Vec<String>,
    pub game: Vec<String>,
    pub classpath: Option<String>,
}

/// Per-OS data directory: XDG on Linux, %LOCALAPPDATA% on Windows,
/// ~/Library/Application Support on macOS.
pub fn data_root() -> PathBuf {
    let root = if cfg!(windows) {
        std::env::var("LOCALAPPDATA")
            .or_else(|_| std::env::var("APPDATA"))
            .map(PathBuf::from)
            .unwrap_or_else(|_| crate::sysinfo::home_dir().join("AppData/Local"))
            .join("deBangLauncher")
    } else if cfg!(target_os = "macos") {
        crate::sysinfo::home_dir()
            .join("Library")
            .join("Application Support")
            .join("deBangLauncher")
    } else {
        std::env::var("XDG_DATA_HOME")
            .map(|v| PathBuf::from(v).join("debang-launcher"))
            .unwrap_or_else(|_| {
                crate::sysinfo::home_dir()
                    .join(".local")
                    .join("share")
                    .join("debang-launcher")
            })
    };
    fs::create_dir_all(&root).ok();
    root
}

/// Classpath separator is platform-specific (";" on Windows).
pub const CLASSPATH_SEP: &str = if cfg!(windows) { ";" } else { ":" };

/// Mojang OS name for the current platform.
pub fn mojang_os() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "osx"
    } else {
        "linux"
    }
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent("deBang-Launcher/0.1")
        .build()
        .map_err(|e| e.to_string())
}

/// Cached metadata older than this is refetched, so new Minecraft/Fabric
/// releases and changed manifests are not missed forever.
const META_TTL_SECS: u64 = 6 * 3600;

fn cache_is_fresh(path: &Path) -> bool {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .map(|e| e.as_secs() < META_TTL_SECS)
        .unwrap_or(false)
}

async fn fetch_json(client: &reqwest::Client, url: &str, cache: &Path) -> Result<Value, String> {
    if cache_is_fresh(cache) {
        if let Ok(raw) = fs::read_to_string(cache) {
            if let Ok(v) = serde_json::from_str::<Value>(&raw) {
                return Ok(v);
            }
        }
    }
    let v: Value = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("GET {}: {}", url, e))?
        .error_for_status()
        .map_err(|e| format!("GET {}: HTTP {}", url, e))?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    if let Some(p) = cache.parent() {
        fs::create_dir_all(p).ok();
    }
    fs::write(cache, serde_json::to_string(&v).unwrap()).ok();
    Ok(v)
}

pub(crate) async fn download_file(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
    expected: Option<u64>,
    cancel: Option<&watch::Receiver<bool>>,
) -> Result<(), String> {
    if cancel.map(|c| *c.borrow()).unwrap_or(false) {
        return Err("загрузка отменена".into());
    }
    if let Ok(meta) = fs::metadata(dest) {
        if meta.len() > 0 {
            match expected {
                Some(exp) if meta.len() == exp => return Ok(()),
                None => return Ok(()),
                _ => {}
            }
        }
    }
    // Append (not replace) the extension: foo.jar.part and foo.zip.part must
    // not share one temp file.
    let mut tmp_name = dest.as_os_str().to_os_string();
    tmp_name.push(".part");
    let tmp = PathBuf::from(tmp_name);
    let fut = download_inner(client, url, dest, &tmp, expected);
    match cancel {
        None => fut.await,
        Some(rx) => {
            tokio::select! {
                r = fut => r,
                _ = wait_cancel(rx.clone()) => {
                    let _ = tokio::fs::remove_file(&tmp).await;
                    Err("загрузка отменена".into())
                }
            }
        }
    }
}

async fn download_inner(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
    tmp: &Path,
    expected: Option<u64>,
) -> Result<(), String> {
    if let Some(p) = dest.parent() {
        fs::create_dir_all(p).map_err(|e| e.to_string())?;
    }
    let mut resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("{}: {}", url, e))?
        .error_for_status()
        .map_err(|e| format!("{}: HTTP {}", url, e))?;
    if let Some(exp) = expected {
        if let Some(len) = resp.content_length() {
            if len != exp {
                return Err(format!("{}: размер {} != ожидаемый {}", url, len, exp));
            }
        }
    }
    let mut f = tokio::fs::File::create(tmp).await.map_err(|e| e.to_string())?;
    let mut written: u64 = 0;
    while let Some(chunk) = resp.chunk().await.map_err(|e| e.to_string())? {
        written += chunk.len() as u64;
        if written > 512 * 1024 * 1024 {
            drop(f);
            let _ = tokio::fs::remove_file(tmp).await;
            return Err(format!("{}: файл подозрительно большой (>512 МБ)", url));
        }
        f.write_all(&chunk).await.map_err(|e| e.to_string())?;
    }
    f.flush().await.ok();
    drop(f);
    if let Some(exp) = expected {
        if written != exp {
            let _ = tokio::fs::remove_file(tmp).await;
            return Err(format!("{}: докачано {} из {} байт", url, written, exp));
        }
    }
    fs::rename(tmp, dest).map_err(|e| e.to_string())
}

// ---------- rules ----------

fn rule_matches(r: &Value) -> bool {
    if let Some(o) = r.get("os") {
        if let Some(name) = o.get("name").and_then(|v| v.as_str()) {
            if name != mojang_os() {
                return false;
            }
        }
        if let Some(arch) = o.get("arch").and_then(|v| v.as_str()) {
            let a = std::env::consts::ARCH;
            let matches_arch = match arch {
                "x86" => a == "x86",
                "x86_64" | "amd64" => a == "x86_64",
                "arm64" | "aarch64" => a == "aarch64",
                other => other == a,
            };
            if !matches_arch {
                return false;
            }
        }
    }
    if r.get("features")
        .and_then(|v| v.as_object())
        .map(|m| !m.is_empty())
        .unwrap_or(false)
    {
        return false;
    }
    true
}

fn rules_allow(rules: Option<&Value>) -> bool {
    match rules {
        None => true,
        Some(Value::Array(arr)) => {
            let mut allow = false;
            for r in arr {
                if !rule_matches(r) {
                    continue;
                }
                match r.get("action").and_then(|v| v.as_str()) {
                    Some("allow") => allow = true,
                    Some("disallow") => return false,
                    _ => {}
                }
            }
            allow
        }
        _ => true,
    }
}

fn push_value(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::String(s) => out.push(s.clone()),
        Value::Array(arr) => {
            for x in arr {
                if let Some(s) = x.as_str() {
                    out.push(s.to_string());
                }
            }
        }
        _ => {}
    }
}

fn collect_args(list: &Value, out: &mut Vec<String>) {
    if let Value::Array(arr) = list {
        for item in arr {
            match item {
                Value::String(s) => out.push(s.clone()),
                Value::Object(obj) => {
                    let mut got = false;
                    if let Some(rules) = obj.get("rules").and_then(|v| v.as_array()) {
                        for r in rules {
                            if !rule_matches(r)
                                || r.get("action").and_then(|v| v.as_str()) != Some("allow")
                            {
                                continue;
                            }
                            if let Some(v) = r.get("value") {
                                push_value(v, out);
                            } else if let Some(v) = obj.get("value") {
                                push_value(v, out);
                            }
                            got = true;
                        }
                    }
                    if !got && !obj.contains_key("rules") {
                        if let Some(v) = obj.get("value") {
                            push_value(v, out);
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

fn substitute(s: &str, map: &HashMap<String, String>) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find("${") {
        out.push_str(&rest[..i]);
        match rest[i..].find('}') {
            Some(j) => {
                let key = &rest[i + 2..i + j];
                out.push_str(map.get(key).map(String::as_str).unwrap_or(""));
                rest = &rest[i + j + 1..];
            }
            None => {
                rest = &rest[i..];
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

// ---------- maven paths ----------

fn coords(name: &str) -> Option<(String, String, String)> {
    let p: Vec<&str> = name.split(':').collect();
    match p.len() {
        3 | 4 if !p[2].is_empty() => Some((p[0].replace('.', "/"), p[1].to_string(), p[2].to_string())),
        5 => Some((p[0].replace('.', "/"), p[1].to_string(), p[3].to_string())),
        4 => Some((p[0].replace('.', "/"), p[1].to_string(), p[2].to_string())),
        _ => None,
    }
}

fn maven_dir(name: &str) -> String {
    match coords(name) {
        Some((g, a, v)) => format!("{}/{}/{}", g, a, v),
        None => "malformed".into(),
    }
}

fn lib_jar_path(root: &Path, name: &str, classifier: Option<&str>) -> PathBuf {
    match coords(name) {
        Some((_, a, v)) => {
            let suffix = classifier.map(|c| format!("-{}", c)).unwrap_or_default();
            root.join(format!("{}/{}-{}{}.jar", maven_dir(name), a, v, suffix))
        }
        None => root.join("malformed.jar"),
    }
}

fn extract_natives(jar: &Path, dest: &Path) -> Result<(), String> {
    use std::io::copy;
    fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    let data = fs::read(jar).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(data)).map_err(|e| e.to_string())?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().to_string();
        if name.contains("META-INF") || entry.is_dir() {
            continue;
        }
        let wanted = if cfg!(windows) { "dll" } else { "so" };
        if Path::new(&name)
            .extension()
            .map(|e| {
                let e = e.to_string_lossy();
                e == wanted || (cfg!(target_os = "macos") && e == "dylib")
            })
            .unwrap_or(false)
        {
            let base = Path::new(&name)
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or(name.clone());
            let mut out = fs::File::create(dest.join(base)).map_err(|e| e.to_string())?;
            copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

async fn ensure_lib(
    client: &reqwest::Client,
    name: &str,
    lib: &Value,
    local_libs: Option<&Path>,
    lib_dir: &Path,
    cancel: Option<&watch::Receiver<bool>>,
) -> Result<PathBuf, String> {
    if let Some(a) = lib
        .get("downloads")
        .and_then(|d| d.get("artifact"))
        .filter(|a| !a.is_null())
    {
        let url = a["url"].as_str().ok_or("artifact без url")?.to_string();
        let size = a["size"].as_u64();
        let rel = match a["path"].as_str() {
            Some(p) if !p.is_empty() => crate::instances::safe_rel(p)
                .map_err(|_| format!("небезопасный путь библиотеки: {}", p))?
                .to_string_lossy()
                .to_string(),
            _ => lib_jar_path(lib_dir, name, None)
                .strip_prefix(lib_dir)
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default(),
        };
        if rel.is_empty() {
            return Err(format!("library без пути: {}", name));
        }
        if let Some(loc) = local_libs {
            let cand = loc.join(&rel);
            if cand.exists() {
                return Ok(cand);
            }
        }
        let target = lib_dir.join(&rel);
        download_file(client, &url, &target, size, cancel).await?;
        return Ok(target);
    }
    let (g, a, v) = coords(name).ok_or(format!("malformed lib: {}", name))?;
    let rel = format!("{}/{}/{}/{}-{}.jar", g, a, v, a, v); // maven layout
    if let Some(loc) = local_libs {
        let cand = loc.join(&rel);
        if cand.exists() {
            return Ok(cand);
        }
    }
    let target = lib_jar_path(lib_dir, name, None);
    if target.exists() {
        return Ok(target);
    }
    let mut bases: Vec<String> = Vec::new();
    if let Some(b) = lib.get("url").and_then(|v| v.as_str()) {
        bases.push(b.trim_end_matches('/').to_string());
    }
    bases.push("https://maven.neoforged.net/releases".into());
    bases.push("https://libraries.minecraft.net".into());
    bases.push("https://repo1.maven.org/maven2".into());
    let mut last = String::from("нет mirrors");
    for b in bases {
        let url = format!("{}/{}", b, rel);
        match download_file(client, &url, &target, None, cancel).await {
            Ok(()) => return Ok(target),
            Err(e) => last = e,
        }
    }
    Err(format!("библиотека {} недоступна ({})", name, last))
}

// ---------- prepare ----------

#[allow(clippy::too_many_arguments)]
pub async fn prepare<F, L>(
    report: F,
    log: L,
    mc: &str,
    loader: &str,
    inst_dir: &Path,
    uuid: &str,
    player_name: &str,
    cancel: Option<watch::Receiver<bool>>,
) -> Result<Prepared, String>
where
    F: Fn(&str, u64, u64) + Send + Sync + 'static,
    L: Fn(String) + Send + Sync + 'static,
{
    if loader != "Vanilla" && loader != "Fabric" && loader != "NeoForge" {
        return Err(format!(
            "Автоматически ставим Vanilla, Fabric и NeoForge ({}: импортируйте run.sh в инстанс).",
            loader
        ));
    }
    let report = Arc::new(report);
    let client = client()?;
    let root = data_root();
    let lib_dir = root.join("libraries");
    fs::create_dir_all(&lib_dir).ok();

    report("manifest", 0, 1);
    let manifest = fetch_json(
        &client,
        "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json",
        &root.join("version_manifest.json"),
    )
    .await?;
    let vurl = manifest["versions"]
        .as_array()
        .and_then(|a| a.iter().find(|x| x["id"].as_str() == Some(mc)))
        .and_then(|x| x["url"].as_str())
        .ok_or_else(|| format!("Версия {} не найдена в манифесте Mojang", mc))?
        .to_string();

    report("version.json", 0, 1);
    let mut vj = fetch_json(&client, &vurl, &root.join(format!("versions/{}.json", mc))).await?;
    let is_legacy = vj.get("arguments").is_none();
    if !matches!(loader, "Vanilla" | "Fabric" | "NeoForge") {
        return Err(format!(
            "загрузчик «{}»: автоматическая подготовка поддерживает Vanilla, Fabric и NeoForge (для {} импортируйте run.sh)",
            loader, loader
        ));
    }

    let mut local_libs: Option<PathBuf> = None;
    let mut neoforge_universal: Option<PathBuf> = None;
    if loader == "NeoForge" {
        let req = vj["javaVersion"]["majorVersion"].as_u64().unwrap_or(17) as u32;
        let (profile, nroot) =
            crate::neoforge::ensure_neoforge(&log, &cancel, mc, req).await?;
        let nver = profile["id"]
            .as_str()
            .and_then(|i| i.strip_prefix("neoforge-"))
            .unwrap_or("")
            .to_string();
        merge_profile(&mut vj, &profile);
        let nlibs = nroot.join("libraries");
        local_libs = Some(nlibs.clone());
        neoforge_universal = Some(nlibs.join(format!(
            "net/neoforged/neoforge/{nver}/neoforge-{nver}-universal.jar"
        )));
    }

    if loader == "Fabric" {
        report("fabric meta", 0, 1);
        let loaders = fetch_json(
            &client,
            &format!("https://meta.fabricmc.net/v2/versions/loader/{}", mc),
            &root.join(format!("versions/fabric-list-{}.json", mc)),
        )
        .await?;
        let first = loaders
            .as_array()
            .and_then(|a| a.first())
            .ok_or("Fabric-загрузчик для этой версии не найден")?;
        let profile_url = match first.pointer("/launcherMeta/url").and_then(|v| v.as_str()) {
            Some(u) => u.to_string(),
            None => {
                let lv = first
                    .pointer("/loader/version")
                    .and_then(|v| v.as_str())
                    .ok_or("нет версии fabric-loader")?;
                format!("https://meta.fabricmc.net/v2/versions/loader/{}/{}/profile/json", mc, lv)
            }
        };
        let profile = fetch_json(
            &client,
            &profile_url,
            &root.join(format!("versions/fabric-{}.json", mc)),
        )
        .await?;
        merge_profile(&mut vj, &profile);
    }

    // client jar
    let d = &vj["downloads"]["client"];
    let jar_url = d["url"].as_str().ok_or("нет downloads.client.url")?.to_string();
    let jar_size = d["size"].as_u64();
    let client_jar = root.join(format!("versions/{}/{}.jar", mc, mc));
    report("client.jar", 0, jar_size.unwrap_or(1));
    download_file(&client, &jar_url, &client_jar, jar_size, cancel.as_ref()).await?;
    report("client.jar", jar_size.unwrap_or(1), jar_size.unwrap_or(1));

    // libraries + natives
    let mut classpath: Vec<PathBuf> = Vec::new();
    let natives_dir = inst_dir.join("natives");
    fs::create_dir_all(&natives_dir).ok();
    let libs = vj["libraries"].as_array().cloned().unwrap_or_default();
    let nlibs = libs.len() as u64;
    for (i, lib) in libs.iter().enumerate() {
        report("libraries", i as u64, nlibs);
        if cancelled(&cancel) {
            return Err("подготовка отменена".into());
        }
        if !rules_allow(lib.get("rules")) {
            continue;
        }
        let name = lib["name"].as_str().ok_or("library без name")?;
        let dl = lib.get("downloads").filter(|v| !v.is_null());
        let target = ensure_lib(&client, name, lib, local_libs.as_deref(), &lib_dir, cancel.as_ref()).await?;
        classpath.push(target);

        // natives for the current OS
        let natives_key = match mojang_os() {
            "windows" => "windows",
            "osx" => "osx",
            _ => "linux",
        };
        let natives_classifier = format!("natives-{}", natives_key);
        let mut native: Option<(String, PathBuf)> = None;
        if is_legacy {
            if let Some(sfx) = lib
                .get("natives")
                .and_then(|n| n.get(natives_key))
                .and_then(|v| v.as_str())
            {
                let classifier = sfx.trim_start_matches('-').to_string();
                let base = lib
                    .get("url")
                    .and_then(|v| v.as_str())
                    .unwrap_or("https://libraries.minecraft.net/")
                    .trim_end_matches('/')
                    .to_string();
                let url = match coords(name) {
                    Some((_, a, v)) => format!(
                        "{}/{}/{}-{}-{}.jar",
                        base,
                        maven_dir(name),
                        a,
                        v,
                        classifier
                    ),
                    None => return Err(format!("malformed native: {}", name)),
                };
                native = Some((url, lib_jar_path(&lib_dir, name, Some(&classifier))));
            }
        } else if let Some((u, path)) = dl
            .and_then(|d| d.get("classifiers"))
            .and_then(|c| c.get(&natives_classifier))
            .filter(|v| !v.is_null())
            .and_then(|v| Some((v["url"].as_str()?, v["path"].as_str().unwrap_or("").to_string())))
        {
            let target = if path.is_empty() {
                lib_jar_path(&lib_dir, name, Some(&natives_classifier))
            } else {
                lib_dir.join(
                    crate::instances::safe_rel(&path)
                        .map_err(|_| format!("небезопасный путь natives: {}", path))?,
                )
            };
            native = Some((u.to_string(), target));
        }
        if let Some((nurl, npath)) = native {
            download_file(&client, &nurl, &npath, None, cancel.as_ref()).await?;
            extract_natives(&npath, &natives_dir)?;
            classpath.push(npath);
        }
    }
    // FML4 (NeoForge 21.x): the vanilla jar must NOT be on the classpath — the
    // production client provider assembles module "minecraft" from the SRG +
    // merged-client + extra jars found under libraryDirectory. Only the
    // universal jar joins the classpath (as the "neoforge" mod).
    match &neoforge_universal {
        Some(u) if !u.exists() => {
            return Err(format!("не найден universal-джар NeoForge: {}", u.display()));
        }
        Some(_) => {}
        None => classpath.push(client_jar),
    }

    // assets
    let assets_root = root.join("assets");
    fs::create_dir_all(&assets_root).ok();
    let idx = &vj["assetIndex"];
    let (idx_url, alias) = if idx.is_null() {
        (
            "https://launchermeta.mojang.com/mc/assets/legacy.json".to_string(),
            "legacy".to_string(),
        )
    } else {
        (
            idx["url"].as_str().ok_or("assetIndex без url")?.to_string(),
            idx["id"].as_str().unwrap_or("legacy").to_string(),
        )
    };
    let idx_json = fetch_json(
        &client,
        &idx_url,
        &assets_root.join(format!("indexes/{}.json", alias)),
    )
    .await?;
    let objects: Vec<(String, String)> = idx_json["objects"]
        .as_object()
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| Some((k.clone(), v["hash"].as_str()?.to_string())))
                .collect()
        })
        .unwrap_or_default();
    let total = objects.len() as u64;
    let done = Arc::new(std::sync::atomic::AtomicU64::new(0));
    report("assets", 0, total);
    let mut set = tokio::task::JoinSet::new();
    for (_, hash) in objects {
        if cancelled(&cancel) {
            set.abort_all();
            while set.join_next().await.is_some() {}
            return Err("подготовка отменена".into());
        }
        if hash.len() < 2 {
            continue;
        }
        let dest = assets_root.join(format!("objects/{}/{}", &hash[..2], hash));
        if dest.exists() {
            done.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            continue;
        }
        let c = client.clone();
        let rep = report.clone();
        let dn = done.clone();
        let url = format!("https://resources.download.minecraft.net/{}/{}", &hash[..2], hash);
        let crx = cancel.clone();
        set.spawn(async move {
            let r = download_file(&c, &url, &dest, None, crx.as_ref()).await;
            let n = dn.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            if n.is_multiple_of(250) {
                rep("assets", n, total);
            }
            r
        });
        while set.len() >= 24 {
            if let Some(Ok(Err(e))) = set.join_next().await {
                set.abort_all();
                while set.join_next().await.is_some() {}
                if cancelled(&cancel) {
                    return Err("подготовка отменена".into());
                }
                return Err(format!("ассет не скачан: {}", e));
            }
        }
    }
    while let Some(joined) = set.join_next().await {
        if let Ok(Err(e)) = joined {
            if !cancelled(&cancel) {
                return Err(format!("ассет не скачан: {}", e));
            }
            return Err("подготовка отменена".into());
        }
    }
    if cancelled(&cancel) {
        return Err("подготовка отменена".into());
    }
    report("assets", done.load(std::sync::atomic::Ordering::Relaxed), total);

    // Command line — dedup: merged loader profiles re-list vanilla libs and
    // NeoForge's UnionFileSystem dies on duplicate classpath entries.
    let mut uniq: Vec<PathBuf> = Vec::with_capacity(classpath.len());
    for p in classpath.iter() {
        if !uniq.contains(p) {
            uniq.push(p.clone());
        }
    }
    let cp = uniq
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join(CLASSPATH_SEP);
    let mut map: HashMap<String, String> = HashMap::new();
    let set = |k: &str, v: &str, m: &mut HashMap<String, String>| {
        m.insert(k.into(), v.into());
    };
    set("auth_player_name", player_name, &mut map);
    set("version_name", mc, &mut map);
    set("game_directory", &inst_dir.to_string_lossy(), &mut map);
    set("assets_root", &assets_root.to_string_lossy(), &mut map);
    set("assets_index_name", &alias, &mut map);
    set("game_assets", &assets_root.join("virtual/legacy").to_string_lossy(), &mut map);
    set("auth_uuid", uuid, &mut map);
    set("auth_access_token", "0", &mut map);
    set("auth_session", &format!("legacy:{}:0", player_name), &mut map);
    set("client_token", "0", &mut map);
    set("user_type", "legacy", &mut map);
    set("version_type", vj["type"].as_str().unwrap_or("release"), &mut map);
    set("natives_directory", &natives_dir.to_string_lossy(), &mut map);
    set("launcher_name", "deBang-Launcher", &mut map);
    set("launcher_version", "0.1.0", &mut map);
    set("classpath", &cp, &mut map);
    set("user_properties", "{}", &mut map);
    set("resolution_width", "1280", &mut map);
    set("resolution_height", "800", &mut map);
    set("classpath_separator", CLASSPATH_SEP, &mut map);
    let ld = local_libs.clone().unwrap_or_else(|| lib_dir.clone());
    set("library_directory", &ld.to_string_lossy(), &mut map);
    set("quickPlayPath", "", &mut map);
    set("xuid", "", &mut map);
    if let Some(data) = vj["data"].as_object() {
        for (k, v) in data {
            if let Some(cv) = v.get("client").and_then(|x| x.as_str()) {
                map.insert(k.clone(), cv.to_string());
            }
        }
    }

    let jvm;
    let game;
    let needs_cp_flag;
    if let Some(args) = vj.get("arguments").filter(|v| !v.is_null()) {
        let mut raw_jvm = Vec::new();
        collect_args(&args["jvm"], &mut raw_jvm);
        needs_cp_flag = !raw_jvm.iter().any(|a| a.contains("${classpath}"));
        let mut jv: Vec<String> = raw_jvm.iter().map(|a| substitute(a, &map)).collect();
        if neoforge_universal.is_some() {
            // Essential's first-run "update available" dialog blocks the boot
            // progress indefinitely — skip it silently.
            jv.push("-Dessential.loader.stage_2_skip_update=true".to_string());
        }
        jvm = jv;
        let mut raw_game = Vec::new();
        collect_args(&args["game"], &mut raw_game);
        game = raw_game.iter().map(|a| substitute(a, &map)).collect();
    } else if let Some(legacy) = vj["minecraftArguments"].as_str() {
        needs_cp_flag = true;
        jvm = Vec::new();
        game = legacy
            .split_whitespace()
            .map(|a| substitute(a, &map))
            .collect();
    } else {
        return Err("в версии нет ни arguments, ни minecraftArguments".into());
    }

    Ok(Prepared {
        java_major: vj["javaVersion"]["majorVersion"]
            .as_u64()
            .unwrap_or(if is_legacy { 8 } else { 17 }) as u32,
        main_class: vj["mainClass"].as_str().ok_or("нет mainClass")?.to_string(),
        jvm,
        game,
        classpath: needs_cp_flag.then_some(cp),
    })
}

fn merge_profile(vj: &mut Value, profile: &Value) {
    if let Some(arr) = vj.get_mut("libraries").and_then(|v| v.as_array_mut()) {
        if let Some(pl) = profile.get("libraries").and_then(|v| v.as_array()) {
            arr.extend(pl.iter().cloned());
        }
    }
    if let Some(mc) = profile.get("mainClass").and_then(|v| v.as_str()) {
        vj["mainClass"] = Value::String(mc.to_string());
    }
    if let Some(pa) = profile.get("arguments").and_then(|v| v.as_object()) {
        if let Some(va) = vj.get_mut("arguments").and_then(|v| v.as_object_mut()) {
            for key in ["game", "jvm"] {
                if let (Some(add), Some(dst)) = (
                    pa.get(key).and_then(|v| v.as_array()),
                    va.get_mut(key).and_then(|v| v.as_array_mut()),
                ) {
                    dst.extend(add.iter().cloned());
                }
            }
        }
    }
    if let Some(av) = profile.get("assetIndex").cloned() {
        vj["assetIndex"] = av;
    }
    if let Some(jv) = profile.get("javaVersion").cloned() {
        vj["javaVersion"] = jv;
    }
    if let Some(dt) = profile.get("data").cloned() {
        vj["data"] = dt;
    }
    if let Some(dl) = profile.get("downloads").filter(|v| !v.is_null()).cloned() {
        vj["downloads"] = dl;
    }
    if let Some(tp) = profile.get("type").and_then(|v| v.as_str()) {
        vj["type"] = Value::String(tp.to_string());
    }
}
