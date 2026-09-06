//! Профиль настроек — то, что имеет смысл передать другому человеку.
//!
//! Настроил обход, подобрал проверкой рабочую стратегию — и всё это остаётся
//! только у тебя. Друг, поставивший приложение, начинает с нуля: подбирает
//! заново то, что у вас, скорее всего, один и тот же провайдер уже подобрал.
//!
//! Что сюда НЕ попадает, и это главное:
//!
//! * ссылки на свои серверы — внутри них uuid и пароли, а профиль пересылают
//!   в мессенджере, где он остаётся навсегда;
//! * пути к папкам — на чужой машине они всё равно другие;
//! * версии установленных сборок и сохранённые системные настройки прокси —
//!   это состояние конкретного компьютера, а не выбор человека.
//!
//! Остаётся ровно выбор: чем обходить, каким набором параметров, на каком
//! порту и что проверять.

use crate::config::{AppConfig, CoreConfig};
use serde::{Deserialize, Serialize};

/// Номер формата. Пригодится, когда набор полей изменится: старый профиль
/// должен либо примениться, либо честно сказать, что он старый.
pub const VERSION: u32 = 1;

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct CorePart {
    pub preset: Option<String>,
    pub port: u16,
    pub system_proxy: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Profile {
    pub version: u32,
    /// Чем обходить: zapret | byedpi | goodbyedpi | xray | singbox
    pub engine: String,
    pub selected_strategy: Option<String>,
    pub game_filter: String,
    pub byedpi_preset: Option<String>,
    pub byedpi_port: u16,
    pub byedpi_system_proxy: bool,
    pub byedpi_custom: Vec<crate::preset::CustomPreset>,
    pub goodbye_preset: Option<String>,
    pub goodbye_custom: Vec<crate::preset::CustomPreset>,
    pub xray: CorePart,
    pub singbox: CorePart,
    pub custom_targets: Vec<String>,
    pub watchdog: bool,
    pub watchdog_interval_min: u32,
    pub watchdog_auto_fix: bool,
}

fn core_part(c: &CoreConfig, fallback_port: u16) -> CorePart {
    CorePart {
        preset: c.preset.clone(),
        port: c.port_or(fallback_port),
        system_proxy: c.system_proxy,
    }
}

pub fn export(cfg: &AppConfig) -> Profile {
    Profile {
        version: VERSION,
        engine: cfg.engine.clone(),
        selected_strategy: cfg.selected_strategy.clone(),
        game_filter: cfg.game_filter.clone(),
        byedpi_preset: cfg.byedpi_preset.clone(),
        byedpi_port: cfg.byedpi_port,
        byedpi_system_proxy: cfg.byedpi_system_proxy,
        byedpi_custom: cfg.byedpi_custom.clone(),
        goodbye_preset: cfg.goodbye_preset.clone(),
        goodbye_custom: cfg.goodbye_custom.clone(),
        xray: core_part(&cfg.xray, crate::config::XRAY_PORT),
        singbox: core_part(&cfg.singbox, crate::config::SINGBOX_PORT),
        custom_targets: cfg.custom_targets.clone(),
        watchdog: cfg.watchdog,
        watchdog_interval_min: cfg.watchdog_interval_min,
        watchdog_auto_fix: cfg.watchdog_auto_fix,
    }
}

pub fn to_text(cfg: &AppConfig) -> Result<String, String> {
    serde_json::to_string_pretty(&export(cfg)).map_err(|e| e.to_string())
}

pub fn parse(text: &str) -> Result<Profile, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("Профиль пустой — вставь то, что прислали".into());
    }
    let profile: Profile = serde_json::from_str(text)
        .map_err(|e| format!("Это не похоже на профиль настроек: {e}"))?;
    if profile.version == 0 || profile.engine.is_empty() {
        return Err("Это не профиль Zapret Studio — не хватает обязательных полей".into());
    }
    if profile.version > VERSION {
        return Err(format!(
            "Профиль от более новой версии приложения (формат {}, у нас {VERSION}) — обнови Zapret Studio",
            profile.version
        ));
    }
    Ok(profile)
}

