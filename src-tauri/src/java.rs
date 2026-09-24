use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct JavaInstall {
    pub path: String,
    pub version: String,
    pub major: u32,
    pub source: String,
}

fn query_java(bin: &str) -> Option<(String, u32)> {
    let out = Command::new(bin).arg("-version").output().ok()?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let start = text.find("version \"")? + 9;
    let raw = text[start..].split('"').next()?;
    let major = if raw.starts_with("1.") {
        raw.strip_prefix("1.")?.split('.').next()?.parse().unwrap_or(0)
    } else {
        raw.split(['.', '-']).next()?.parse().unwrap_or(0)
    };
    Some((raw.to_string(), major))
}

#[tauri::command]
pub fn detect_java() -> Vec<JavaInstall> {    let mut found: Vec<JavaInstall> = Vec::new();
    let mut seen: Vec<String> = Vec::new();

    // 1. archlinux-java status
    if let Ok(out) = Command::new("archlinux-java").arg("status").output() {
        let text = String::from_utf8_lossy(&out.stdout).to_string();
        for line in text.lines().skip(1) {
            let name = line.trim().split(' ').next().unwrap_or("").to_string();
            if name.is_empty() {
                continue;
            }
            let p = format!("/usr/lib/jvm/{}/bin/java", name);
            if Path::new(&p).exists() && !seen.contains(&p) {
                if let Some((version, major)) = query_java(&p) {
                    seen.push(p.clone());
                    found.push(JavaInstall {
                        path: p,
                        version,
                        major,
                        source: "archlinux-java".into(),
                    });
                }
            }
        }
    }

    // 2. /usr/lib/jvm scan
    if let Ok(entries) = fs::read_dir("/usr/lib/jvm") {
        for entry in entries.flatten() {
            let p = entry.path().join("bin/java");
            let ps = p.to_string_lossy().to_string();
            if p.exists() && !seen.contains(&ps) {
                if let Some((version, major)) = query_java(&ps) {
                    seen.push(ps.clone());
                    found.push(JavaInstall {
                        path: ps,
                        version,
                        major,
                        source: "/usr/lib/jvm".into(),
                    });
                }
            }
        }
    }

    // 2b. Windows / macOS well-known locations
    {
        #[allow(unused_mut)]
        let mut candidates: Vec<(String, PathBuf)> = Vec::new();
        #[cfg(windows)]
        {
            for var in ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"] {
                if let Ok(base) = std::env::var(var) {
                    for sub in ["Java", "Eclipse Adoptium", "Microsoft", "Zulu", "BellSoft"] {
                        let dir = PathBuf::from(&base).join(sub);
                        if let Ok(rd) = fs::read_dir(&dir) {
                            for e in rd.flatten() {
                                candidates.push((
                                    "Program Files".into(),
                                    e.path().join("bin").join("java.exe"),
                                ));
                            }
                        }
                    }
                }
            }
        }
        #[cfg(target_os = "macos")]
        {
            let base = Path::new("/Library/Java/JavaVirtualMachines");
            if let Ok(rd) = fs::read_dir(base) {
                for e in rd.flatten() {
                    candidates.push((
                        "JavaVirtualMachines".into(),
                        e.path()
                            .join("Contents")
                            .join("Home")
                            .join("bin")
                            .join("java"),
                    ));
                }
            }
            // /usr/libexec/java_home -V lists installed JDKs
            if let Ok(out) = Command::new("/usr/libexec/java_home")
                .arg("-V")
                .output()
            {
                for line in String::from_utf8_lossy(&out.stderr).lines() {
                    if let Some(rest) = line.split_whitespace().nth(1) {
                        if !rest.is_empty() && Path::new(rest).exists() {
                            candidates.push((
                                "java_home".into(),
                                Path::new(rest).join("bin").join("java"),
                            ));
                        }
                    }
                }
            }
        }
        for (src, p) in candidates {
            let ps = p.to_string_lossy().to_string();
            if p.exists() && !seen.contains(&ps) {
                if let Some((version, major)) = query_java(&ps) {
                    seen.push(ps.clone());
                    found.push(JavaInstall {
                        path: ps,
                        version,
                        major,
                        source: src,
                    });
                }
            }
        }
    }

    // 3. $JAVA_HOME
    if let Ok(jh) = std::env::var("JAVA_HOME") {
        let p = if cfg!(windows) {
            Path::new(&jh).join("bin").join("java.exe").to_string_lossy().to_string()
        } else {
            Path::new(&jh).join("bin").join("java").to_string_lossy().to_string()
        };
        if Path::new(&p).exists() && !seen.contains(&p) {
            if let Some((version, major)) = query_java(&p) {
                seen.push(p.clone());
                found.push(JavaInstall {
                    path: p,
                    version,
                    major,
                    source: "JAVA_HOME".into(),
                });
            }
        }
    }

    // 4. $PATH
    let which_cmd = if cfg!(windows) { "where" } else { "which" };
    if let Ok(out) = Command::new(which_cmd).arg("java").output() {
        let ps = String::from_utf8_lossy(&out.stdout)
            .lines()
            .next()
            .unwrap_or("")
            .trim()
            .to_string();
        if !ps.is_empty() && Path::new(&ps).exists() && !seen.contains(&ps) {
            if let Some((version, major)) = query_java(&ps) {
                found.push(JavaInstall {
                    path: ps,
                    version,
                    major,
                    source: "$PATH".into(),
                });
            }
        }
    }

    found.sort_by_key(|j| j.major);
    found.dedup_by(|a, b| a.path == b.path);
    found
}

#[tauri::command]
pub fn check_java_version(path: String) -> Result<String, String> {
    if !Path::new(&path).exists() {
        return Err(format!("Путь не найден: {}", path));
    }
    query_java(&path)
        .map(|(v, _)| v)
        .ok_or_else(|| "Не удалось определить версию (ожидается путь к бинарнику java)".to_string())
}

pub fn pick_java_path(req: u32) -> Option<String> {
    let all = detect_java();
    all.iter()
        .find(|j| j.major == req)
        .or_else(|| all.iter().filter(|j| j.major > req).min_by_key(|j| j.major))
        .map(|j| j.path.clone())
}
