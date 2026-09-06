//! Cloudflare WARP — не обход блокировок, а туннель, но соседствует с ними
//! теснее всех остальных.
//!
//! Во-первых, его собственные серверы в России часто недоступны: официальное
//! приложение не подключается, пока не включишь обход, — тогда Cloudflare
//! «чинится» и WARP встаёт. Поэтому его хосты вынесены в отдельную группу
//! проверки: видно, помог обход или нет.
//!
//! Во-вторых, включённый WARP уводит в туннель весь трафик, включая наши
//! проверки. Тогда любая стратегия покажет отличный результат — мерился бы
//! туннель, а не обход. Об этом приложение обязано предупреждать.

use crate::sysutil::{out_text, run_hidden};
use serde::Serialize;
use std::path::PathBuf;

const DEFAULT_PATH: &str = r"C:\Program Files\Cloudflare\Cloudflare WARP\warp-cli.exe";

/// Откуда качать сам WARP. Приложение его не ставит: это чужая программа
/// со своим установщиком и своей службой.
pub const DOWNLOAD_URL: &str = "https://one.one.one.one/";

/// Что Cloudflare отвечает о самом себе. По этой странице видно то, чего не
/// видно из warp-cli: идёт ли трафик через туннель на самом деле.
const TRACE_URL: &str = "https://www.cloudflare.com/cdn-cgi/trace";

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct WarpState {
    pub installed: bool,
    pub connected: bool,
    /// Как есть из warp-cli: «Disconnected», «Connected», причина отключения
    pub detail: String,
    /// Режим из настроек: WarpWithDnsOverHttps, DoH и так далее
    pub mode: Option<String>,
    /// Куда отправить за установкой, если программы нет
    pub install_url: &'static str,
}

/// Путь к warp-cli: сначала штатное место установки, потом PATH.
pub fn cli() -> Option<PathBuf> {
    let standard = PathBuf::from(DEFAULT_PATH);
    if standard.is_file() {
        return Some(standard);
    }
    run_hidden("where", &["warp-cli"])
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| out_text(&o).lines().next().map(|l| PathBuf::from(l.trim())))
        .filter(|p| p.is_file())
}

fn run(args: &[&str]) -> Result<String, String> {
    let exe = cli().ok_or("Cloudflare WARP не установлен")?;
    let out = run_hidden(&exe.display().to_string(), args)
        .map_err(|e| format!("не запускается warp-cli: {e}"))?;
    let text = out_text(&out).trim().to_string();
    if out.status.success() {
        Ok(text)
    } else {
        Err(if text.is_empty() { "warp-cli отказал".into() } else { text })
    }
}

pub fn status() -> WarpState {
    // Программы нет — но карточку в интерфейсе всё равно показываем, иначе
    // «а где у меня WARP?» превращается в загадку без ответа
    let Some(_) = cli() else {
        return WarpState {
            detail: "Cloudflare WARP не установлен".into(),
            install_url: DOWNLOAD_URL,
            ..Default::default()
        };
    };
    let text = run(&["status"]).unwrap_or_default();
    // «Status update: Connected» / «Status update: Disconnected»
    let detail = text
        .lines()
        .find_map(|l| l.split("Status update:").nth(1))
        .map(|v| v.trim().to_string())
        .unwrap_or_else(|| text.lines().next().unwrap_or("состояние неизвестно").trim().to_string());
    let reason = text.lines().find_map(|l| l.split("Reason:").nth(1)).map(|v| v.trim().to_string());

    WarpState {
        installed: true,
        connected: detail.eq_ignore_ascii_case("Connected"),
        detail: match reason {
            Some(r) if !r.is_empty() && !detail.eq_ignore_ascii_case("Connected") => {
                format!("{detail} · {r}")
            }
            _ => detail,
        },
        mode: run(&["settings"]).ok().and_then(|s| {
            s.lines()
                .find(|l| l.contains("Mode:"))
                .and_then(|l| l.split("Mode:").nth(1))
                .map(|v| v.trim().to_string())
        }),
        install_url: DOWNLOAD_URL,
    }
}

pub fn connect() -> Result<String, String> {
    run(&["connect"])?;
    Ok("WARP подключается — на это уходит пара секунд".into())
}