/// Накладывает профиль на настройки. Всё, чего в профиле нет — пути, версии,
/// ссылки на серверы, — остаётся своим: чужой профиль не должен отобрать у
/// человека уже настроенное.
pub fn apply(p: &Profile, cfg: &mut AppConfig) -> Vec<String> {
    let mut changed = Vec::new();
    let known = ["zapret", "byedpi", "goodbyedpi", "xray", "singbox"];
    if known.contains(&p.engine.as_str()) && cfg.engine != p.engine {
        changed.push(format!("движок → {}", p.engine));
        cfg.engine = p.engine.clone();
    }
    if p.selected_strategy.is_some() && cfg.selected_strategy != p.selected_strategy {
        changed.push("стратегия zapret".into());
        cfg.selected_strategy = p.selected_strategy.clone();
    }
    if ["off", "all", "tcp", "udp"].contains(&p.game_filter.as_str()) {
        cfg.game_filter = p.game_filter.clone();
    }

    if p.byedpi_preset.is_some() {
        cfg.byedpi_preset = p.byedpi_preset.clone();
    }
    if p.byedpi_port >= 1024 {
        cfg.byedpi_port = p.byedpi_port;
    }
    cfg.byedpi_system_proxy = p.byedpi_system_proxy;
    if !p.byedpi_custom.is_empty() {
        changed.push(format!("своих пресетов ByeDPI: {}", p.byedpi_custom.len()));
        cfg.byedpi_custom = p.byedpi_custom.clone();
    }
    if p.goodbye_preset.is_some() {
        cfg.goodbye_preset = p.goodbye_preset.clone();
    }
    if !p.goodbye_custom.is_empty() {
        changed.push(format!("своих пресетов GoodbyeDPI: {}", p.goodbye_custom.len()));
        cfg.goodbye_custom = p.goodbye_custom.clone();
    }

    for (part, core) in [(&p.xray, &mut cfg.xray), (&p.singbox, &mut cfg.singbox)] {
        if part.preset.is_some() {
            core.preset = part.preset.clone();
        }
        if part.port >= 1024 {
            core.port = part.port;
        }
        core.system_proxy = part.system_proxy;
    }

    if !p.custom_targets.is_empty() {
        changed.push(format!("своих сайтов для проверки: {}", p.custom_targets.len()));
        cfg.custom_targets = p.custom_targets.clone();
    }
    cfg.watchdog = p.watchdog;
    if p.watchdog_interval_min > 0 {
        cfg.watchdog_interval_min = p.watchdog_interval_min;
    }
    cfg.watchdog_auto_fix = p.watchdog_auto_fix;
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Настройки, в которых есть что скрывать.
    fn secretive() -> AppConfig {
        let mut cfg = AppConfig::default();
        cfg.engine = "xray".into();
        cfg.xray.server =
            Some("vless://11111111-2222-3333-4444-555555555555@srv.example.com:443?pbk=SECRETKEY".into());
        cfg.singbox.server = Some("trojan://my-password@1.2.3.4:8443".into());
        cfg.zapret_dir = Some(std::path::PathBuf::from(r"C:\Users\Иван\zapret"));
        cfg.byedpi_dir = Some(std::path::PathBuf::from(r"C:\Users\Иван\byedpi"));
        cfg.installed_version = Some("1.10.2".into());
        cfg.saved_proxy = Some(crate::sysproxy::ProxyState {
            enabled: true,
            server: "corp-proxy.local:3128".into(),
            bypass: String::new(),
        });
        cfg.custom_targets = vec!["example.com".into()];
        cfg
    }

    /// Профиль пересылают в мессенджере. Ни одного секрета в нём быть не должно.
    #[test]
    fn never_leaks_secrets_or_local_paths() {
        let text = to_text(&secretive()).unwrap();
        for secret in [
            "vless://",
            "trojan://",
            "11111111",
            "SECRETKEY",
            "my-password",
            "srv.example.com",
            r"C:\Users",
            "Иван",
            "corp-proxy",
            "1.10.2",
        ] {
            assert!(!text.contains(secret), "в профиле утёк «{secret}»:\n{text}");
        }
        // А то, ради чего его и передают, — на месте
        assert!(text.contains("xray"), "{text}");
        assert!(text.contains("example.com"), "{text}");
    }

    /// Свой сервер и свои папки чужой профиль трогать не должен.
    #[test]
    fn keeps_what_belongs_to_this_machine() {
        let mut mine = secretive();
        let theirs = parse(&to_text(&{
            let mut c = AppConfig::default();
            c.engine = "byedpi".into();
            c.byedpi_port = 1099;
            c
        })
        .unwrap())
        .unwrap();

        apply(&theirs, &mut mine);
        assert_eq!(mine.engine, "byedpi");
        assert_eq!(mine.byedpi_port, 1099);
        // Ссылка на сервер и путь к папке остались своими
        assert!(mine.xray.server.as_deref().unwrap().contains("SECRETKEY"));
        assert!(mine.zapret_dir.is_some());
        assert_eq!(mine.installed_version.as_deref(), Some("1.10.2"));
    }

    #[test]
    fn refuses_junk_with_a_readable_reason() {
        for (text, expect) in [
            ("", "пустой"),
            ("   ", "пустой"),
            ("просто текст", "не похоже"),
            ("{}", "не профиль"),
            (r#"{"version":1}"#, "не профиль"),
            (r#"{"version":99,"engine":"zapret"}"#, "более новой"),
        ] {
            let err = parse(text).expect_err(text);
            assert!(err.to_lowercase().contains(expect), "для «{text}»: {err}");
        }
    }

    /// Профиль должен переживать поездку туда и обратно без потерь.
    #[test]
    fn survives_a_round_trip() {
        let mut cfg = AppConfig::default();
        cfg.engine = "singbox".into();
        cfg.singbox.preset = Some("frag-slow".into());
        cfg.singbox.port = 1090;
        cfg.custom_targets = vec!["rutracker.org".into(), "example.com:443".into()];
        cfg.watchdog_interval_min = 5;

        let restored = parse(&to_text(&cfg).unwrap()).unwrap();
        let mut fresh = AppConfig::default();
        apply(&restored, &mut fresh);

        assert_eq!(fresh.engine, "singbox");
        assert_eq!(fresh.singbox.preset.as_deref(), Some("frag-slow"));
        assert_eq!(fresh.singbox.port, 1090);
        assert_eq!(fresh.custom_targets.len(), 2);
        assert_eq!(fresh.watchdog_interval_min, 5);
    }
}
