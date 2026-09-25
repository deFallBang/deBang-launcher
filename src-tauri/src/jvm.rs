//! Automatic JVM tuning: GC flags, heap sizes and per-instance proxy
//! properties. Kept separate from `launch` so it can be unit tested and
//! previewed in the UI before the game is started.

use crate::instances::{InstanceConfig, ProxyConfig};

/// Generational ZGC — Java 21+ with modern Minecraft.
pub const ZGC_FLAGS: [&str; 2] = ["-XX:+UseZGC", "-XX:+ZGenerational"];

/// Conservative G1 tuning — Java 8/17 on legacy Minecraft (1.8.9 – 1.16.5).
pub const LEGACY_G1_FLAGS: [&str; 5] = [
    "-XX:+UseG1GC",
    "-XX:+ParallelRefProcEnabled",
    "-XX:+MaxGCPauseMillis=200",
    "-XX:+UnlockExperimentalVMOptions",
    "-XX:+DisableExplicitGC",
];

/// Plain G1 for modern Java on mid-range versions (1.17 – 1.20.4).
pub const MID_G1_FLAGS: [&str; 2] = ["-XX:+UseG1GC", "-XX:+ParallelRefProcEnabled"];

/// `(major, minor, patch)` for a Minecraft version. Snapshots and unknown
/// formats sort as the newest release (a large minor), so they get modern flags.
pub fn mc_version_tuple(v: &str) -> (u32, u32, u32) {
    let v = v.trim();
    let core = v.split(['-', ' ']).next().unwrap_or(v);
    let mut it = core.split('.');
    let major = it.next().and_then(|x| x.parse().ok()).unwrap_or(0);
    if major != 1 {
        // snapshots like 23w45a, betas, etc. — treat as "newest"
        return (1, 999, 0);
    }
    let minor = it.next().and_then(|x| x.parse().ok()).unwrap_or(999);
    let patch = it.next().and_then(|x| x.parse().ok()).unwrap_or(0);
    (major, minor, patch)
}

pub fn mc_at_least(v: &str, minor: u32, patch: u32) -> bool {
    mc_version_tuple(v) >= (1, minor, patch)
}

pub fn mc_between(v: &str, from: (u32, u32), to: (u32, u32)) -> bool {
    let t = mc_version_tuple(v);
    t >= (1, from.0, 0) && t <= (1, to.0, to.1)
}

/// GC flags for the given Java major and Minecraft version.
pub fn auto_gc_flags(java_major: u32, mc_version: &str) -> Vec<String> {
    if java_major >= 21 && mc_at_least(mc_version, 20, 5) {
        return ZGC_FLAGS.iter().map(|s| s.to_string()).collect();
    }
    if java_major >= 17 && mc_at_least(mc_version, 17, 0) && !mc_at_least(mc_version, 20, 5) {
        return MID_G1_FLAGS.iter().map(|s| s.to_string()).collect();
    }
    if mc_between(mc_version, (8, 9), (16, 5)) && matches!(java_major, 8 | 17 | 21..) {
        return LEGACY_G1_FLAGS.iter().map(|s| s.to_string()).collect();
    }
    Vec::new()
}

/// Does the user already pin a collector themselves?
pub fn has_explicit_gc(args: &[String]) -> bool {
    args.iter().any(|a| {
        a.starts_with("-XX:+UseG1GC")
            || a.starts_with("-XX:+UseZGC")
            || a.starts_with("-XX:+UseShenandoah")
            || a.starts_with("-XX:+UseParallelGC")
            || a.starts_with("-XX:+UseSerialGC")
            || a.starts_with("-XX:+UseEpsilonGC")
            || a.starts_with("-XX:+UseConcMarkSweepGC")
    })
}

/// Desired max heap in MB for a version, before the system-RAM cap.
pub fn desired_max_heap_mb(mc_version: &str) -> u32 {
    let t = mc_version_tuple(mc_version);
    if t >= (1, 20, 0) {
        8 * 1024 // 1.20+ — 8 GB
    } else if t >= (1, 12, 2) {
        6 * 1024 // 1.12.2 … 1.19 — 6 GB
    } else {
        3 * 1024 // ≤ 1.12.1 — 3 GB
    }
}