pub fn disconnect() -> Result<String, String> {
    run(&["disconnect"])?;
    Ok("WARP отключён".into())
}

// ------------------------------------------------------- проверка туннеля

/// Ответ Cloudflare о нашем же соединении. `warp-cli status` говорит только
/// то, что думает о себе служба; здесь видно, что получилось на самом деле.
#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct WarpProbe {
    /// warp=on или warp=plus — трафик правда идёт через туннель
    pub active: bool,
    /// как есть из ответа: on | off | plus
    pub value: String,
    /// адрес, которым нас видит Cloudflare
    pub ip: String,
    /// страна выхода: RU, DE, NL…
    pub loc: String,
    /// ближайший дата-центр Cloudflare: три буквы вроде «DME»
    pub colo: String,
    pub checked_at: String,
    pub error: Option<String>,
}

/// Разбор ответа: это простые строки «ключ=значение».
fn read_trace(text: &str) -> WarpProbe {
    let field = |key: &str| -> String {
        text.lines()
            .find_map(|l| l.strip_prefix(&format!("{key}=")))
            .unwrap_or_default()
            .trim()
            .to_string()
    };
    let value = field("warp");
    WarpProbe {
        // «plus» — это WARP+, тот же туннель, только платный
        active: value == "on" || value == "plus",
        value,
        ip: field("ip"),
        loc: field("loc"),
        colo: field("colo"),
        checked_at: crate::sysutil::hhmmss(),
        error: None,
    }
}

/// Спрашивает Cloudflare, видит ли он нас из туннеля. Мимо системного прокси:
/// проверяем состояние машины, а не то, куда ходит один браузер.
pub async fn probe() -> WarpProbe {
    let client = match reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(8))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return WarpProbe { error: Some(e.to_string()), ..Default::default() };
        }
    };
    match client.get(TRACE_URL).send().await {
        Ok(r) if r.status().is_success() => match r.text().await {
            Ok(text) => read_trace(&text),
            Err(e) => WarpProbe {
                error: Some(format!("Cloudflare ответил, но ответ не прочитать: {e}")),
                checked_at: crate::sysutil::hhmmss(),
                ..Default::default()
            },
        },
        Ok(r) => WarpProbe {
            error: Some(format!("Cloudflare ответил {}", r.status())),
            checked_at: crate::sysutil::hhmmss(),
            ..Default::default()
        },
        // Сам факт недоступности Cloudflare — тоже ответ: в России его
        // часто режут, и именно поэтому WARP не подключается
        Err(_) => WarpProbe {
            error: Some(
                "Cloudflare не отвечает — похоже, провайдер его режет. Включи обход и проверь снова"
                    .into(),
            ),
            checked_at: crate::sysutil::hhmmss(),
            ..Default::default()
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Разбор вывода warp-cli проверяем на настоящей программе, если она есть.
    #[test]
    fn reads_status_without_guessing() {
        let s = status();
        if !s.installed {
            return;
        }
        assert!(!s.detail.is_empty(), "состояние должно быть описано словами");
        assert!(!s.detail.contains("Status update"), "в detail попал сырой префикс: {}", s.detail);
        // «Connected» и флаг должны согласовываться между собой
        assert_eq!(s.connected, s.detail.starts_with("Connected"), "{}", s.detail);
    }

    /// Ответ Cloudflare разбираем построчно — проверяем на настоящем виде.
    #[test]
    fn reads_warp_flag_from_trace() {
        let sample = "fl=123abc\nh=www.cloudflare.com\nip=104.28.1.1\nts=1700000000\nloc=NL\ncolo=AMS\nwarp=on\ngateway=off\n";
        let p = read_trace(sample);
        assert!(p.active);
        assert_eq!(p.value, "on");
        assert_eq!(p.ip, "104.28.1.1");
        assert_eq!(p.loc, "NL");
        assert_eq!(p.colo, "AMS");

        // WARP+ — тот же туннель
        assert!(read_trace("warp=plus\n").active);
        // а вот это уже мимо туннеля
        let off = read_trace("warp=off\nloc=RU\n");
        assert!(!off.active);
        assert_eq!(off.loc, "RU");
        // мусор не должен выглядеть как включённый WARP
        assert!(!read_trace("").active);
    }
}
