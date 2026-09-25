use crate::LauncherState;
use serde::{Deserialize, Serialize};
use tauri::State;
use std::fs;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProxyConfig {
    /// "None" | "Socks5" | "Http"
    #[serde(default = "default_proxy_type")]
    pub kind: String,
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub port: u16,
    #[serde(default)]
    pub login: String,
    #[serde(default)]
    pub password: String,
}

fn default_proxy_type() -> String {
    "None".to_string()
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            kind: default_proxy_type(),
            host: String::new(),
            port: 0,
            login: String::new(),
            password: String::new(),
        }
    }
}

impl ProxyConfig {
    /// Java system properties for this proxy. SOCKS5 uses the Java SOCKS
    /// properties (auth via java.net.socks.*), HTTP covers both http/https.
    pub fn java_args(&self) -> Vec<String> {
        let host = self.host.trim();
        if matches!(self.kind.as_str(), "None" | "" | "none") || host.is_empty() || self.port == 0 {
            return Vec::new();
        }
        let h = host.to_string();
        let p = self.port.to_string();
        let mut out = Vec::new();
        if self.kind.eq_ignore_ascii_case("socks5") || self.kind.eq_ignore_ascii_case("socks") {
            out.push(format!("-DsocksProxyHost={}", h));
            out.push(format!("-DsocksProxyPort={}", p));
            if !self.login.trim().is_empty() {
                out.push(format!("-Djava.net.socks.username={}", self.login.trim()));
            }
            if !self.password.is_empty() {
                out.push(format!("-Djava.net.socks.password={}", self.password));
            }
        } else {
            out.push(format!("-Dhttp.proxyHost={}", h));
            out.push(format!("-Dhttp.proxyPort={}", p));
            out.push(format!("-Dhttps.proxyHost={}", h));
            out.push(format!("-Dhttps.proxyPort={}", p));
        }
        out
    }

