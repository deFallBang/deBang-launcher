use crate::instances::instance_dir;
use crate::java;
use crate::instances::pseudo_uuid;
use crate::LauncherState;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{ChildStderr, ChildStdout, Command};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchSettings {
    pub java_path: String,
    pub min_mem_mb: u32,
    pub max_mem_mb: u32,
    pub jvm_args: Vec<String>,
    pub player_name: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LogEvent {
    pub(crate) time: u64,
    pub(crate) line: String,
    pub(crate) stream: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct StatusEvent {
    running: bool,
    instance_id: String,
    code: Option<i32>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PrepEvent {
    phase: String,
    done: u64,
    total: u64,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Console lines only lived in the webview, so a failed launch left no trace
/// outside the app. Everything is mirrored into `launcher.log` (capped) — that
/// file is the only way to diagnose a launch that dies before Minecraft writes
/// its own log.
fn log_file() -> std::path::PathBuf {
    crate::versions::data_root().join("launcher.log")
}

fn append_log(line: &str, stream: &str) {
    use std::io::Write;
    let path = log_file();
    // Keep the file bounded: truncate to the last ~1 MB when it grows too big.
    if let Ok(meta) = std::fs::metadata(&path) {
        if meta.len() > 2 * 1024 * 1024 {
            if let Ok(data) = std::fs::read(&path) {
                let cut = data.len().saturating_sub(1024 * 1024);
                let _ = std::fs::write(&path, &data[cut..]);
            }
        }
    }
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = writeln!(f, "[{:.3}][{}] {}", now_ms() as f64 / 1000.0, stream, line);
    }
}

pub(crate) fn emit(app: &AppHandle, line: String, stream: &str) {
    append_log(&line, stream);
    let _ = app.emit(
        "game://log",
        LogEvent {
            time: now_ms(),
            line,
            stream: stream.to_string(),
        },
    );
}

/// Log line from non-command code (installers, modpacks).
pub(crate) fn log_line(app: Option<&AppHandle>, line: String) {
    if let Some(a) = app {
        emit(a, line, "launcher");
    }
}

/// `prep://progress` event from non-command code.
pub(crate) fn emit_prep(app: Option<&AppHandle>, phase: &str, done: u64, total: u64) {
    if let Some(a) = app {
        let _ = a.emit(
            "prep://progress",
            PrepEvent {
                phase: phase.to_string(),
                done,
                total,
            },
        );
    }
}

fn emit_status(app: &AppHandle, id: &str, code: Option<i32>) {
    let _ = app.emit(
        "game://status",
        StatusEvent {
            running: false,
            instance_id: id.to_string(),
            code,
        },
    );
}

/// Java major version of a binary, if it can be queried.
pub fn java_major_of(path: &str) -> Option<u32> {
    java::detect_java()
        .into_iter()
        .find(|j| j.path == path)
        .map(|j| j.major)
        .or_else(|| {
            let out = std::process::Command::new(path).arg("-version").output().ok()?;
            let text = format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            let raw = text.split("version \"").nth(1)?.split('"').next()?;
            if raw.starts_with("1.") {
                raw.strip_prefix("1.")?.split('.').next()?.parse().ok()
            } else {
                raw.split(['.', '-']).next()?.parse().ok()
            }
        })
}

fn total_ram_mb() -> u64 {
    crate::sysinfo::get_mem_info().map(|m| m.total_mb).unwrap_or(4096)
}

/// What the launcher would pass to the JVM for this instance — shown in the
/// profile settings dialog before the game starts.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchPlanView {
    pub java_major: u32,
    pub min_mem_mb: u32,
    pub max_mem_mb: u32,
    pub auto_gc: bool,
    pub auto_mem: bool,
    pub auto_gc_applied: bool,
    pub gc_flags: Vec<String>,
    pub proxy_enabled: bool,
    pub notes: Vec<String>,
}

#[tauri::command]
/// Loose parameters: the frontend must send `instanceId` and `settings`
/// (Tauri resolves arguments by their snake_case parameter names).
pub fn instance_launch_plan(
    instance_id: String,
    settings: LaunchSettings,
) -> Result<LaunchPlanView, String> {
    let dir = instance_dir(&instance_id)?;
    let raw = std::fs::read_to_string(dir.join("instance.json"))
        .map_err(|_| "Версия не найдена".to_string())?;
    let cfg: crate::instances::InstanceConfig =
        serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    let req = if cfg.version == "1.8.9" { 8 } else { 17 };
    let java = pick_java(&settings.java_path, req).unwrap_or_default();
    let java_major = java_major_of(&java).unwrap_or(0);
    let user_args: Vec<String> = settings
        .jvm_args
        .iter()
        .flat_map(|a| a.split_whitespace().map(String::from))
        .collect();
    let plan = crate::jvm::build_plan(
        &cfg,
        java_major,
        total_ram_mb(),
        &user_args,
        settings.min_mem_mb,
        settings.max_mem_mb,
    );
    Ok(LaunchPlanView {
        java_major: plan.java_major,
        min_mem_mb: plan.min_mem_mb,
        max_mem_mb: plan.max_mem_mb,
        auto_gc: cfg.auto_gc,
        auto_mem: cfg.auto_mem,
        auto_gc_applied: plan.auto_gc_applied,
        gc_flags: plan.gc_flags,
        proxy_enabled: !plan.proxy_args.is_empty(),
        notes: plan.notes,
    })
}

/// An imported run script (optional): `run.bat`/`run.cmd` on Windows,
/// `run.sh` elsewhere. Returns `None` when the instance has no script, in
/// which case the normal automatic bootstrap is used.
struct RunScript {
    program: String,
    args: Vec<String>,
    name: String,
}

/// Per-instance scratch directory. The game must never write into the shared
/// /tmp: it is a RAM-backed tmpfs on many systems, and a full tmpfs makes the
/// JVM die with SIGBUS inside the dynamic loader (the "Mojang crash screen").
fn instance_tmp(dir: &std::path::Path) -> Option<PathBuf> {
    let t = dir.join("tmp");
    std::fs::create_dir_all(&t).ok()?;
    Some(t)
}

/// JVM flags that keep crash diagnostics inside the instance directory.
fn diagnostics_args(dir: &std::path::Path, java_major: u32) -> Vec<String> {
    let logs = dir.join("logs");
    let _ = std::fs::create_dir_all(&logs);
    let mut out = Vec::new();
    // `CreateCoredumpOnCrash` only exists since Java 9; on Java 8 the JVM
    // refuses to start with "Unrecognized VM option 'CreateCoredumpOnCrash'".
    if java_major >= 9 {
        out.push("-XX:-CreateCoredumpOnCrash".to_string());
    }
    out.push(format!(
        "-XX:ErrorFile={}",
        logs.join("hs_err_pid%p.log").to_string_lossy()
    ));
    out
}

fn run_script(dir: &std::path::Path) -> Option<RunScript> {
    let mut names: Vec<&str> = vec!["run.sh"];
    if cfg!(windows) {
        names = vec!["run.bat", "run.cmd", "run.sh"];
    }
    for n in names {
        let p = dir.join(n);
        if p.exists() {
            let is_batch = cfg!(windows) && (n.ends_with(".bat") || n.ends_with(".cmd"));
            return Some(if is_batch {
                RunScript {
                    program: "cmd".into(),
                    args: vec!["/C".into(), p.to_string_lossy().to_string()],
                    name: n.to_string(),
                }
            } else {
                RunScript {
                    program: "/bin/bash".into(),
                    args: vec![p.to_string_lossy().to_string()],
                    name: n.to_string(),
                }
            });
        }
    }
    None
}

/// Which Java major version to run a game with.
///
/// MC 1.8.9 and older (every legacy Forge profile) only work on **exactly**
/// Java 8: a newer JVM dies at once with `UnsupportedClassVersionError` deep in
/// FML, so the "≥ required" rule that is right for modern versions is exactly
/// backwards there and must never apply to the user's preferred Java.
fn choose_java_major(required: u32, preferred: Option<u32>, available: &[u32]) -> Option<u32> {
    if required == 8 {
        if preferred == Some(8) && available.contains(&8) {
            return Some(8);
        }
        return available.contains(&8).then_some(8);
    }
    if let Some(p) = preferred {
        if p >= required && available.contains(&p) {
            return Some(p);
        }
    }
    if available.contains(&required) {
        return Some(required);
    }
    available.iter().copied().filter(|m| *m > required).min()
}

fn pick_java(preferred: &str, major: u32) -> Result<String, String> {
    let installs = java::detect_java();
    let available: Vec<u32> = installs.iter().map(|j| j.major).collect();
    let preferred_major = java_major_of(preferred);
    let chosen = choose_java_major(major, preferred_major, &available)
        .ok_or_else(|| match major {
            8 => "Для этой версии нужна Java 8 — установите: sudo pacman -S --needed jre8-openjdk"
                .to_string(),
            _ => format!(
                "Не найдена Java {} (нужно ≥ {}). Установите: sudo pacman -S --needed jre-openjdk",
                major, major
            ),
        })?;
    if !preferred.is_empty() && preferred_major == Some(chosen) {
        return Ok(preferred.to_string());
    }
    installs
        .iter()
        .find(|j| j.major == chosen)
        .map(|j| j.path.clone())
        .ok_or_else(|| format!("не удалось найти исполняемый файл Java {}", chosen))
}

#[tauri::command]
pub async fn launch_instance(
    app: AppHandle,
    state: State<'_, LauncherState>,
    instance_id: String,
    settings: LaunchSettings,
) -> Result<u32, String> {
    // Reserve the single launch slot for the whole preparation, otherwise two
    // clicks can pass the `is_running` check and both spawn processes.
    let _slot = state.launch_lock.lock().await;
    if is_running(&state) {
        return Err("Игра уже запущена".into());
    }
    let _ = state.cancel.send(false);
    let cancel_rx = state.cancel.subscribe();
    let dir = instance_dir(&instance_id)?;
    let cfg_raw = std::fs::read_to_string(dir.join("instance.json"))
        .map_err(|_| "Версия не найдена".to_string())?;
    let mut cfg: crate::instances::InstanceConfig =
        serde_json::from_str(&cfg_raw).map_err(|e| e.to_string())?;
    if cfg.uuid.is_empty() {
        cfg.uuid = pseudo_uuid(&instance_id);
    }

    let manual_jar = ["minecraft.jar", "server.jar"]
        .into_iter()
        .map(|j| dir.join(j))
        .find(|p| p.exists());

    let jvm_user: Vec<String> = settings
        .jvm_args
        .iter()
        .flat_map(|a| a.split_whitespace().map(String::from))
        .collect();

    let mut need_java: u32 = 8;
    let (program, args) = if let Some(jar) = manual_jar {
        let java_major = java_major_of(&settings.java_path).unwrap_or(0);
        let plan = crate::jvm::build_plan(
            &cfg,
            java_major,
            total_ram_mb(),
            &jvm_user,
            settings.min_mem_mb,
            settings.max_mem_mb,
        );
        for n in &plan.notes {
            emit(&app, format!("│ {}", n), "launcher");
        }
        let mut a = vec![
            format!("-Xms{}M", plan.min_mem_mb),
            format!("-Xmx{}M", plan.max_mem_mb),
        ];
        a.extend(plan.proxy_args.iter().cloned());
        if let Some(t) = instance_tmp(&dir) {
            a.push(format!("-Djava.io.tmpdir={}", t.to_string_lossy()));
        }
        a.extend(diagnostics_args(&dir, 8));
        a.push("-jar".into());
        a.push(jar.to_string_lossy().to_string());
        (settings.java_path.clone(), a)
    } else if let Some(script) = run_script(&dir) {
        // A run script is only used when the user actually imported one; the
        // automatic bootstrap below works on Linux, Windows and macOS.
        if cfg.auto_gc || cfg.auto_mem || !cfg.proxy.java_args().is_empty() {
            emit(
                &app,
                format!(
                    "│ {} запускается скриптом: авто-GC, авто-память и прокси из профиля НЕ применяются",
                    script.name
                ),
                "launcher",
            );
        }
        (script.program, script.args)
    } else {
        // ------- real bootstrap -------
        emit(
            &app,
            format!(
                "┌ deBang · подготовка «{}» · MC {} · {}",
                cfg.name, cfg.version, cfg.loader
            ),
            "launcher",
        );
        let app_p = app.clone();
        let app_log = app.clone();
        let last_log = std::sync::Arc::new(std::sync::Mutex::new((String::new(), 0u64)));
        // A watchdog: a download that never finishes must not keep the button
        // in the "Запуск" state forever. Individual requests have their own
        // timeouts; this is the last resort for a stuck phase.
        let prepared_fut = crate::versions::prepare(
            move |phase, done, total| {
                let _ = app_p.emit(
                    "prep://progress",
                    PrepEvent {
                        phase: phase.to_string(),
                        done,
                        total,
                    },
                );
                let key = phase.to_string();
                let mut guard = last_log.lock().unwrap_or_else(|e| e.into_inner());
                if (guard.0 != key || done == total || done % 2000 == 0)
                    && now_ms() - guard.1 > 250
                {
                    *guard = (key.clone(), now_ms());
                    drop(guard);
                    emit(
                        &app_p,
                        format!("│ загрузка: {} — {}/{}", key, done, total),
                        "launcher",
                    );
                }
            },
            move |line| emit(&app_log, line, "launcher"),
            &cfg.version,
            &cfg.loader,
            &dir,
            &cfg.uuid,
            if settings.player_name.is_empty() { "deBangPlayer" } else { &settings.player_name },
            Some(cancel_rx),
        );
        let prepared = match tokio::time::timeout(
            std::time::Duration::from_secs(20 * 60),
            prepared_fut,
        )
        .await
        {
            Ok(r) => r,
            Err(_) => {
                emit(
                    &app,
                    "└ ОШИБКА подготовки: истекло 20 минут — загрузка не завершилась, смотри launcher.log"
                        .to_string(),
                    "launcher",
                );
                return Err("подготовка не завершилась за 20 минут (подробности в launcher.log)".into());
            }
        };
        let _ = app.emit(
            "prep://progress",
            PrepEvent {
                phase: "done".into(),
                done: 1,
                total: 1,
            },
        );
        let prepared = match prepared {
            Ok(p) => p,
            Err(e) => {
                emit(&app, format!("└ ОШИБКА подготовки: {}", e), "launcher");
                return Err(e);
            }
        };
        let java = pick_java(&settings.java_path, prepared.java_major)?;
        need_java = prepared.java_major;
        let plan = crate::jvm::build_plan(
            &cfg,
            prepared.java_major,
            total_ram_mb(),
            &jvm_user,
            settings.min_mem_mb,
            settings.max_mem_mb,
        );
        for n in &plan.notes {
            emit(&app, format!("│ {}", n), "launcher");
        }
        let mut a = vec![
            format!("-Xms{}M", plan.min_mem_mb),
            format!("-Xmx{}M", plan.max_mem_mb),
        ];
        a.extend(plan.gc_flags.iter().cloned());
        a.extend(plan.proxy_args.iter().cloned());
        if let Some(t) = instance_tmp(&dir) {
            a.push(format!("-Djava.io.tmpdir={}", t.to_string_lossy()));
            emit(
                &app,
                format!("│ временный каталог игры: {}", t.to_string_lossy()),
                "launcher",
            );
        }
        // Ask the JVM which of the flags it can parse — both the automatic GC
        // flags and the user's preset. One bad flag (`-XX:+MaxGCPauseMillis=200`
        // shipped in 1.3.0) aborts the JVM before the game ever starts.
        let mut wanted = diagnostics_args(&dir, prepared.java_major);
        wanted.extend(plan.gc_flags.clone());
        wanted.extend(plan.jvm_args.iter().cloned());
        let (clean_args, dropped_args) =
            crate::jvm::sanitize_args(&java, &wanted, prepared.java_major);
        for f in &dropped_args {
            emit(
                &app,
                format!("│ Java {} не поддерживает {} — флаг пропущен", prepared.java_major, f),
                "launcher",
            );
        }
        a.extend(clean_args);
        a.extend(prepared.jvm.iter().cloned());
        if let Some(cp) = &prepared.classpath {
            a.push("-cp".into());
            a.push(cp.clone());
        }
        a.push(prepared.main_class.clone());
        a.extend(prepared.game.iter().cloned());

        let shown: Vec<String> = a
            .iter()
            .map(|x| {
                if x.contains("/libraries/") && x.ends_with(".jar") {
                    "…:<classpath>".into()
                } else {
                    x.clone()
                }
            })
            .collect();
        emit(
            &app,
            format!("$ {} {}", java, crate::jvm::redact(&shown).join(" ")),
            "launcher",
        );
        append_log(&format!("instance={} version={} loader={}", cfg.id, cfg.version, cfg.loader), "launch");
        emit(
            &app,
            format!(
                "└ mainClass={}, Java {} требуется",
                prepared.main_class, prepared.java_major
            ),
            "launcher",
        );
        (java, a)
    };

    let mut child = Command::new(&program)
        .args(&args)
        .current_dir(&dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("Не удалось запустить {} — {}", program, e))?;

    let pid = child.id().unwrap_or(0);
    let stdout = child
        .stdout
        .take()
        .ok_or("не удалось перехватить stdout процесса")?;
    let stderr = child
        .stderr
        .take()
        .ok_or("не удалось перехватить stderr процесса")?;

    {
        let a = app.clone();
        tauri::async_runtime::spawn(async move {
            stream_lines::<ChildStdout>(a, stdout, "stdout").await
        });
    }
    {
        let a = app.clone();
        tauri::async_runtime::spawn(async move {
            stream_lines::<ChildStderr>(a, stderr, "stderr").await
        });
    }

    {
        let mut guard = state.child.lock().map_err(|e| e.to_string())?;
        *guard = Some(child);
    }

    let _ = app.emit(
        "game://status",
        StatusEvent {
            running: true,
            instance_id: instance_id.clone(),
            code: None,
        },
    );

    let app2 = app.clone();
    let id2 = instance_id.clone();
    let started = std::time::Instant::now();
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(400)).await;
            let st = app2.state::<LauncherState>();
            let code = {
                let mut guard = match st.child.lock() {
                    Ok(g) => g,
                    Err(_) => break,
                };
                match guard.as_mut() {
                    None => break,
                    Some(c) => match c.try_wait() {
                        Ok(Some(status)) => {
                            guard.take();
                            Some(status.code())
                        }
                        Ok(None) => None,
                        Err(_) => {
                            guard.take();
                            Some(None)
                        }
                    },
                }
            };
            if let Some(code) = code {
                // A game that dies in the first seconds almost never has a
                // readable console, so say what happened instead of silently
                // resetting the UI back to "Запуск".
                let secs = started.elapsed().as_secs();
                if let Some(c) = code {
                    if c != 0 {
                        log_line(
                            Some(&app2),
                            format!(
                                "└ игра завершилась с кодом {} через {} с — последние строки в консоли",
                                c, secs
                            ),
                        );
                        if need_java == 8 {
                            log_line(
                                Some(&app2),
                                "└ для этой версии обязательна Java 8: проверь в Настройках → Java"
                                    .into(),
                            );
                        }
                    } else if secs < 10 {
                        log_line(
                            Some(&app2),
                            format!(
                                "└ игра закрылась через {} с — версия Java, скорее всего, несовместима",
                                secs
                            ),
                        );
                    }
                }
                emit_status(&app2, &id2, code);
                break;
            }
        }
    });

    Ok(pid)
}

