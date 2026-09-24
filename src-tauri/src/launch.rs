use crate::instances::instance_dir;
use crate::java;
use crate::instances::pseudo_uuid;
use crate::LauncherState;
use serde::{Deserialize, Serialize};
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

pub(crate) fn emit(app: &AppHandle, line: String, stream: &str) {
    let _ = app.emit(
        "game://log",
        LogEvent {
            time: now_ms(),
            line,
            stream: stream.to_string(),
        },
    );
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

fn pick_java(preferred: &str, major: u32) -> Result<String, String> {
    let installs = java::detect_java();
    if !preferred.is_empty() {
        if let Some(j) = installs.iter().find(|j| j.path == preferred) {
            if j.major >= major {
                return Ok(preferred.to_string());
            }
        }
    }
    if let Some(j) = installs.iter().find(|j| j.major == major) {
        return Ok(j.path.clone());
    }
    if let Some(j) = installs.iter().filter(|j| j.major > major).min_by_key(|j| j.major) {
        return Ok(j.path.clone());
    }
    Err(format!(
        "Не найдена Java {} (нужно ≥ {}). Установите: sudo pacman -S --needed jre-openjdk",
        major, major
    ))
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
        .map_err(|_| "Инстанс не найден".to_string())?;
    let mut cfg: crate::instances::InstanceConfig =
        serde_json::from_str(&cfg_raw).map_err(|e| e.to_string())?;
    if cfg.uuid.is_empty() {
        cfg.uuid = pseudo_uuid(&instance_id);
    }

    let run_sh = dir.join("run.sh");
    let manual_jar = ["minecraft.jar", "server.jar"]
        .into_iter()
        .map(|j| dir.join(j))
        .find(|p| p.exists());

    let jvm_user: Vec<String> = settings
        .jvm_args
        .iter()
        .flat_map(|a| a.split_whitespace().map(String::from))
        .collect();

    let (program, args) = if let Some(jar) = manual_jar {
        let mut a = vec![
            format!("-Xms{}M", settings.min_mem_mb),
            format!("-Xmx{}M", settings.max_mem_mb),
        ];
        a.extend(jvm_user);
        a.push("-jar".into());
        a.push(jar.to_string_lossy().to_string());
        (settings.java_path.clone(), a)
    } else if run_sh.exists() {
        (
            "/bin/bash".into(),
            vec![run_sh.to_string_lossy().to_string()],
        )
    } else if cfg!(windows) {
        let batch = ["run.bat", "run.cmd"]
            .into_iter()
            .map(|b| dir.join(b))
            .find(|p| p.exists());
        if let Some(batch) = batch {
            (
                "cmd".into(),
                vec![
                    "/C".into(),
                    batch.to_string_lossy().to_string(),
                ],
            )
        } else {
            return Err("В инстансе нет ни run.sh, ни run.bat/cmd — импортируйте run-скрипт".into());
        }
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
        let prepared = crate::versions::prepare(
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
        )
        .await;
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
        let mut a = vec![
            format!("-Xms{}M", settings.min_mem_mb),
            format!("-Xmx{}M", settings.max_mem_mb),
        ];
        a.extend(jvm_user);
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
        emit(&app, format!("$ {} {}", java, shown.join(" ")), "launcher");
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
    state.child.lock().map(|g| g.is_some()).unwrap_or(false)
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
