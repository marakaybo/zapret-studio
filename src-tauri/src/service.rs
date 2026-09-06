use crate::sysutil::{out_text, run_hidden};
use serde::Serialize;
use std::path::Path;

pub const SERVICE_NAME: &str = "zapret";
pub const TASK_NAME: &str = "Zapret Studio Autostart";

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct ServiceState {
    pub installed: bool,
    pub running: bool,
    pub strategy: Option<String>,
}

pub fn status() -> ServiceState {
    let Ok(out) = run_hidden("sc", &["query", SERVICE_NAME]) else {
        return ServiceState::default();
    };
    let text = out_text(&out);
    let installed = out.status.success() && !text.contains("1060");
    if !installed {
        return ServiceState::default();
    }
    let running = text.contains("RUNNING") || text.contains("START_PENDING");
    let strategy = run_hidden(
        "reg",
        &[
            "query",
            r"HKLM\System\CurrentControlSet\Services\zapret",
            "/v",
            "zapret-discord-youtube",
        ],
    )
    .ok()
    .and_then(|o| {
        let t = out_text(&o);
        t.lines()
            .find(|l| l.contains("REG_SZ"))
            .and_then(|l| l.split("REG_SZ").nth(1))
            .map(|v| v.trim().to_string())
    })
    .filter(|s| !s.is_empty());

    ServiceState { installed, running, strategy }
}

/// Ставит стратегию системной службой — обход поднимается вместе с Windows,
/// даже если приложение не запущено.
pub fn install(root: &Path, name: &str, args: &[String]) -> Result<(), String> {
    let exe = root.join("bin").join("winws.exe");
    if !exe.is_file() {
        return Err(format!("не найден {}", exe.display()));
    }
    let _ = run_hidden("net", &["stop", SERVICE_NAME]);
    let _ = run_hidden("sc", &["delete", SERVICE_NAME]);
    let _ = run_hidden("taskkill", &["/F", "/IM", "winws.exe"]);
    let _ = run_hidden("netsh", &["interface", "tcp", "set", "global", "timestamps=enabled"]);

    let mut bin_path = format!("\"{}\"", exe.display());
    for a in args {
        bin_path.push(' ');
        if a.contains(' ') {
            bin_path.push_str(&format!("\"{a}\""));
        } else {
            bin_path.push_str(a);
        }
    }

    let out = run_hidden(
        "sc",
        &[
            "create",
            SERVICE_NAME,
            "binPath=",
            &bin_path,
            "DisplayName=",
            "zapret",
            "start=",
            "auto",
        ],
    )
    .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!("не удалось создать службу: {}", out_text(&out).trim()));
    }
    let _ = run_hidden("sc", &["description", SERVICE_NAME, "Zapret DPI bypass"]);

    let start = run_hidden("sc", &["start", SERVICE_NAME]).map_err(|e| e.to_string())?;
    if !start.status.success() {
        let msg = out_text(&start);
        let _ = run_hidden("sc", &["delete", SERVICE_NAME]);
        return Err(format!("служба создана, но не стартовала: {}", msg.trim()));
    }

    let _ = run_hidden(
        "reg",
        &[
            "add",
            r"HKLM\System\CurrentControlSet\Services\zapret",
            "/v",
            "zapret-discord-youtube",
            "/t",
            "REG_SZ",
            "/d",
            name,
            "/f",
        ],
    );
    Ok(())
}

pub fn remove() -> Result<(), String> {
    let _ = run_hidden("net", &["stop", SERVICE_NAME]);
    let out = run_hidden("sc", &["delete", SERVICE_NAME]).map_err(|e| e.to_string())?;
    let text = out_text(&out);
    if !out.status.success() && !text.contains("1060") {
        return Err(format!("не удалось удалить службу: {}", text.trim()));
    }
    let _ = run_hidden("taskkill", &["/F", "/IM", "winws.exe"]);
    Ok(())
}

pub fn stop_service() -> Result<(), String> {
    let out = run_hidden("net", &["stop", SERVICE_NAME]).map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(out_text(&out).trim().to_string());
    }
    Ok(())
}

pub fn start_service() -> Result<(), String> {
    let out = run_hidden("sc", &["start", SERVICE_NAME]).map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(out_text(&out).trim().to_string());
    }
    Ok(())
}

// --- автозапуск самого приложения (задача планировщика: без запроса UAC) ---

pub fn autostart_enabled() -> bool {
    run_hidden("schtasks", &["/Query", "/TN", TASK_NAME])
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn set_autostart(enable: bool) -> Result<(), String> {
    if !enable {
        let _ = run_hidden("schtasks", &["/Delete", "/TN", TASK_NAME, "/F"]);
        return Ok(());
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let tr = format!("\"{}\" --tray", exe.display());
    let out = run_hidden(
        "schtasks",
        &["/Create", "/TN", TASK_NAME, "/TR", &tr, "/SC", "ONLOGON", "/RL", "HIGHEST", "/F"],
    )
    .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!("планировщик отказал: {}", out_text(&out).trim()));
    }
    Ok(())
}