    /// Short, secret-free description for the log.
    pub fn describe(&self) -> String {
        if self.java_args().is_empty() {
            return "без прокси".to_string();
        }
        let auth = if self.login.trim().is_empty() && self.password.is_empty() {
            ""
        } else {
            ", с авторизацией"
        };
        format!("{} {}:{}{}", self.kind, self.host.trim(), self.port, auth)
    }
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct InstanceConfig {
    pub id: String,
    pub name: String,
    pub version: String,
    pub loader: String,
    pub created: String,
    #[serde(default)]
    pub uuid: String,
    /// Per-instance proxy. Absent in configs written by older versions.
    #[serde(default)]
    pub proxy: ProxyConfig,
    /// Let the launcher pick GC flags based on Java + Minecraft version.
    #[serde(default = "yes")]
    pub auto_gc: bool,
    /// Let the launcher pick Xms/Xmx from the version and system RAM.
    #[serde(default)]
    pub auto_mem: bool,
}

fn yes() -> bool {
    true
}

pub fn pseudo_uuid(seed: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in seed.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    for round in 0..3 {
        h ^= h >> 12;
        h = h.wrapping_mul(0x2545F4914F6CDD1D);
        h ^= h >> 31;
        let _ = round;
    }
    format!(
        "{:08x}-{:04x}-4{:03x}-8{:03x}-{:012x}",
        h & 0xFFFF_FFFF,
        (h >> 32) & 0xFFFF,
        (h >> 16) & 0xFFF,
        (h >> 48) & 0xFFF,
        h & 0xFFFF_FFFF_FFFF
    )
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceInfo {
    pub config: InstanceConfig,
    pub dir: String,
    pub mod_count: usize,
    pub has_run_script: bool,
}

pub fn instances_root() -> PathBuf {
    let root = crate::versions::data_root().join("instances");
    fs::create_dir_all(&root).ok();
    root
}

const LOADERS: [&str; 4] = ["Vanilla", "Fabric", "NeoForge", "Forge"];

pub fn is_valid_loader(loader: &str) -> bool {
    LOADERS.iter().any(|l| l.eq_ignore_ascii_case(loader))
}

pub fn canonical_loader(loader: &str) -> Result<String, String> {
    LOADERS
        .iter()
        .find(|l| l.eq_ignore_ascii_case(loader))
        .map(|l| l.to_string())
        .ok_or_else(|| {
            format!(
                "неизвестный загрузчик «{}» (доступно: {})",
                loader,
                LOADERS.join(", ")
            )
        })
}

/// Rejects empty ids, path separators, `..` and anything that is not a plain
/// directory name — commands take instance ids from the webview, so a
/// compromised frontend must not be able to escape the instances root.
pub fn valid_instance_id(id: &str) -> bool {
    !id.is_empty()
        && id != "."
        && id != ".."
        && id.len() <= 96
        && !id.contains('/')
        && !id.contains('\\')
        && !id.contains('\0')
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

pub fn instance_dir(id: &str) -> Result<PathBuf, String> {
    if !valid_instance_id(id) {
        return Err(format!("недопустимый идентификатор инстанса «{}»", id));
    }
    Ok(instances_root().join(id))
}

/// Validates a relative path coming from untrusted metadata (modpack indexes,
/// Mojang/Modrinth manifests) and returns it in normalized form.
pub fn safe_rel(rel: &str) -> Result<PathBuf, String> {
    let cleaned = rel.replace('\\', "/");
    let p = std::path::Path::new(&cleaned);
    if cleaned.trim().is_empty() || cleaned.starts_with('/') || p.is_absolute() {
        return Err(format!("недопустимый путь «{}»", rel));
    }
    let mut out = PathBuf::new();
    let mut depth = 0usize;
    for comp in p.components() {
        match comp {
            std::path::Component::Normal(c) => {
                out.push(c);
                depth += 1;
            }
            std::path::Component::CurDir => {}
            _ => return Err(format!("недопустимый путь «{}»", rel)),
        }
    }
    if out.as_os_str().is_empty() || depth == 0 {
        return Err(format!("недопустимый путь «{}»", rel));
    }
    Ok(out)
}

/// Joins a validated relative path and guarantees the result stays inside
/// `base` — protects against zip-slip / absolute paths in modpack archives.
pub fn safe_join(base: &std::path::Path, rel: &str) -> Result<PathBuf, String> {
    let r = safe_rel(rel)?;
    let first = r.components().next().map(|c| c.as_os_str().to_string_lossy().to_string());
    if matches!(first.as_deref(), Some("instance.json") | Some("run.sh")) {
        return Err(format!("путь «{}» зарезервирован лаунчером", rel));
    }
    let out = base.join(&r);
    if !out.starts_with(base) {
        return Err(format!("путь «{}» выходит за пределы каталога", rel));
    }
    Ok(out)
}

fn read_config(dir: &std::path::Path) -> Option<InstanceConfig> {
    let raw = fs::read_to_string(dir.join("instance.json")).ok()?;
    let mut cfg: InstanceConfig = serde_json::from_str(&raw).ok()?;
    if cfg.id.trim().is_empty()
        || cfg.version.trim().is_empty()
        || !is_valid_loader(&cfg.loader)
    {
        return None;
    }
    // tolerate unknown proxy kinds from hand-edited configs
    if !matches!(cfg.proxy.kind.as_str(), "None" | "Socks5" | "Http") {
        cfg.proxy.kind = "None".into();
    }
    Some(cfg)
}

#[tauri::command]
pub fn list_instances() -> Vec<InstanceInfo> {
    let root = instances_root();
    let mut list = Vec::new();
    if let Ok(entries) = fs::read_dir(root) {
        for e in entries.flatten() {
            if let Some(cfg) = read_config(&e.path()) {
                let mods = e.path().join("mods");
                let mod_count = fs::read_dir(&mods)
                    .map(|rd| rd.flatten().count())
                    .unwrap_or(0);
                let has_run_script = e.path().join("run.sh").exists();
                list.push(InstanceInfo {
                    config: cfg,
                    dir: e.path().to_string_lossy().to_string(),
                    mod_count,
                    has_run_script,
                });
            }
        }
    }
    list.sort_by(|a, b| b.config.created.cmp(&a.config.created));
    list
}

fn slugify(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .split("-")
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

#[tauri::command]
pub fn create_instance(
    name: String,
    version: String,
    loader: String,
) -> Result<InstanceInfo, String> {
    let base = slugify(&name);
    if base.is_empty() {
        return Err("Некорректное имя инстанса".into());
    }
    let loader = canonical_loader(&loader)?;
    let version = version.trim().to_string();
    if version.is_empty() || version.len() > 32 {
        return Err("Некорректная версия Minecraft".into());
    }
    let mut id = base.clone();
    let mut n = 1;
    while instance_dir(&id).map(|d| d.exists()).unwrap_or(false) {
        n += 1;
        id = format!("{}-{}", base, n);
    }
    let dir = instance_dir(&id)?;
    for sub in ["", "mods", "resourcepacks", "shaderpacks", "saves", "logs"] {
        if let Err(e) = fs::create_dir_all(dir.join(sub)) {
            let _ = fs::remove_dir_all(&dir);
            return Err(e.to_string());
        }
    }
    let cfg = InstanceConfig {
        id: id.clone(),
        name,
        version,
        loader,
        created: chrono_now(),
        uuid: pseudo_uuid(&id),
        proxy: ProxyConfig::default(),
        auto_gc: true,
        auto_mem: false,
    };
    if let Err(e) = fs::write(
        dir.join("instance.json"),
        serde_json::to_string_pretty(&cfg).map_err(|e| e.to_string())?,
    ) {
        let _ = fs::remove_dir_all(&dir);
        return Err(e.to_string());
    }
    Ok(InstanceInfo {
        config: cfg,
        dir: dir.to_string_lossy().to_string(),
        mod_count: 0,
        has_run_script: false,
    })
}

#[tauri::command]
pub fn delete_instance(id: String) -> Result<(), String> {
    let dir = instance_dir(&id)?;
    if !dir.exists() {
        return Err("Инстанс не найден".into());
    }
    fs::remove_dir_all(dir).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn download_mod(
    state: State<'_, LauncherState>,
    instance_id: String,
    url: String,
    filename: String,
    sub: Option<String>,
) -> Result<String, String> {
    let _ = state.cancel.send(false);
    let sub = sub.unwrap_or_else(|| "mods".to_string());
    if !matches!(sub.as_str(), "mods" | "resourcepacks" | "shaderpacks") {
        return Err("недопустимая целевая папка".into());
    }
    if !url.starts_with("https://cdn.modrinth.com/") && !url.starts_with("https://github.com/") {
        return Err("Разрешены только URL с cdn.modrinth.com / github.com".into());
    }
    let safe_name = filename
        .chars()
        .map(|c| if c.is_alphanumeric() || ".-_".contains(c) { c } else { '_' })
        .collect::<String>();
    let dir = instance_dir(&instance_id)?.join(&sub);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let target = dir.join(&safe_name);
    let c = reqwest::Client::builder()
        .user_agent("deBang-Launcher/0.1")
        .build()
        .map_err(|e| e.to_string())?;
    crate::versions::download_file(&c, &url, &target, None, Some(&state.cancel.subscribe())).await?;
    Ok(target.to_string_lossy().to_string())
}

/// Copies a user-picked wallpaper/video into the launcher data directory and
/// returns the stored path. The webview's asset scope only covers that
/// directory, so a compromised frontend cannot read arbitrary files.
/// Writes instance.json with owner-only permissions — it may contain proxy
/// credentials.
pub fn write_config(dir: &std::path::Path, cfg: &InstanceConfig) -> Result<(), String> {
    let body = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    let path = dir.join("instance.json");
    fs::write(&path, body).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

fn info_for(cfg: InstanceConfig) -> InstanceInfo {
    let dir = instance_dir(&cfg.id).unwrap_or_else(|_| PathBuf::from(&cfg.id));
    let mod_count = fs::read_dir(dir.join("mods"))
        .map(|rd| rd.flatten().count())
        .unwrap_or(0);
    InstanceInfo {
        config: cfg,
        dir: dir.to_string_lossy().to_string(),
        mod_count,
        has_run_script: dir.join("run.sh").exists() || dir.join("run.bat").exists(),
    }
}

#[tauri::command]
/// Loose parameters on purpose: Tauri resolves each argument by its (snake_
/// case) name, so the frontend must send exactly these keys:
/// `instanceId`, `proxy`, `autoGc`, `autoMem` — see the contract test below.
pub fn update_instance_settings(
    instance_id: String,
    proxy: Option<ProxyConfig>,
    auto_gc: Option<bool>,
    auto_mem: Option<bool>,
) -> Result<InstanceInfo, String> {
    let dir = instance_dir(&instance_id)?;
    let mut cfg = read_config(&dir).ok_or("Инстанс не найден или повреждён")?;
    if let Some(p) = proxy {
        if !matches!(p.kind.as_str(), "None" | "Socks5" | "Http") {
            return Err("неизвестный тип прокси".into());
        }
        if p.kind != "None" {
            if p.host.trim().is_empty() {
                return Err("укажите хост прокси".into());
            }
            if p.port == 0 {
                return Err("укажите порт прокси".into());
            }
            if p.host.contains(char::is_whitespace) {
                return Err("недопустимый хост прокси".into());
            }
        }
        cfg.proxy = p;
    }
    if let Some(v) = auto_gc {
        cfg.auto_gc = v;
    }
    if let Some(v) = auto_mem {
        cfg.auto_mem = v;
    }
    write_config(&dir, &cfg)?;
    Ok(info_for(cfg))
}

#[tauri::command]
pub fn import_background(src: String) -> Result<String, String> {
    let src_path = std::path::Path::new(&src);
    if !src_path.is_file() {
        return Err("Файл не найден".into());
    }
    let ext = src_path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if !matches!(
        ext.as_str(),
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "mp4" | "webm" | "mkv"
    ) {
        return Err("Поддерживаются только изображения и видео (png, jpg, webp, gif, mp4, webm, mkv)".into());
    }
    let size = fs::metadata(src_path).map(|m| m.len()).unwrap_or(0);
    if size == 0 || size > 512 * 1024 * 1024 {
        return Err("Файл пуст или слишком большой (>512 МБ)".into());
    }
    let dir = crate::versions::data_root().join("backgrounds");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let stem: String = src_path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "background".into())
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let target = dir.join(format!("{}-{}.{}", stem, stamp, ext));
    fs::copy(src_path, &target).map_err(|e| e.to_string())?;
    Ok(target.to_string_lossy().to_string())
}

#[tauri::command]
pub fn import_run_file(instance_id: String, src: String) -> Result<String, String> {
    let src_path = std::path::Path::new(&src);
    let name = src_path
        .file_name()
        .ok_or("Некорректный путь")?
        .to_string_lossy()
        .to_string();
    if !matches!(
        name.as_str(),
        "minecraft.jar" | "server.jar" | "run.sh" | "run.bat" | "run.cmd"
    ) {
        return Err("Разрешены только minecraft.jar, server.jar, run.sh, run.bat или run.cmd (для очень старых сборок)".into());
    }
    let dir = instance_dir(&instance_id)?;
    if !dir.join("instance.json").exists() {
        return Err("Инстанс не найден".into());
    }
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let target = dir.join(&name);
    fs::copy(src_path, &target).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    if name.ends_with(".sh") {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&target, fs::Permissions::from_mode(0o755)).ok();
    }
    Ok(target.to_string_lossy().to_string())
}

fn chrono_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // lightweight RFC3339-ish UTC without chrono dependency
    let days = (secs / 86400) as i64;
    let rem = secs % 86400;
    let (y, m, d) = civil_from_days(days);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        y,
        m,
        d,
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod settings_ipc_tests {
    use super::*;

    /// Mirrors the payload src/lib/api.ts sends. The command itself must keep
    /// loose parameters named exactly like these fields in snake_case:
    /// instanceId -> instance_id, autoGc -> auto_gc, autoMem -> auto_mem.
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct FrontendPayload {
        instance_id: String,
        #[serde(default)]
        proxy: Option<ProxyConfig>,
        #[serde(default)]
        auto_gc: Option<bool>,
        #[serde(default)]
        auto_mem: Option<bool>,
    }

    #[test]
    fn frontend_payload_matches_command_args() {
        let raw = r#"{
            "instanceId": "my-instance",
            "autoGc": true,
            "autoMem": false,
            "proxy": { "kind": "Socks5", "host": "127.0.0.1", "port": 1080, "login": "u", "password": "p" }
        }"#;
        let p: FrontendPayload = serde_json::from_str(raw).expect("payload must deserialise");
        assert_eq!(p.instance_id, "my-instance");
        assert_eq!(p.auto_gc, Some(true));
        assert_eq!(p.auto_mem, Some(false));
        let proxy = p.proxy.expect("proxy present");
        assert_eq!(proxy.kind, "Socks5");
        assert_eq!(proxy.port, 1080);
    }

    #[test]
    fn proxy_defaults_when_only_toggles_are_sent() {
        let p: FrontendPayload =
            serde_json::from_str(r#"{"instanceId":"x","autoMem":true}"#).unwrap();
        assert!(p.proxy.is_none());
        assert_eq!(p.auto_mem, Some(true));
        assert!(p.auto_gc.is_none());
    }
}
