use serde::Serialize;
use std::fs;
use std::path::PathBuf;

/// Memory information in megabytes, as the UI expects.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MemInfo {
    pub total_mb: u64,
    pub available_mb: u64,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SysInfo {
    pub session: String,
    pub os: String,
    pub kernel: String,
    pub arch: String,
    pub renderer: String,
    pub data_dir: String,
    pub launcher_version: String,
}

// ---------- per-OS memory ----------

#[cfg(target_os = "linux")]
fn mem_linux() -> Result<MemInfo, String> {
    let content = fs::read_to_string("/proc/meminfo").map_err(|e| e.to_string())?;
    let mut total = 0u64;
    let mut avail = 0u64;
    for line in content.lines() {
        let mut parts = line.split_whitespace();
        let key = parts.next().unwrap_or("");
        let value: u64 = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
        match key {
            "MemTotal:" => total = value,
            "MemAvailable:" => avail = value,
            _ => {}
        }
    }
    Ok(MemInfo {
        total_mb: total / 1024,
        available_mb: avail / 1024,
    })
}

#[cfg(windows)]
mod win {
    #[repr(C)]
    #[derive(Default)]
    pub struct MemoryStatusEx {
        pub length: u32,
        pub memory_load: u32,
        pub total_phys: u64,
        pub avail_phys: u64,
        pub total_page_file: u64,
        pub avail_page_file: u64,
        pub total_virtual: u64,
        pub avail_virtual: u64,
        pub avail_extended_virtual: u64,
    }

    #[repr(C)]
    #[derive(Default)]
    pub struct OsVersionInfoExW {
        pub os_version_info_size: u32,
        pub major_version: u32,
        pub minor_version: u32,
        pub build_number: u32,
        pub platform_id: u32,
        pub service_pack_major: u16,
        pub service_pack_minor: u16,
        pub suite_mask: u16,
        pub product_type: u8,
        pub reserved: u8,
    }

    #[link(name = "kernel32")]
    extern "system" {
        pub fn GlobalMemoryStatusEx(lpBuffer: *mut MemoryStatusEx) -> i32;
        pub fn GetVersionExW(lpVersionInformation: *mut OsVersionInfoExW) -> i32;
    }
}

#[cfg(windows)]
fn mem_windows() -> Result<MemInfo, String> {
    unsafe {
        let mut st = win::MemoryStatusEx {
            length: std::mem::size_of::<win::MemoryStatusEx>() as u32,
            ..Default::default()
        };
        if win::GlobalMemoryStatusEx(&mut st) == 0 {
            return Err("GlobalMemoryStatusEx failed".into());
        }
        Ok(MemInfo {
            total_mb: st.total_phys / 1024 / 1024,
            available_mb: st.avail_phys / 1024 / 1024,
        })
    }
}

#[cfg(windows)]
fn os_windows() -> (String, String) {
    unsafe {
        let mut v = win::OsVersionInfoExW {
            os_version_info_size: std::mem::size_of::<win::OsVersionInfoExW>() as u32,
            ..Default::default()
        };
        if win::GetVersionExW(&mut v) == 0 {
            return ("Windows".into(), std::env::consts::ARCH.into());
        }
        let name = if v.build_number >= 22000 {
            "Windows 11"
        } else if v.build_number >= 10240 {
            "Windows 10"
        } else {
            "Windows"
        };
        (
            name.to_string(),
            format!(
                "сборка {}.{}.{}",
                v.major_version, v.minor_version, v.build_number
            ),
        )
    }
}

#[cfg(target_os = "macos")]
mod mac {
    #[repr(C)]
    #[derive(Default)]
    pub struct SysctlValue {
        pub value: i64,
        pub length: std::ffi::c_ulong,
    }

    extern "C" {
        pub fn sysctlbyname(
            name: *const std::os::raw::c_char,
            oldp: *mut std::ffi::c_void,
            oldlenp: *mut std::ffi::c_ulong,
            newp: *mut std::ffi::c_void,
            newlen: std::ffi::c_ulong,
        ) -> i32;
    }

    pub fn u64_by_name(name: &str) -> Option<u64> {
        let c = std::ffi::CString::new(name).ok()?;
        let mut val: u64 = 0;
        let mut len = std::mem::size_of::<u64>() as std::ffi::c_ulong;
        let rc = unsafe {
            sysctlbyname(
                c.as_ptr(),
                &mut val as *mut u64 as *mut std::ffi::c_void,
                &mut len,
                std::ptr::null_mut(),
                0,
            )
        };
        if rc == 0 {
            Some(val)
        } else {
            None
        }
    }

