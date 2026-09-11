use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct AppConfig {
    /// Папка со сборкой zapret (внутри должны быть bin/winws.exe и *.bat)
    pub zapret_dir: Option<PathBuf>,
    /// true — папкой владеет приложение (скачало само), можно обновлять
    pub managed: bool,
    pub installed_version: Option<String>,
    pub selected_strategy: Option<String>,
    pub app_autostart: bool,
    pub start_minimized: bool,
    pub auto_check_updates: bool,
    pub autostart_bypass: bool,
    /// off | all | tcp | udp
    pub game_filter: String,
    pub onboarded: bool,
    pub best_strategy: Option<String>,
    pub last_test_at: Option<String>,
    /// Ставить обновления zapret без вопроса, как только они выходят
    pub auto_install_updates: bool,
    /// Как часто фоновая проверка ходит на GitHub
    pub update_interval_hours: u32,
    pub last_update_check: Option<String>,

    // --- ByeDPI: второй движок обхода, локальный SOCKS5-прокси ---
    /// zapret | byedpi — чем сейчас обходим блокировки
    pub engine: String,
    /// Папка с ciadpi.exe
    pub byedpi_dir: Option<PathBuf>,
    /// true — папку скачало приложение, можно обновлять
    pub byedpi_managed: bool,
    pub byedpi_version: Option<String>,
    /// id выбранного пресета
    pub byedpi_preset: Option<String>,
    pub byedpi_port: u16,
    /// Прописывать системный прокси Windows при запуске ByeDPI
    pub byedpi_system_proxy: bool,
    /// Поднимать прокси сразу при старте приложения
    pub byedpi_autostart: bool,
    pub byedpi_custom: Vec<crate::byedpi::CustomPreset>,
    pub byedpi_best: Option<String>,
    pub byedpi_last_test_at: Option<String>,
    /// Настройки прокси, которые были у пользователя до нас, — чтобы вернуть
    /// их даже после аварийного завершения приложения
    pub saved_proxy: Option<crate::sysproxy::ProxyState>,
    /// Настройки DNS, которые были у пользователя до нас. В отличие от
    /// прокси, при выходе они не возвращаются: прокси в никуда оставляет
    /// человека без интернета, а выбранный резолвер работает сам по себе
    /// и держится до тех пор, пока его не выключат.
    pub saved_dns: Option<crate::dns::SavedDns>,

    // --- GoodbyeDPI: третий движок, тоже на драйвере WinDivert ---
    pub goodbye_dir: Option<PathBuf>,
    pub goodbye_managed: bool,
    pub goodbye_version: Option<String>,
    pub goodbye_preset: Option<String>,
    /// Поднимать обход сразу при старте приложения
    pub goodbye_autostart: bool,
    pub goodbye_custom: Vec<crate::preset::CustomPreset>,
    pub goodbye_best: Option<String>,
    pub goodbye_last_test_at: Option<String>,

    // --- Прокси-ядра: Xray и sing-box. Набор настроек у них общий ---
    pub xray: CoreConfig,
    pub singbox: CoreConfig,

    /// Свои сайты для проверки — как их ввёл пользователь. К ним ещё
    /// добавляется `utils/targets.txt` из папки zapret, если он есть.
    pub custom_targets: Vec<String>,

    /// Откуда приложение берёт обновления самому себе, `владелец/репозиторий`.
    /// Пусто — берём зашитый по умолчанию.
    pub app_repo: Option<String>,

    // --- Сторож: тихо проверяет, что обход ещё пробивает ---
    pub watchdog: bool,
    /// Как часто осматривать. Реже — меньше запросов, позже узнаешь
    pub watchdog_interval_min: u32,
    /// Пытаться починить самому: перезапустить обход, а если не помогло —
    /// переключиться на лучшую по последней проверке
    pub watchdog_auto_fix: bool,
}

/// Порты по умолчанию у каждого локального прокси свои: иначе, переключаясь
/// между движками, пользователь ловил бы «порт занят» на ровном месте.
pub const XRAY_PORT: u16 = 1081;
pub const SINGBOX_PORT: u16 = 1082;

/// Настройки прокси-ядра. У Xray и sing-box они совпадают до последнего поля,
/// поэтому живут отдельной структурой, а не двумя десятками плоских ключей.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct CoreConfig {
    /// Папка с ядром
    pub dir: Option<PathBuf>,
    /// true — папку скачало приложение, можно обновлять
    pub managed: bool,
    pub version: Option<String>,
    /// id выбранного пресета
    pub preset: Option<String>,
    pub port: u16,
    /// Прописывать системный прокси Windows при запуске
    pub system_proxy: bool,
    /// Поднимать ядро сразу при старте приложения
    pub autostart: bool,
    /// Ссылка на свой сервер — как её вставил пользователь, целиком.
    /// Внутри лежит его uuid или пароль, поэтому наружу отдаётся только
    /// короткая сводка (см. `link::ServerLink::summary`).
    pub server: Option<String>,
    /// Все известные серверы: из подписки и добавленные руками. Хранятся
    /// ссылками целиком, поэтому наружу отдаются только сводками
    pub servers: Vec<String>,
    /// Ссылка на подписку, если серверы пришли оттуда
    pub subscription: Option<String>,
    pub best: Option<String>,
    pub last_test_at: Option<String>,
}

impl Default for CoreConfig {
    fn default() -> Self {
        Self {
            dir: None,
            managed: false,
            version: None,
            preset: None,
            // 0 означает «порт не выбирали» — подставим свой для каждого ядра
            port: 0,
            system_proxy: true,
            autostart: false,
            server: None,
            servers: Vec::new(),
            subscription: None,
            best: None,
            last_test_at: None,
        }
    }
}

impl CoreConfig {
    /// Порт с запасным значением: в старых конфигах его нет вовсе, а порты
    /// ниже 1024 занимает система.
    pub fn port_or(&self, fallback: u16) -> u16 {
        if self.port >= 1024 { self.port } else { fallback }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            zapret_dir: None,
            managed: false,
            installed_version: None,
            selected_strategy: None,
            app_autostart: false,
            start_minimized: false,
            auto_check_updates: true,
            autostart_bypass: false,
            game_filter: "off".into(),
            onboarded: false,
            best_strategy: None,
            last_test_at: None,
            auto_install_updates: false,
            update_interval_hours: 6,
            last_update_check: None,
            engine: "zapret".into(),
            byedpi_dir: None,
            byedpi_managed: false,
            byedpi_version: None,
            byedpi_preset: None,
            byedpi_port: 1080,
            byedpi_system_proxy: true,
            byedpi_autostart: false,
            byedpi_custom: Vec::new(),
            byedpi_best: None,
            byedpi_last_test_at: None,
            saved_proxy: None,
            saved_dns: None,
            goodbye_dir: None,
            goodbye_managed: false,
            goodbye_version: None,
            goodbye_preset: None,
            goodbye_autostart: false,
            goodbye_custom: Vec::new(),
            goodbye_best: None,
            goodbye_last_test_at: None,
            xray: CoreConfig { port: XRAY_PORT, ..Default::default() },
            singbox: CoreConfig { port: SINGBOX_PORT, ..Default::default() },
            custom_targets: Vec::new(),
            app_repo: None,
            watchdog: true,
            watchdog_interval_min: 15,
            watchdog_auto_fix: false,
        }
    }
}

pub fn load(path: &Path) -> AppConfig {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(path: &Path, cfg: &AppConfig) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let data = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    std::fs::write(path, data).map_err(|e| e.to_string())
}