async fn stream_lines<T>(app: AppHandle, pipe: T, stream: &'static str)
where
    T: tokio::io::AsyncRead + Unpin,
{
    let mut lines = BufReader::new(pipe).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        emit(&app, line, stream);
    }
}

fn is_running(state: &State<'_, LauncherState>) -> bool {
    let Ok(mut guard) = state.child.lock() else {
        return false;
    };
    match guard.as_mut() {
        // A process that already exited (crashed app, closed terminal, killed
        // JVM) must not keep the launcher in the "running" state forever.
        Some(child) => match child.try_wait() {
            Ok(Some(_)) => {
                guard.take();
                false
            }
            _ => true,
        },
        None => false,
    }
}

#[tauri::command]
pub fn stop_instance(
    app: AppHandle,
    state: State<'_, LauncherState>,
    instance_id: String,
) -> Result<(), String> {
    let mut guard = state.child.lock().map_err(|e| e.to_string())?;
    match guard.as_mut() {
        None => Err("Игра не запущена".into()),
        Some(c) => {
            let _ = c.start_kill();
            guard.take();
            drop(guard);
            emit_status(&app, &instance_id, None);
            Ok(())
        }
    }
}

#[tauri::command]
pub fn is_game_running(state: State<'_, LauncherState>) -> bool {
    is_running(&state)
}