    pub fn u32_by_name(name: &str) -> Option<u32> {
        let c = std::ffi::CString::new(name).ok()?;
        let mut val: u32 = 0;
        let mut len = std::mem::size_of::<u32>() as std::ffi::c_ulong;
        let rc = unsafe {
            sysctlbyname(
                c.as_ptr(),
                &mut val as *mut u32 as *mut std::ffi::c_void,
                &mut len,
                std::ptr::null_mut(),
                0,
            )
        };
        if rc == 0 {
            Some(val)
        } else {
            None
        }
    }
}

#[cfg(target_os = "macos")]
fn mem_macos() -> Result<MemInfo, String> {
    let total = mac::u64_by_name("hw.memsize").ok_or("hw.memsize unavailable")?;
    let page = mac::u64_by_name("hw.pagesize").unwrap_or(4096);
    let free = mac::u64_by_name("vm.page_free_count").unwrap_or(0);
    // macOS keeps most of the free memory in the inactive/purgeable lists
    let inactive = mac::u64_by_name("vm.page_inactive_count").unwrap_or(0);
    let speculative = mac::u64_by_name("vm.page_speculative_count").unwrap_or(0);
    let available = ((free + inactive + speculative) * page).min(total);
    Ok(MemInfo {
        total_mb: total / 1024 / 1024,
        available_mb: available / 1024 / 1024,
    })
}

#[tauri::command]
pub fn get_mem_info() -> Result<MemInfo, String> {
    #[cfg(target_os = "linux")]
    {
        mem_linux()
    }
    #[cfg(windows)]
    {
        mem_windows()
    }
    #[cfg(target_os = "macos")]
    {
        mem_macos()
    }
    #[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
    {
        Err("неизвестная платформа".into())
    }
}

#[tauri::command]
pub fn get_sys_info() -> SysInfo {
    let arch = std::env::consts::ARCH.to_string();
    let data_dir = crate::versions::data_root().to_string_lossy().to_string();
    let renderer = if cfg!(target_os = "linux") {
        "WebKitGTK 4.1"
    } else if cfg!(windows) {
        "WebView2"
    } else {
        "WKWebView"
    }
    .to_string();

    #[cfg(target_os = "linux")]
    let (os, kernel) = {
        let pretty = fs::read_to_string("/etc/os-release")
            .ok()
            .and_then(|s| {
                s.lines()
                    .find_map(|l| l.strip_prefix("PRETTY_NAME="))
                    .map(|v| v.trim_matches('"').to_string())
            })
            .unwrap_or_else(|| "Linux".to_string());
        let session = std::env::var("XDG_SESSION_TYPE")
            .or_else(|_| std::env::var("WAYLAND_DISPLAY").map(|_| "wayland".into()))
            .unwrap_or_else(|_| "x11".into());
        let kernel = std::process::Command::new("uname")
            .arg("-r")
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default();
        (format!("{} · {}", pretty, session), kernel)
    };

    #[cfg(windows)]
    let (os, kernel) = os_windows();

    #[cfg(target_os = "macos")]
    let (os, kernel) = {
        let ver = mac::u32_by_name("kern.osproductversion")
            .map(|v| v as f32 / 10000.0)
            .map(|v| format!("{:.2}", v))
            .unwrap_or_else(|| "?".into());
        (
            format!("macOS {}", ver),
            std::process::Command::new("uname")
                .arg("-r")
                .output()
                .ok()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_default(),
        )
    };

    #[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
    let (os, kernel) = (std::env::consts::OS.to_string(), String::new());

    let session = if cfg!(windows) {
        "windows".to_string()
    } else if cfg!(target_os = "macos") {
        "cocoa".to_string()
    } else {
        std::env::var("XDG_SESSION_TYPE")
            .or_else(|_| std::env::var("WAYLAND_DISPLAY").map(|_| "wayland".into()))
            .unwrap_or_else(|_| "x11".into())
    };

    SysInfo {
        session,
        os,
        kernel,
        arch,
        renderer,
        data_dir,
        launcher_version: env!("CARGO_PKG_VERSION").to_string(),
    }
}

/// Best-effort home directory on every platform (`HOME` is often unset on
/// Windows).
pub fn home_dir() -> PathBuf {
    #[cfg(windows)]
    {
        for k in ["USERPROFILE", "HOME"] {
            if let Ok(v) = std::env::var(k) {
                if !v.is_empty() {
                    return PathBuf::from(v);
                }
            }
        }
        PathBuf::from("C:\\")
    }
    #[cfg(not(windows))]
    {
        std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/tmp"))
    }
}
