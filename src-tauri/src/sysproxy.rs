//! Системный прокси Windows — то, через что трафик попадает в ByeDPI.
//!
//! ByeDPI ничего не перехватывает сам: он лишь ждёт подключений на
//! 127.0.0.1. Чтобы браузеры и Discord пошли через него, приложение
//! прописывает адрес прокси в настройки Internet Settings — те самые, что
//! лежат в «Параметры → Сеть и Интернет → Прокси-сервер».
//!
//! Формат `socks=socks5://host:port` выбран не случайно: Chromium (а значит
//! Chrome, Edge и Discord) без явной схемы считает такой прокси SOCKS4 и
//! теряет разрешение имён на стороне прокси. Firefox системные настройки
//! читает по умолчанию, так что отдельно его настраивать не нужно.
//!
//! Что бы ни случилось с приложением, пользователь не должен остаться с
//! прокси в никуда: прежнее состояние сохраняется в конфиг и восстанавливается
//! при остановке, выходе и следующем запуске.

use crate::sysutil::{out_text, run_hidden};
use serde::{Deserialize, Serialize};

const KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings";

/// Локальные адреса мимо прокси — иначе роутер и принтеры в сети
/// начнут ходить через ByeDPI без всякого смысла.
const BYPASS: &str = "localhost;127.*;10.*;172.16.*;172.17.*;172.18.*;172.19.*;172.20.*;172.21.*;172.22.*;172.23.*;172.24.*;172.25.*;172.26.*;172.27.*;172.28.*;172.29.*;172.30.*;172.31.*;192.168.*;<local>";

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProxyState {
    pub enabled: bool,
    pub server: String,
    pub bypass: String,
}

fn read_value(name: &str) -> Option<String> {
    let out = run_hidden("reg", &["query", KEY, "/v", name]).ok()?;
    if !out.status.success() {
        return None;
    }
    let text = out_text(&out);
    let line = text.lines().find(|l| l.contains(name) && l.contains("REG_"))?;
    // Формат строки: "    ProxyServer    REG_SZ    socks=socks5://127.0.0.1:1080"
    let mut parts = line.split_whitespace();
    parts.next()?;
    parts.next()?;
    let rest: Vec<&str> = parts.collect();
    Some(rest.join(" "))
}

pub fn read() -> ProxyState {
    ProxyState {
        enabled: read_value("ProxyEnable").map(|v| v.trim() != "0x0").unwrap_or(false),
        server: read_value("ProxyServer").unwrap_or_default(),
        bypass: read_value("ProxyOverride").unwrap_or_default(),
    }
}

fn set_string(name: &str, value: &str) -> Result<(), String> {
    if value.is_empty() {
        let _ = run_hidden("reg", &["delete", KEY, "/v", name, "/f"]);
        return Ok(());
    }
    let out = run_hidden("reg", &["add", KEY, "/v", name, "/t", "REG_SZ", "/d", value, "/f"])
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!("не удалось записать {name}: {}", out_text(&out).trim()));
    }
    Ok(())
}

fn set_enabled(on: bool) -> Result<(), String> {
    let out = run_hidden(
        "reg",
        &["add", KEY, "/v", "ProxyEnable", "/t", "REG_DWORD", "/d", if on { "1" } else { "0" }, "/f"],
    )
    .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!("не удалось переключить прокси: {}", out_text(&out).trim()));
    }
    Ok(())
}

/// Адрес, который мы прописываем в систему для нашего прокси.
pub fn socks_value(port: u16) -> String {
    format!("socks=socks5://127.0.0.1:{port}")
}

/// Похоже ли текущее значение на наше — чтобы не сохранить самих себя
/// в «прежнее состояние» при повторном включении.
pub fn is_ours(state: &ProxyState, port: u16) -> bool {
    state.enabled && state.server == socks_value(port)
}

/// Наш ли это адрес вообще, без привязки к порту: порт мог смениться в
/// настройках между запусками, но возвращать пользователю нужно всё равно не
/// наш прокси, а то, что стояло у него.
pub fn is_ours_any(state: &ProxyState) -> bool {
    state.server.starts_with("socks=socks5://127.0.0.1:")
}

/// Включает системный прокси на наш порт и возвращает то, что было до этого.
pub fn enable(port: u16) -> Result<ProxyState, String> {
    let before = read();
    set_string("ProxyServer", &socks_value(port))?;
    set_string("ProxyOverride", BYPASS)?;
    set_enabled(true)?;
    notify();
    Ok(before)
}

/// Возвращает настройки в исходное состояние. `saved` — то, что вернул `enable`.
pub fn restore(saved: Option<&ProxyState>) -> Result<(), String> {
    match saved {
        Some(s) if s.enabled => {
            set_string("ProxyServer", &s.server)?;
            set_string("ProxyOverride", &s.bypass)?;
            set_enabled(true)?;
        }
        // Прокси не было — выключаем и убираем свой адрес, чтобы в настройках
        // Windows не осталось следов приложения.
        _ => {
            set_enabled(false)?;
            set_string("ProxyServer", "")?;
            if saved.map(|s| s.bypass.is_empty()).unwrap_or(true) {
                set_string("ProxyOverride", "")?;
            } else {
                set_string("ProxyOverride", &saved.unwrap().bypass)?;
            }
        }
    }
    notify();
    Ok(())
}

// Chromium и Firefox следят за веткой реестра сами, а вот приложения на
// WinINET узнают о смене настроек только после этого уведомления.
#[link(name = "wininet")]
extern "system" {
    fn InternetSetOptionW(
        h: *mut std::ffi::c_void,
        option: u32,
        buffer: *mut std::ffi::c_void,
        length: u32,
    ) -> i32;
}

const INTERNET_OPTION_SETTINGS_CHANGED: u32 = 39;
const INTERNET_OPTION_REFRESH: u32 = 37;

fn notify() {
    unsafe {
        InternetSetOptionW(std::ptr::null_mut(), INTERNET_OPTION_SETTINGS_CHANGED, std::ptr::null_mut(), 0);
        InternetSetOptionW(std::ptr::null_mut(), INTERNET_OPTION_REFRESH, std::ptr::null_mut(), 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn socks_value_carries_explicit_scheme() {
        // Без явного socks5:// Chromium подставляет SOCKS4 — тогда имена сайтов
        // резолвятся до прокси и обход теряет смысл.
        assert_eq!(socks_value(1080), "socks=socks5://127.0.0.1:1080");
    }

    #[test]
    fn recognises_our_own_settings() {
        let ours = ProxyState { enabled: true, server: socks_value(1080), bypass: BYPASS.into() };
        assert!(is_ours(&ours, 1080));
        assert!(!is_ours(&ours, 1081));
        let foreign = ProxyState { enabled: true, server: "http=10.0.0.1:3128".into(), bypass: String::new() };
        assert!(!is_ours(&foreign, 1080));
        assert!(!is_ours(&ProxyState::default(), 1080));
    }

    /// Читает настоящий реестр: проверяем, что разбор строки reg query не врёт.
    #[test]
    fn reads_current_state_without_panicking() {
        let s = read();
        assert!(!s.server.contains("REG_SZ"), "в значение попал тип: {}", s.server);
    }
}