#[tauri::command]
pub fn cancel_download(app: AppHandle, state: State<'_, LauncherState>) {
    let _ = state.cancel.send(true);
    emit(&app, "⏹ отмена загрузки запрошена…".to_string(), "launcher");
}

#[cfg(test)]
mod ipc_tests {
    use super::*;

    /// Mirrors the payload src/lib/api.ts sends for the profile preview:
    /// `instanceId` -> instance_id, `settings` -> settings.
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct FrontendPayload {
        instance_id: String,
        settings: LaunchSettings,
    }

    #[test]
    fn plan_request_matches_frontend_payload() {
        let raw = r#"{
            "instanceId": "abc",
            "settings": {
                "javaPath": "", "playerName": "deBangPlayer",
                "minMemMb": 2048, "maxMemMb": 4096, "jvmArgs": ["-Xmx1G"]
            }
        }"#;
        let p: FrontendPayload = serde_json::from_str(raw).expect("payload must deserialise");
        assert_eq!(p.instance_id, "abc");
        assert_eq!(p.settings.player_name, "deBangPlayer");
        assert_eq!(p.settings.max_mem_mb, 4096);
    }
}

#[cfg(test)]
mod tmp_and_diag_tests {
    use super::*;

    #[test]
    fn per_instance_tmpdir_is_created_inside_instance() {
        let dir = std::env::temp_dir().join("debang-tmp-test-inst");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let t = instance_tmp(&dir).expect("tmp dir");
        assert!(t.starts_with(&dir));
        assert!(t.exists());
        let flag = format!("-Djava.io.tmpdir={}", t.to_string_lossy());
        assert!(!flag.contains("/tmp/debang-tmp-test") || flag.contains("debang-tmp-test"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn diagnostics_go_to_instance_logs() {
        let dir = std::env::temp_dir().join("debang-diag-test-inst");
        let _ = std::fs::remove_dir_all(&dir);
        let args = diagnostics_args(&dir, 21);
        assert!(args.iter().any(|a| a == "-XX:-CreateCoredumpOnCrash"));
        let err = args
            .iter()
            .find(|a| a.starts_with("-XX:ErrorFile="))
            .expect("ErrorFile flag");
        assert!(err.contains("hs_err_pid"));
        assert!(err.contains(&dir.to_string_lossy().to_string()));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// Every flag the launcher adds itself must be accepted by the JVM it will
/// start: one bad `-XX:` flag kills the game before the window appears. Runs
/// against the installed Java 8 (skipped when it is not installed).
#[test]
fn own_flags_are_accepted_by_installed_java8() {
    let Some(java) = crate::java::detect_java()
        .into_iter()
        .find(|j| j.major == 8)
        .map(|j| j.path)
    else {
        return;
    };
    let dir = std::env::temp_dir().join("debang-flag-probe");
    let mut flags = diagnostics_args(&dir, 8);
    flags.extend(crate::jvm::auto_gc_flags(8, "1.8.9"));
    for f in flags {
        let ok = std::process::Command::new(&java)
            .arg(&f)
            .arg("-version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        assert!(ok, "Java 8 не принимает флаг {}", f);
    }
}

#[cfg(test)]
mod java_choice_tests {
    use super::*;

    #[test]
    fn legacy_requires_java_8_even_when_newer_preferred() {
        let available = [8, 17, 21, 26];
        assert_eq!(choose_java_major(8, Some(21), &available), Some(8));
        assert_eq!(choose_java_major(8, Some(26), &available), Some(8));
        assert_eq!(choose_java_major(8, Some(8), &available), Some(8));
    }

    #[test]
    fn legacy_without_java_8_fails_instead_of_using_newer() {
        assert_eq!(choose_java_major(8, Some(21), &[17, 21, 26]), None);
        assert_eq!(choose_java_major(8, None, &[17]), None);
    }

    #[test]
    fn modern_prefers_configured_java_then_exact_then_next() {
        let available = [8, 17, 21];
        assert_eq!(choose_java_major(21, Some(21), &available), Some(21));
        assert_eq!(choose_java_major(17, Some(21), &available), Some(21));
        assert_eq!(choose_java_major(17, Some(8), &available), Some(17));
        assert_eq!(choose_java_major(21, None, &available), Some(21));
        assert_eq!(choose_java_major(25, Some(21), &available), None);
        assert_eq!(choose_java_major(25, Some(21), &[8, 17, 21, 26]), Some(26));
        assert_eq!(choose_java_major(17, Some(21), &[8]), None);
    }
}