/// Heap plan for a launch: Xms/Xmx in MB, honouring the 70 % RAM ceiling.
pub fn auto_heap_mb(mc_version: &str, total_ram_mb: u64) -> (u32, u32) {
    let desired = desired_max_heap_mb(mc_version) as u64;
    let cap = ((total_ram_mb as f64) * 0.70) as u64;
    // never take more than 70 % of RAM, and leave at least 2 GB for the game to
    // be usable on tiny machines
    let mut max_mb = desired.min(cap);
    if total_ram_mb < 3 * 1024 {
        max_mb = total_ram_mb.saturating_sub(1024); // very small machine
    }
    max_mb = max_mb.max(1024);
    let max_mb = (max_mb / 256) * 256;
    // Xms ≈ a quarter of Xmx, at least 1 GB and at most 2 GB
    let mut min_mb = (max_mb / 4).clamp(1024, 2048);
    min_mb = (min_mb / 256) * 256;
    (min_mb as u32, max_mb as u32)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchPlan {
    pub java_major: u32,
    pub min_mem_mb: u32,
    pub max_mem_mb: u32,
    pub auto_gc_applied: bool,
    pub gc_flags: Vec<String>,
    pub proxy_args: Vec<String>,
    pub notes: Vec<String>,
}

/// Builds the JVM part of the command line. `user_args` are the flags coming
/// from the global settings (preset + custom); auto GC never overrides an
/// explicit collector chosen by the user.
pub fn build_plan(
    cfg: &InstanceConfig,
    java_major: u32,
    total_ram_mb: u64,
    user_args: &[String],
    user_min_mem: u32,
    user_max_mem: u32,
) -> LaunchPlan {
    let mut notes = Vec::new();
    let (min_mem_mb, max_mem_mb) = if cfg.auto_mem {
        let (mn, mx) = auto_heap_mb(&cfg.version, total_ram_mb);
        notes.push(format!(
            "авто-память: -Xms{}M -Xmx{}M (версия {}), 70% ОЗУ = {} МБ",
            mn,
            mx,
            cfg.version,
            ((total_ram_mb as f64) * 0.70) as u64
        ));
        (mn, mx)
    } else {
        (user_min_mem, user_max_mem)
    };

    let mut gc_flags = Vec::new();
    let mut auto_gc_applied = false;
    if cfg.auto_gc {
        if has_explicit_gc(user_args) {
            notes.push("умный GC: пропущен — в пользовательских флагах уже выбран сборщик".into());
        } else {
            gc_flags = auto_gc_flags(java_major, &cfg.version);
            if gc_flags.is_empty() {
                notes.push(format!(
                    "умный GC: нечего добавлять для Java {} + MC {}",
                    java_major, cfg.version
                ));
            } else {
                auto_gc_applied = true;
                notes.push(format!(
                    "умный GC: {} для Java {} + MC {}",
                    gc_flags.join(" "),
                    java_major,
                    cfg.version
                ));
            }
        }
    }

    let proxy_args = cfg.proxy.java_args();
    if !proxy_args.is_empty() {
        notes.push(format!("прокси: {}", cfg.proxy.describe()));
    }

    LaunchPlan {
        java_major,
        min_mem_mb,
        max_mem_mb,
        auto_gc_applied,
        gc_flags,
        proxy_args,
        notes,
    }
}

/// Hides secrets in the command line before it is written to the console.
pub fn redact(args: &[String]) -> Vec<String> {
    args.iter()
        .map(|a| {
            let lower = a.to_ascii_lowercase();
            if (lower.contains("password") || lower.contains("socks.username")) && a.contains('=') {
                let key = a.split('=').next().unwrap_or("");
                format!("{}=***", key)
            } else {
                a.clone()
            }
        })
        .collect()
}

pub fn proxy_of(cfg: &InstanceConfig) -> &ProxyConfig {
    &cfg.proxy
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instances::ProxyConfig;

    fn cfg(version: &str) -> InstanceConfig {
        InstanceConfig {
            id: "t".into(),
            name: "t".into(),
            version: version.into(),
            loader: "Vanilla".into(),
            created: String::new(),
            uuid: String::new(),
            proxy: ProxyConfig::default(),
            auto_gc: true,
            auto_mem: false,
        }
    }

    #[test]
    fn version_tuples() {
        assert_eq!(mc_version_tuple("1.8.9"), (1, 8, 9));
        assert_eq!(mc_version_tuple("1.21.1"), (1, 21, 1));
        assert_eq!(mc_version_tuple("1.21.1-forge"), (1, 21, 1));
        assert_eq!(mc_version_tuple("23w45a"), (1, 999, 0));
        assert!(mc_at_least("1.20.5", 20, 5));
        assert!(!mc_at_least("1.20.4", 20, 5));
    }

    #[test]
    fn gc_selection() {
        assert_eq!(
            auto_gc_flags(21, "1.21.1"),
            vec!["-XX:+UseZGC", "-XX:+ZGenerational"]
        );
        assert_eq!(auto_gc_flags(25, "1.20.4"), vec!["-XX:+UseG1GC", "-XX:+ParallelRefProcEnabled"]);
        assert!(auto_gc_flags(8, "1.12.2").contains(&"-XX:+UseG1GC".to_string()));
        assert!(auto_gc_flags(17, "1.8.9").contains(&"-XX:+UseG1GC".to_string()));
        assert!(auto_gc_flags(8, "1.16.5").contains(&"-XX:+MaxGCPauseMillis=200".to_string()));
        // Java 8 cannot run modern Minecraft — no modern flags
        assert!(auto_gc_flags(8, "1.21.1").is_empty());
    }

    #[test]
    fn heap_bands_and_cap() {
        assert_eq!(desired_max_heap_mb("1.8.9"), 3 * 1024);
        assert_eq!(desired_max_heap_mb("1.12.2"), 6 * 1024);
        assert_eq!(desired_max_heap_mb("1.21.1"), 8 * 1024);
        // 32 GB machine: 8 GB wanted, 70 % = 22.4 GB -> 8 GB
        assert_eq!(auto_heap_mb("1.21.1", 32 * 1024).1, 8 * 1024);
        // 8 GB machine: 70 % = 5.6 GB -> 5.5 GB (256 MB step)
        assert_eq!(auto_heap_mb("1.21.1", 8 * 1024).1, 5 * 1024 + 512);
        // 4 GB machine: 70 % = 2.8 GB -> 2.75 GB (256 MB step)
        assert_eq!(auto_heap_mb("1.21.1", 4 * 1024).1, 2 * 1024 + 768);
        // tiny machine: never above 70 % and never below 1 GB
        let (mn, mx) = auto_heap_mb("1.8.9", 2 * 1024);
        assert_eq!((mn, mx), (1024, 1024));
        let (mn, mx) = auto_heap_mb("1.8.9", 32 * 1024);
        assert!(mn <= mx && mn >= 1024);
        // Xms never exceeds 2 GB even for a huge Xmx
        assert!(auto_heap_mb("1.21.1", 64 * 1024).0 <= 2048);
    }

    #[test]
    fn explicit_gc_is_respected() {
        let c = cfg("1.21.1");
        let plan = build_plan(&c, 21, 32 * 1024, &["-XX:+UseShenandoahGC".into()], 2048, 4096);
        assert!(!plan.auto_gc_applied);
        assert!(plan.gc_flags.is_empty());
    }

    #[test]
    fn proxy_args_and_redaction() {
        let mut c = cfg("1.21.1");
        c.proxy = ProxyConfig {
            kind: "Socks5".into(),
            host: "127.0.0.1".into(),
            port: 1080,
            login: "user".into(),
            password: "hunter2".into(),
        };
        let args = c.proxy.java_args();
        assert!(args.contains(&"-DsocksProxyHost=127.0.0.1".to_string()));
        assert!(args.contains(&"-DsocksProxyPort=1080".to_string()));
        assert!(args.iter().any(|a| a.contains("socks.username=user")));
        let safe = redact(&args);
        assert!(safe.iter().all(|a| !a.contains("hunter2")));
        assert!(safe.iter().any(|a| a.ends_with("=***")));

        c.proxy.kind = "Http".into();
        let h = c.proxy.java_args();
        assert!(h.contains(&"-Dhttp.proxyHost=127.0.0.1".to_string()));
        assert!(h.contains(&"-Dhttps.proxyPort=1080".to_string()));

        c.proxy.kind = "None".into();
        assert!(c.proxy.java_args().is_empty());
    }

    #[test]
    fn legacy_config_deserialises() {
        let old = r#"{"id":"a","name":"a","version":"1.20.1","loader":"Fabric","created":"x"}"#;
        let cfg: InstanceConfig = serde_json::from_str(old).expect("old config must load");
        assert_eq!(cfg.proxy.kind, "None");
        assert!(cfg.auto_gc, "auto GC defaults to on for old configs");
        assert!(!cfg.auto_mem, "auto memory defaults to off for old configs");
    }
}
