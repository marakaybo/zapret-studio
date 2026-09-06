mod byedpi;
mod config;
mod diag;
mod goodbye;
mod link;
mod preset;
mod profile;
mod proxycore;
mod runner;
mod selfupdate;
mod service;
mod singbox;
mod strategies;
mod sysproxy;
mod sysutil;
mod tester;
mod tools;
mod updater;
mod warp;
mod xray;

use config::AppConfig;
use runner::{LogLine, Runner};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use strategies::Strategy;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State};

/// Движки, между которыми переключается большая кнопка. Порядок — тот же,
/// что в интерфейсе: сначала те, что правят пакеты, потом локальные прокси.
const ENGINES: [&str; 5] = ["zapret", "byedpi", "goodbyedpi", "xray", "singbox"];
/// Из них — прокси-ядра с конфигом вместо ключей запуска
const CORES: [&str; 2] = ["xray", "singbox"];

pub struct AppState {
    cfg: Mutex<AppConfig>,
    cfg_path: Mutex<PathBuf>,
    data_dir: Mutex<PathBuf>,
    runner: Arc<Runner>,
    proxy: Arc<byedpi::Proxy>,
    goodbye: Arc<goodbye::Engine>,
    xray: Arc<proxycore::Core>,
    singbox: Arc<proxycore::Core>,
    cancel: Arc<AtomicBool>,
    testing: Arc<AtomicBool>,
    /// Взведён — качающаяся установка должна свернуться на ближайшем шаге
    install_cancel: Arc<AtomicBool>,
    /// Сколько осмотров подряд сторож видел мёртвый обход. Нужен, чтобы не
    /// повторять одно и то же уведомление и чтобы чинить по нарастающей
    watchdog_fails: Arc<AtomicUsize>,
    /// Пока один движок поднимается или гасится, второй такой же запрос ждёт
    engine_lock: Mutex<()>,
}

impl AppState {
    fn config(&self) -> AppConfig {
        self.cfg.lock().unwrap().clone()
    }
    fn set_config(&self, next: AppConfig) -> Result<(), String> {
        let path = self.cfg_path.lock().unwrap().clone();
        config::save(&path, &next)?;
        *self.cfg.lock().unwrap() = next;
        Ok(())
    }
    /// Меняет настройки, не отпуская замок: прочитать, поправить, записать —
    /// одним действием.
    ///
    /// Обычная пара `config()` + `set_config()` оставляет между собой щель:
    /// кто-то другой успевает записать своё, а наша запись это затирает.
    /// Пользователь при этом ничего не делал неправильно — он просто щёлкнул
    /// переключатель в ту же секунду, когда фоновая проверка обновлений
    /// сохраняла отметку времени, и настройка молча не сохранилась.
    ///
    /// Поэтому так пишут все, кто работает в фоне: сторож, проверки
    /// обновлений, автоустановка, прогон стратегий. Команды из интерфейса
    /// человек запускает по одной, и им хватает обычной пары — а если такая
    /// команда всё же перебьёт фоновую отметку, потеряется лишь она,
    /// и следующий круг поставит её заново.
    ///
    /// Внутри замыкания трогать `state` нельзя: замок не рекурсивный.
    fn update_config<T>(&self, change: impl FnOnce(&mut AppConfig) -> T) -> Result<T, String> {
        let path = self.cfg_path.lock().unwrap().clone();
        let mut guard = self.cfg.lock().unwrap();
        let result = change(&mut guard);
        config::save(&path, &guard)?;
        Ok(result)
    }
    fn root(&self) -> Option<PathBuf> {
        self.cfg.lock().unwrap().zapret_dir.clone()
    }
    fn managed_dir(&self) -> PathBuf {
        self.data_dir.lock().unwrap().join("zapret")
    }
    fn managed_byedpi_dir(&self) -> PathBuf {
        self.data_dir.lock().unwrap().join("byedpi")
    }
    fn managed_goodbye_dir(&self) -> PathBuf {
        self.data_dir.lock().unwrap().join("goodbyedpi")
    }
    fn managed_core_dir(&self, engine: &str) -> PathBuf {
        self.data_dir.lock().unwrap().join(engine)
    }
    /// Пускает к движкам по одному. Команды выполняются в пуле потоков и
    /// могут прийти одновременно: человек жмёт «выключить», а сторож в этот
    /// же миг чинит обход — и всё заканчивается тем, что обход работает
    /// вопреки нажатой кнопке. Замок берут только команды снаружи; внутренние
    /// помощники его не трогают, иначе получилось бы самоблокирование.
    ///
    /// Отравленный замок разворачиваем: паника в одной команде не повод
    /// навсегда лишить человека управления обходом.
    fn engine_guard(&self) -> std::sync::MutexGuard<'_, ()> {
        self.engine_lock.lock().unwrap_or_else(|e| e.into_inner())
    }
    /// Ядро по имени движка. Их всего два, но обращений к ним много —
    /// без этого каждое было бы отдельным match.
    fn core(&self, engine: &str) -> Option<&Arc<proxycore::Core>> {
        match engine {
            "xray" => Some(&self.xray),
            "singbox" => Some(&self.singbox),
            _ => None,
        }
    }
    /// Процессы, которые запустили мы сами: диагностика не должна принимать
    /// их за чужой обход и предлагать остановить.
    fn own_processes(&self) -> Vec<&'static str> {
        let mut own = Vec::new();
        if self.proxy.is_running() {
            own.push(byedpi::EXE);
        }
        if self.goodbye.is_running() {
            own.push(goodbye::EXE);
        }
        if self.xray.is_running() {
            own.push(xray::EXE);
        }
        if self.singbox.is_running() {
            own.push(singbox::EXE);
        }
        own
    }
    /// Порт работающего локального прокси — нужен там, где важно отличить наш
    /// прокси от чужого: в диагностике, при замерах и при возврате системных
    /// настроек. Одновременно работает только один движок, так что первый
    /// найденный он и есть.
    fn own_proxy_port(&self) -> Option<u16> {
        if self.proxy.is_running() {
            return Some(*self.proxy.port.lock().unwrap());
        }
        [&self.xray, &self.singbox]
            .into_iter()
            .find(|c| c.is_running())
            .map(|c| *c.port.lock().unwrap())
    }
    /// Работает ли сейчас выбранный движок — без сборки полного снимка.
    fn engine_running(&self) -> bool {
        match self.config().engine.as_str() {
            "byedpi" => self.proxy.is_running(),
            "goodbyedpi" => self.goodbye.is_running(),
            engine if self.core(engine).is_some() => self.core(engine).unwrap().is_running(),
            _ => self.runner.is_running() || service::status().running,
        }
    }
}

/// Состояние GoodbyeDPI — по составу почти как у ByeDPI, только вместо порта
/// и системного прокси список заблокированных доменов.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoodbyeState {
    installed: bool,
    dir: Option<String>,
    managed: bool,
    managed_dir: String,
    version: Option<String>,
    running: bool,
    current: Option<String>,
    autostart: bool,
    presets: Vec<preset::Preset>,
    selected: Option<String>,
    best: Option<String>,
    last_test_at: Option<String>,
    /// Сколько доменов в russia-blacklist.txt — None, если файла нет
    blacklist: Option<usize>,
}

/// Всё про второй движок одним куском — интерфейсу так удобнее.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ByeState {
    installed: bool,
    dir: Option<String>,
    managed: bool,
    managed_dir: String,
    version: Option<String>,
    running: bool,
    current: Option<String>,
    port: u16,
    system_proxy: bool,
    /// Системный прокси прямо сейчас направлен в наш ByeDPI
    system_proxy_active: bool,
    autostart: bool,
    presets: Vec<byedpi::Preset>,
    selected: Option<String>,
    best: Option<String>,
    last_test_at: Option<String>,
}

/// Состояние прокси-ядра — Xray или sing-box. Снаружи оно почти как ByeDPI
/// (локальный прокси, порт, системные настройки), но добавлен свой сервер:
/// пресеты «через сервер» без него не работают.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreState {
    installed: bool,
    dir: Option<String>,
    managed: bool,
    managed_dir: String,
    version: Option<String>,
    running: bool,
    current: Option<String>,
    autostart: bool,
    presets: Vec<preset::Preset>,
    selected: Option<String>,
    best: Option<String>,
    last_test_at: Option<String>,
    port: u16,
    system_proxy: bool,
    system_proxy_active: bool,
    /// Короткая сводка по серверу: протокол, адрес, шифрование. Ни uuid, ни
    /// пароля здесь нет — им в интерфейсе делать нечего.
    server: Option<String>,
    /// Ссылка сохранена, но не разбирается — говорим, почему
    server_error: Option<String>,
    /// Все известные серверы сводками, в том же порядке, что в настройках
    servers: Vec<String>,
    /// Какой из них выбран
    selected_server: Option<usize>,
    /// Откуда пришёл список
    subscription: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    config: AppConfig,
    dir_ok: bool,
    strategies: Vec<Strategy>,
    running: bool,
    current: Option<String>,
    service: service::ServiceState,
    autostart: bool,
    testing: bool,
    app_version: String,
    managed_dir: String,
    /// loaded | none | any | missing — режим списка адресов ipset-all.txt
    ipset_mode: String,
    fakes: Option<tools::FakesInfo>,
    conflicts: Vec<diag::ConflictItem>,
    /// zapret | byedpi | goodbyedpi | xray | singbox — что выбрано
    engine: String,
    byedpi: ByeState,
    goodbye: GoodbyeState,
    xray: CoreState,
    singbox: CoreState,
    warp: warp::WarpState,
    /// Свои сайты, которые уйдут в проверку: настройки плюс targets.txt
    check_targets: Vec<String>,
    /// Откуда приложение берёт обновления себе
    app_repo: String,
    /// Название движка, который работает, хотя выбран другой. Обычно это
    /// остаток от прошлого запуска приложения — и он молча портит и обход,
    /// и проверку, потому что правит те же самые пакеты.
    stray: Option<String>,
    error: Option<String>,
}

fn build_snapshot(state: &AppState) -> Snapshot {
    let cfg = state.config();
    let root = cfg.zapret_dir.clone();
    let dir_ok = root.as_deref().map(sysutil::looks_like_zapret).unwrap_or(false);
    let mut error = None;
    let strategies = match (&root, dir_ok) {
        (Some(dir), true) => match strategies::list(dir, &cfg.game_filter) {
            Ok(list) => list,
            Err(e) => {
                error = Some(e);
                Vec::new()
            }
        },
        _ => Vec::new(),
    };
    let svc = service::status();
    // Читать системные настройки прокси приходится запуском reg query —
    // на каждый прокси отдельно это уже заметная задержка. Читаем один раз
    let proxy_now = sysproxy::read();
    let bye = build_byedpi_state(state, &cfg, &proxy_now);
    let gdpi = build_goodbye_state(state, &cfg);
    let xr = build_core_state(state, &cfg, "xray", &proxy_now);
    let sb = build_core_state(state, &cfg, "singbox", &proxy_now);
    let zapret_running = state.runner.is_running() || svc.running;

    // Главный экран показывает состояние выбранного движка, а не «хоть что-то работает»
    let name_of = |presets: &[preset::Preset], id: &Option<String>| {
        id.as_deref().and_then(|i| preset::find(presets, i)).map(|p| p.name.clone())
    };
    let (running, current) = match cfg.engine.as_str() {
        "byedpi" => (bye.running, name_of(&bye.presets, &bye.current)),
        "goodbyedpi" => (gdpi.running, name_of(&gdpi.presets, &gdpi.current)),
        "xray" => (xr.running, name_of(&xr.presets, &xr.current)),
        "singbox" => (sb.running, name_of(&sb.presets, &sb.current)),
        _ => (
            zapret_running,
            state
                .runner
                .current
                .lock()
                .unwrap()
                .clone()
                .or_else(|| svc.running.then(|| svc.strategy.clone()).flatten()),
        ),
    };

    Snapshot {
        running,
        current,
        service: svc,
        dir_ok,
        strategies,
        autostart: service::autostart_enabled(),
        testing: state.testing.load(Ordering::Relaxed),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        managed_dir: state.managed_dir().display().to_string(),
        ipset_mode: root.as_deref().map(tools::ipset_mode).unwrap_or_else(|| "missing".into()),
        fakes: root.as_deref().filter(|_| dir_ok).map(tools::fakes),
        conflicts: diag::scan_conflicts(&state.own_processes()),
        check_targets: own_targets(&cfg),
        app_repo: app_repo(&cfg),
        engine: cfg.engine.clone(),
        stray: stray_engine(
            &cfg.engine,
            [zapret_running, bye.running, gdpi.running, xr.running, sb.running],
        ),
        byedpi: bye,
        goodbye: gdpi,
        xray: xr,
        singbox: sb,
        warp: warp::status(),
        config: cfg,
        error,
    }
}

/// Ищет движок, который работает вопреки выбору. Приложение не может просто
/// показать «выключено»: пока чужой winws жив, обход как бы есть, а
/// проверка выбранного движка мерит на самом деле его.
fn stray_engine(engine: &str, running: [bool; ENGINES.len()]) -> Option<String> {
    const NAMES: [&str; ENGINES.len()] = [
        "zapret (winws.exe)",
        "ByeDPI (ciadpi.exe)",
        "GoodbyeDPI (goodbyedpi.exe)",
        "Xray (xray.exe)",
        "sing-box (sing-box.exe)",
    ];
    ENGINES
        .iter()
        .enumerate()
        .find(|(i, id)| running[*i] && **id != engine)
        .map(|(i, _)| NAMES[i].to_string())
}

/// Свои сайты для проверки: из настроек плюс `utils/targets.txt` в папке
/// zapret. Этот файл сборка бережёт между обновлениями, и до сих пор он
/// сохранялся впустую — приложение его не читало.
/// Что последняя проверка сочла лучшим у выбранного движка.
fn engine_best(cfg: &AppConfig) -> Option<String> {
    match cfg.engine.as_str() {
        "byedpi" => cfg.byedpi_best.clone(),
        "goodbyedpi" => cfg.goodbye_best.clone(),
        e if CORES.contains(&e) => core_cfg(cfg, e).best.clone(),
        _ => cfg.best_strategy.clone(),
    }
}

/// Что работает прямо сейчас — в тех же единицах, что и «лучшая»:
/// у zapret это имя стратегии, у остальных — id пресета.
fn engine_current(state: &AppState, cfg: &AppConfig) -> Option<String> {
    match cfg.engine.as_str() {
        "byedpi" => state.proxy.current(),
        "goodbyedpi" => state.goodbye.current(),
        e if CORES.contains(&e) => state.core(e).and_then(|c| c.current()),
        _ => state.runner.current.lock().unwrap().clone().or(cfg.selected_strategy.clone()),
    }
}

/// Репозиторий приложения: из настроек, если задан, иначе зашитый.
fn app_repo(cfg: &AppConfig) -> String {
    cfg.app_repo
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(selfupdate::REPO)
        .to_string()
}

fn own_targets(cfg: &AppConfig) -> Vec<String> {
    let mut list: Vec<String> = cfg.custom_targets.clone();
    let Some(root) = &cfg.zapret_dir else { return list };
    let Ok(text) = std::fs::read_to_string(root.join("utils").join("targets.txt")) else {
        return list;
    };
    for line in text.lines() {
        let line = line.trim();
        if tester::parse_own(line).is_some() && !list.iter().any(|x| x == line) {
            list.push(line.to_string());
        }
    }
    list
}

/// Полный набор целей для прогона: встроенные плюс свои.
fn check_targets(cfg: &AppConfig) -> Vec<tester::Target> {
    tester::all(&own_targets(cfg))
}

// ------------------------------------------------------ прокси-ядра: общее

/// Всё, чем ядра отличаются друг от друга, лежит в их же модулях, поэтому
/// здесь достаточно выбрать нужный `Spec`.
fn core_spec(engine: &str) -> Result<&'static proxycore::Spec, String> {
    match engine {
        "xray" => Ok(&xray::SPEC),
        "singbox" => Ok(&singbox::SPEC),
        other => Err(format!("«{other}» — не прокси-ядро")),
    }
}

fn core_cfg<'a>(cfg: &'a AppConfig, engine: &str) -> &'a config::CoreConfig {
    if engine == "xray" { &cfg.xray } else { &cfg.singbox }
}

fn core_cfg_mut<'a>(cfg: &'a mut AppConfig, engine: &str) -> &'a mut config::CoreConfig {
    if engine == "xray" { &mut cfg.xray } else { &mut cfg.singbox }
}

fn core_default_port(engine: &str) -> u16 {
    if engine == "xray" { config::XRAY_PORT } else { config::SINGBOX_PORT }
}

fn core_port(cfg: &AppConfig, engine: &str) -> u16 {
    core_cfg(cfg, engine).port_or(core_default_port(engine))
}

fn core_presets(engine: &str) -> Vec<preset::Preset> {
    if engine == "xray" { xray::presets() } else { singbox::presets() }
}

fn core_config(
    engine: &str,
    id: &str,
    port: u16,
    server: Option<&link::ServerLink>,
) -> Result<serde_json::Value, String> {
    if engine == "xray" {
        xray::config(id, port, server)
    } else {
        singbox::config(id, port, server)
    }
}

/// Разбирает сохранённую ссылку на сервер. Её могли вставить с опечаткой —
/// тогда пресеты «через сервер» просто не заработают, и приложение обязано
/// сказать об этом заранее, а не после нажатия кнопки.
fn core_server(cfg: &AppConfig, engine: &str) -> Result<Option<link::ServerLink>, String> {
    match core_cfg(cfg, engine).server.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(raw) => link::parse(raw).map(Some),
        None => Ok(None),
    }
}

fn build_core_state(
    state: &AppState,
    cfg: &AppConfig,
    engine: &str,
    proxy_now: &sysproxy::ProxyState,
) -> CoreState {
    let spec = core_spec(engine).expect("движок из списка ядер");
    let cc = core_cfg(cfg, engine);
    let core = state.core(engine).expect("ядро из списка ядер");
    let installed = cc.dir.as_deref().map(|d| proxycore::installed(spec, d)).unwrap_or(false);
    let port = core_port(cfg, engine);
    let (server, server_error) = match core_server(cfg, engine) {
        Ok(Some(l)) => (Some(l.summary()), None),
        Ok(None) => (None, None),
        Err(e) => (None, Some(e)),
    };
    // Наружу — только сводки: в самих ссылках лежат uuid и пароли
    let servers: Vec<String> = cc
        .servers
        .iter()
        .map(|raw| link::parse(raw).map(|l| l.summary()).unwrap_or_else(|e| format!("не разобрать: {e}")))
        .collect();
    let selected_server = cc.server.as_ref().and_then(|s| cc.servers.iter().position(|x| x == s));
    CoreState {
        installed,
        dir: cc.dir.as_ref().map(|d| d.display().to_string()),
        managed: cc.managed,
        managed_dir: state.managed_core_dir(engine).display().to_string(),
        version: cc.version.clone(),
        running: core.is_running(),
        current: core.current(),
        autostart: cc.autostart,
        presets: core_presets(engine),
        selected: cc.preset.clone(),
        best: cc.best.clone(),
        last_test_at: cc.last_test_at.clone(),
        port,
        system_proxy: cc.system_proxy,
        system_proxy_active: core.is_running() && sysproxy::is_ours(proxy_now, port),
        server,
        server_error,
        servers,
        selected_server,
        subscription: cc.subscription.clone(),
    }
}

fn build_goodbye_state(state: &AppState, cfg: &AppConfig) -> GoodbyeState {
    let dir = cfg.goodbye_dir.clone();
    let installed = dir.as_deref().map(goodbye::looks_like_goodbye).unwrap_or(false);
    GoodbyeState {
        blacklist: dir.as_ref().filter(|_| installed).and_then(|d| {
            std::fs::read_to_string(d.join(goodbye::BLACKLIST))
                .ok()
                .map(|t| t.lines().filter(|l| !l.trim().is_empty()).count())
        }),
        installed,
        dir: dir.as_ref().map(|d| d.display().to_string()),
        managed: cfg.goodbye_managed,
        managed_dir: state.managed_goodbye_dir().display().to_string(),
        version: cfg.goodbye_version.clone(),
        running: state.goodbye.is_running(),
        current: state.goodbye.current(),
        autostart: cfg.goodbye_autostart,
        presets: goodbye::all(&cfg.goodbye_custom),
        selected: cfg.goodbye_preset.clone(),
        best: cfg.goodbye_best.clone(),
        last_test_at: cfg.goodbye_last_test_at.clone(),
    }
}

fn build_byedpi_state(
    state: &AppState,
    cfg: &AppConfig,
    proxy_now: &sysproxy::ProxyState,
) -> ByeState {
    let dir = cfg.byedpi_dir.clone();
    let installed = dir.as_deref().map(byedpi::looks_like_byedpi).unwrap_or(false);
    let running = state.proxy.is_running();
    let port = cfg.byedpi_port;
    ByeState {
        installed,
        dir: dir.as_ref().map(|d| d.display().to_string()),
        managed: cfg.byedpi_managed,
        managed_dir: state.managed_byedpi_dir().display().to_string(),
        version: cfg.byedpi_version.clone(),
        current: state.proxy.current(),
        running,
        port,
        system_proxy: cfg.byedpi_system_proxy,
        system_proxy_active: sysproxy::is_ours(proxy_now, port),
        autostart: cfg.byedpi_autostart,
        presets: byedpi::all(&cfg.byedpi_custom),
        selected: cfg.byedpi_preset.clone(),
        best: cfg.byedpi_best.clone(),
        last_test_at: cfg.byedpi_last_test_at.clone(),
    }
}

/// Часть настроек (игровой фильтр, ipset, фейки) читается winws.exe только при
/// старте, поэтому после их смены перезапускаем то, что сейчас работает.
fn restart_if_running(app: &AppHandle, state: &AppState) -> Option<String> {
    let cfg = state.config();
    let root = cfg.zapret_dir.clone()?;
    let svc = service::status();
    let name = cfg.selected_strategy.clone()?;
    let list = strategies::list(&root, &cfg.game_filter).ok()?;
    let s = strategies::find(&list, &name)?;

    if svc.installed {
        // У службы аргументы вшиты в binPath — пересоздаём её с новыми
        return match service::install(&root, &s.name, &s.args) {
            Ok(()) => Some(format!("Служба перезапущена с новыми настройками")),
            Err(e) => Some(format!("Не удалось перезапустить службу: {e}")),
        };
    }
    if state.runner.is_running() {
        return match state.runner.start(app, &root, &s.name, &s.args, true) {
            Ok(()) => Some("Обход перезапущен с новыми настройками".into()),
            Err(e) => Some(format!("Не удалось перезапустить обход: {e}")),
        };
    }
    None
}

// ------------------------------------------------------------ движок ByeDPI

/// Направляет системный прокси в наш SOCKS5 и запоминает, что стояло до нас.
/// Через него ходят все локальные прокси: и ByeDPI, и оба ядра.
fn apply_system_proxy(state: &AppState, port: u16, who: &str) -> Result<String, String> {
    let before = sysproxy::enable(port)?;
    // Если в системе уже стоял наш адрес (перезапуск, смена порта), то
    // «прежним состоянием» остаётся то, что мы сохранили в первый раз, —
    // иначе настройки пользователя потеряются навсегда.
    if !sysproxy::is_ours_any(&before) {
        // Здесь особенно важно не потерять запись: в saved_proxy лежат
        // настройки прокси, которые были у человека до нас, и второго
        // шанса их узнать не будет
        state.update_config(|cfg| cfg.saved_proxy = Some(before))?;
    }
    Ok(format!("Системный прокси направлен в {who} · 127.0.0.1:{port}"))
}

/// Возвращает системный прокси как было. Чужие настройки не трогает: если
/// поверх нашего прокси пользователь поставил свой, это его дело.
fn clear_system_proxy(state: &AppState) -> Result<Option<String>, String> {
    if !sysproxy::is_ours_any(&sysproxy::read()) {
        return Ok(None);
    }
    let saved = state.config().saved_proxy.clone();
    sysproxy::restore(saved.as_ref())?;
    state.update_config(|cfg| cfg.saved_proxy = None)?;
    Ok(Some(match saved {
        Some(s) if s.enabled => "Системный прокси возвращён к прежним настройкам".into(),
        _ => "Системный прокси выключен".into(),
    }))
}

/// Останавливает zapret: два обхода одновременно только мешают друг другу
/// и делают проверку бессмысленной.
fn stop_zapret(app: &AppHandle, state: &AppState) -> Vec<String> {
    let mut messages = Vec::new();
    if service::status().running && service::stop_service().is_ok() {
        messages.push("Служба zapret остановлена".into());
    }
    if state.runner.is_running() {
        state.runner.stop(Some(app));
    }
    messages
}

fn stop_goodbye(app: &AppHandle, state: &AppState) {
    if state.goodbye.is_running() {
        state.goodbye.stop(&state.runner, Some(app));
    }
}

fn start_goodbye(app: &AppHandle, state: &AppState, id: Option<String>) -> Result<Vec<String>, String> {
    let cfg = state.config();
    let root = cfg
        .goodbye_dir
        .clone()
        .filter(|d| goodbye::looks_like_goodbye(d))
        .ok_or("GoodbyeDPI не установлен — скачай его в настройках")?;
    let presets = goodbye::all(&cfg.goodbye_custom);
    let id = id
        .or(cfg.goodbye_preset.clone())
        .or_else(|| presets.first().map(|p| p.id.clone()))
        .ok_or("Пресет не выбран")?;
    let p = goodbye::find(&presets, &id).ok_or("Пресет не найден")?;

    // GoodbyeDPI и zapret делят драйвер WinDivert — вместе они только мешают
    let messages = stop_others(app, state, "goodbyedpi")?;
    state.goodbye.start(app, &state.runner, &root, p, false)?;

    let chosen = p.id.clone();
    state.update_config(|cfg| cfg.goodbye_preset = Some(chosen))?;
    Ok(messages)
}

/// Гасит все движки, кроме указанного: одновременно работает только один,
/// иначе они мешают друг другу, а проверка начинает врать.
fn stop_others(app: &AppHandle, state: &AppState, keep: &str) -> Result<Vec<String>, String> {
    let mut messages = Vec::new();
    if keep != "zapret" {
        messages.extend(stop_zapret(app, state));
    }
    if keep != "byedpi" {
        messages.extend(stop_byedpi(app, state)?);
    }
    if keep != "goodbyedpi" {
        stop_goodbye(app, state);
    }
    for engine in CORES.iter().filter(|e| **e != keep) {
        messages.extend(stop_core(app, state, engine)?);
    }
    Ok(messages)
}

/// Поднимает то, что выбрано в настройках, — нужен там, где обход надо
/// вернуть на место: после остановки чужих обходов и при автозапуске.
fn start_active_engine(app: &AppHandle, state: &AppState) -> Result<Vec<String>, String> {
    match state.config().engine.as_str() {
        "byedpi" => start_byedpi(app, state, None),
        "goodbyedpi" => start_goodbye(app, state, None),
        engine if CORES.contains(&engine) => {
            let engine = engine.to_string();
            start_core(app, state, &engine, None)
        }
        _ => start_zapret(app, state, None).map(|_| Vec::new()),
    }
}

// ------------------------------------------------------- движки Xray и sing-box

fn stop_core(app: &AppHandle, state: &AppState, engine: &str) -> Result<Vec<String>, String> {
    let mut messages = Vec::new();
    let Some(core) = state.core(engine) else { return Ok(messages) };
    if core.is_running() {
        core.stop(&state.runner, Some(app));
    }
    if let Some(m) = clear_system_proxy(state)? {
        messages.push(m);
    }
    Ok(messages)
}

fn start_core(
    app: &AppHandle,
    state: &AppState,
    engine: &str,
    id: Option<String>,
) -> Result<Vec<String>, String> {
    let spec = core_spec(engine)?;
    let cfg = state.config();
    let cc = core_cfg(&cfg, engine);
    let root = cc
        .dir
        .clone()
        .filter(|d| proxycore::installed(spec, d))
        .ok_or_else(|| format!("{} не установлен — скачай его в настройках", spec.name))?;

    let presets = core_presets(engine);
    let id = id
        .or(cc.preset.clone())
        .or_else(|| presets.first().map(|p| p.id.clone()))
        .ok_or("Пресет не выбран")?;
    let p = proxycore::find(&presets, &id).ok_or("Пресет не найден")?;

    // Ссылку разбираем до остановки прежнего движка: если она с опечаткой,
    // честнее оставить работать то, что работало
    let server = core_server(&cfg, engine)?;
    let port = core_port(&cfg, engine);
    let config = core_config(engine, &p.id, port, server.as_ref())?;

    let mut messages = stop_others(app, state, engine)?;
    let core = state.core(engine).expect("ядро из списка ядер");
    core.start(app, &state.runner, &root, p, &config, port, false)?;

    // Режим TUN забирает трафик сам, целым адаптером. Системный прокси
    // поверх него — лишний круг по той же дороге, а снять его надо: иначе
    // после переключения на TUN в настройках Windows остался бы наш адрес
    let tun = engine == "singbox" && singbox::is_tun(&p.id);
    if tun {
        if let Some(m) = clear_system_proxy(state)? {
            messages.push(m);
        }
        messages.push(format!(
            "{}: поднят сетевой адаптер — через него идёт весь трафик машины, включая UDP и игры",
            spec.name
        ));
    } else if cc.system_proxy {
        match apply_system_proxy(state, port, spec.name) {
            Ok(m) => messages.push(m),
            Err(e) => {
                // Прокси, в который никто не ходит, бесполезен — честнее
                // погасить его и сказать, что пошло не так
                core.stop(&state.runner, Some(app));
                return Err(format!("Не удалось настроить системный прокси: {e}"));
            }
        }
    } else {
        messages.push(format!(
            "{} слушает 127.0.0.1:{port} — пропиши этот прокси в приложении сам",
            spec.name
        ));
    }
    if proxycore::needs_server(&p.id) {
        if let Some(l) = &server {
            messages.push(format!("Трафик идёт на твой сервер: {}", l.summary()));
        }
    }

    let chosen = p.id.clone();
    state.update_config(|cfg| core_cfg_mut(cfg, engine).preset = Some(chosen))?;
    Ok(messages)
}

fn stop_byedpi(app: &AppHandle, state: &AppState) -> Result<Vec<String>, String> {
    let mut messages = Vec::new();
    if state.proxy.is_running() {
        state.proxy.stop(&state.runner, Some(app));
    }
    if let Some(m) = clear_system_proxy(state)? {
        messages.push(m);
    }
    Ok(messages)
}

fn start_byedpi(app: &AppHandle, state: &AppState, id: Option<String>) -> Result<Vec<String>, String> {
    let cfg = state.config();
    let root = cfg
        .byedpi_dir
        .clone()
        .filter(|d| byedpi::looks_like_byedpi(d))
        .ok_or("ByeDPI не установлен — скачай его в настройках")?;
    let presets = byedpi::all(&cfg.byedpi_custom);
    let id = id
        .or(cfg.byedpi_preset.clone())
        .or_else(|| presets.first().map(|p| p.id.clone()))
        .ok_or("Пресет не выбран")?;
    let preset = byedpi::find(&presets, &id).ok_or("Пресет не найден")?;

    let mut messages = stop_others(app, state, "byedpi")?;
    state
        .proxy
        .start(app, &state.runner, &root, preset, cfg.byedpi_port, false)?;

    if cfg.byedpi_system_proxy {
        match apply_system_proxy(state, cfg.byedpi_port, "ByeDPI") {
            Ok(m) => messages.push(m),
            Err(e) => {
                // Прокси, в который никто не ходит, бесполезен — честнее
                // погасить его и сказать, что пошло не так.
                state.proxy.stop(&state.runner, Some(app));
                return Err(format!("Не удалось настроить системный прокси: {e}"));
            }
        }
    } else {
        messages.push(format!(
            "ByeDPI слушает 127.0.0.1:{} — пропиши этот SOCKS5 в приложении сам",
            cfg.byedpi_port
        ));
    }

    let chosen = preset.id.clone();
    state.update_config(|cfg| cfg.byedpi_preset = Some(chosen))?;
    Ok(messages)
}

// ---------------------------------------------------------------- команды

/// `(async)` у синхронных команд — не украшение. Tauri выполняет команды
/// без этой пометки на главном потоке, том самом, который рисует окно.
/// А здесь запускаются процессы Windows, ожидается порт ядра и есть явные
/// паузы — всё это время окно не перерисовывалось бы вовсе. С пометкой
/// команда уезжает в пул потоков, тело остаётся прежним.
///
/// Платой идёт то, что команды теперь могут выполняться одновременно, —
/// поэтому фоновые записи настроек ходят через `update_config`.
#[tauri::command(async)]
fn snapshot(state: State<'_, AppState>) -> Snapshot {
    build_snapshot(&state)
}

#[tauri::command(async)]
fn set_zapret_dir(path: String, state: State<'_, AppState>) -> Result<Snapshot, String> {
    let picked = PathBuf::from(&path);
    let root = sysutil::resolve_zapret_root(&picked)
        .ok_or("В этой папке нет bin\\winws.exe — выбери папку с распакованным zapret")?;
    let mut cfg = state.config();
    cfg.zapret_dir = Some(root.clone());
    cfg.managed = false;
    cfg.onboarded = true;
    cfg.installed_version = detect_version(&root);
    if cfg.selected_strategy.is_none() {
        cfg.selected_strategy = strategies::list(&root, &cfg.game_filter)
            .ok()
            .and_then(|l| l.first().map(|s| s.name.clone()));
    }
    state.set_config(cfg)?;
    strategies::ensure_user_lists(&root);
    Ok(build_snapshot(&state))
}

/// Версию берём из service.bat: там строка set "LOCAL_VERSION=1.10.2"
fn detect_version(root: &std::path::Path) -> Option<String> {
    let text = std::fs::read_to_string(root.join("service.bat")).ok()?;
    let line = text.lines().find(|l| l.contains("LOCAL_VERSION="))?;
    let after = line.split("LOCAL_VERSION=").nth(1)?;
    Some(after.trim_matches(['"', ' ', '\r']).to_string())
}

#[tauri::command]
async fn check_update(state: State<'_, AppState>) -> Result<updater::UpdateCheck, String> {
    let current = state.config().installed_version;
    Ok(updater::check(current).await)
}

#[tauri::command]
async fn install_zapret(app: AppHandle, fresh: bool) -> Result<Snapshot, String> {
    do_install(&app, fresh).await
}

/// Общая механика установки: используется и кнопкой, и фоновым автообновлением.
async fn do_install(app: &AppHandle, fresh: bool) -> Result<Snapshot, String> {
    let state = app.state::<AppState>();
    let cfg = state.config();
    let target = if fresh || cfg.zapret_dir.is_none() {
        state.managed_dir()
    } else {
        cfg.zapret_dir.clone().unwrap()
    };
    let managed = fresh || cfg.managed || cfg.zapret_dir.is_none();

    state.install_cancel.store(false, Ordering::SeqCst);
    // Оверлей показываем до похода на GitHub: иначе первые полминуты
    // отменить нечего — окно просто «думает»
    updater::emit(app, "download", 0.0, "Спрашиваю GitHub про последний релиз…");
    let release =
        updater::cancellable(state.install_cancel.clone(), updater::latest_release()).await?;

    // Обновляем только остановленный обход, иначе файлы заняты процессом
    let runner = state.runner.clone();
    let was_running = runner.is_running();
    let restore = runner.current.lock().unwrap().clone();
    tokio::task::spawn_blocking({
        let r = runner.clone();
        move || r.stop(None)
    })
    .await
    .map_err(|e| e.to_string())?;

    let version = updater::install(app, &release, &target, state.install_cancel.clone()).await?;

    let filter = state.config().game_filter.clone();
    let list = strategies::list(&target, &filter).unwrap_or_default();
    // Установка может идти в фоне, пока человек что-то меняет в настройках,
    // поэтому пишем под замком и возвращаем то, что в итоге выбрано
    let selected = state.update_config(|cfg| {
        cfg.zapret_dir = Some(target.clone());
        cfg.managed = managed;
        cfg.installed_version = Some(version);
        cfg.onboarded = true;
        if cfg.selected_strategy.is_none()
            || !list.iter().any(|s| Some(&s.name) == cfg.selected_strategy.as_ref())
        {
            cfg.selected_strategy = list.first().map(|s| s.name.clone());
        }
        cfg.selected_strategy.clone()
    })?;

    if was_running {
        if let Some(name) = restore.or(selected) {
            if let Some(s) = strategies::find(&list, &name) {
                let r = runner.clone();
                let app2 = app.clone();
                let root2 = target.clone();
                let (n, a) = (s.name.clone(), s.args.clone());
                let _ = tokio::task::spawn_blocking(move || r.start(&app2, &root2, &n, &a, false)).await;
            }
        }
    }
    Ok(build_snapshot(&state))
}

/// Установка идёт в отдельной задаче, поэтому «Отмена» просто взводит флаг:
/// загрузка и распаковка проверяют его на каждом шаге.
#[tauri::command]
fn cancel_install(state: State<'_, AppState>) {
    state.install_cancel.store(true, Ordering::SeqCst);
}

#[tauri::command(async)]
fn select_strategy(name: String, state: State<'_, AppState>) -> Result<Snapshot, String> {
    let mut cfg = state.config();
    cfg.selected_strategy = Some(name);
    state.set_config(cfg)?;
    Ok(build_snapshot(&state))
}

#[tauri::command(async)]
fn start_bypass(app: AppHandle, name: Option<String>, state: State<'_, AppState>) -> Result<Snapshot, String> {
    let _guard = state.engine_guard();
    // Что именно сделали — в журнал: тост покажет только «включено»
    let messages = start_bypass_named(&app, &state, name)?;
    for m in messages {
        state.runner.log(Some(&app), "info", m);
    }
    Ok(build_snapshot(&state))
}

fn start_zapret(app: &AppHandle, state: &AppState, name: Option<String>) -> Result<(), String> {
    let cfg = state.config();
    stop_others(app, state, "zapret")?;
    let root = cfg.zapret_dir.clone().ok_or("Папка zapret не выбрана")?;
    let name = name
        .or(cfg.selected_strategy.clone())
        .ok_or("Стратегия не выбрана")?;
    let list = strategies::list(&root, &cfg.game_filter)?;
    let s = strategies::find(&list, &name).ok_or("Стратегия не найдена")?;

    // Если включён автозапуск, обходом управляет служба — работаем через неё,
    // иначе пользователь получит два winws одновременно.
    let svc = service::status();
    if svc.installed {
        if svc.strategy.as_deref() == Some(s.name.as_str()) && !svc.running {
            service::start_service()?;
        } else if svc.strategy.as_deref() != Some(s.name.as_str()) {
            service::install(&root, &s.name, &s.args)?;
            state.runner.log(Some(app), "info", format!("Служба переключена на «{}»", s.name));
        }
        let mut cfg = state.config();
        cfg.selected_strategy = Some(s.name.clone());
        state.set_config(cfg)?;
        return Ok(());
    }

    state.runner.start(app, &root, &s.name, &s.args, false)?;
    let mut cfg = state.config();
    cfg.selected_strategy = Some(s.name.clone());
    state.set_config(cfg)?;
    Ok(())
}

#[tauri::command(async)]
fn stop_bypass(app: AppHandle, state: State<'_, AppState>) -> Result<Snapshot, String> {
    let _guard = state.engine_guard();
    // Гасим всё: пользователь нажал «выключить», а не «переключить»
    for m in stop_others(&app, &state, "")? {
        state.runner.log(Some(&app), "info", m);
    }
    Ok(build_snapshot(&state))
}

// ---------------------------------------------------------- команды ByeDPI

#[tauri::command(async)]
fn set_engine(app: AppHandle, engine: String, state: State<'_, AppState>) -> Result<ActionResult, String> {
    let _guard = state.engine_guard();
    if !ENGINES.contains(&engine.as_str()) {
        return Err("неизвестный движок обхода".into());
    }
    // Новый движок пока не включаем — только освобождаем место
    let mut messages = stop_others(&app, &state, &engine)?;
    let mut cfg = state.config();
    cfg.engine = engine.clone();
    state.set_config(cfg)?;
    messages.push(format!("Обход переключён на {}", engine_name(&engine)));
    Ok(ActionResult { snapshot: build_snapshot(&state), messages })
}

fn engine_name(engine: &str) -> &'static str {
    match engine {
        "byedpi" => "ByeDPI",
        "goodbyedpi" => "GoodbyeDPI",
        "xray" => xray::SPEC.name,
        "singbox" => singbox::SPEC.name,
        _ => "zapret",
    }
}

#[tauri::command(async)]
fn set_byedpi_dir(path: String, state: State<'_, AppState>) -> Result<Snapshot, String> {
    let picked = PathBuf::from(&path);
    let root = byedpi::resolve_root(&picked)
        .ok_or("В этой папке нет ciadpi.exe — выбери папку с распакованным ByeDPI")?;
    let mut cfg = state.config();
    cfg.byedpi_version = byedpi::installed_version(&root);
    cfg.byedpi_dir = Some(root);
    cfg.byedpi_managed = false;
    if cfg.byedpi_preset.is_none() {
        cfg.byedpi_preset = byedpi::builtin().first().map(|p| p.id.clone());
    }
    state.set_config(cfg)?;
    Ok(build_snapshot(&state))
}

#[tauri::command]
async fn check_byedpi_update(state: State<'_, AppState>) -> Result<updater::UpdateCheck, String> {
    Ok(byedpi::check(state.config().byedpi_version).await)
}

#[tauri::command]
async fn install_byedpi(app: AppHandle, fresh: bool) -> Result<Snapshot, String> {
    do_install_byedpi(&app, fresh).await
}

async fn do_install_byedpi(app: &AppHandle, fresh: bool) -> Result<Snapshot, String> {
    let state = app.state::<AppState>();
    let cfg = state.config();
    let target = if fresh || cfg.byedpi_dir.is_none() {
        state.managed_byedpi_dir()
    } else {
        cfg.byedpi_dir.clone().unwrap()
    };
    let managed = fresh || cfg.byedpi_managed || cfg.byedpi_dir.is_none();

    state.install_cancel.store(false, Ordering::SeqCst);
    updater::emit(app, "download", 0.0, "Спрашиваю GitHub про последний релиз…");
    let release =
        updater::cancellable(state.install_cancel.clone(), byedpi::latest_release()).await?;

    // Работающий процесс держит ciadpi.exe — заменить файл не дадут
    let was = state.proxy.current().filter(|_| state.proxy.is_running());
    {
        let (proxy, runner) = (state.proxy.clone(), state.runner.clone());
        let app2 = app.clone();
        tokio::task::spawn_blocking(move || proxy.stop(&runner, Some(&app2)))
            .await
            .map_err(|e| e.to_string())?;
    }

    let version = byedpi::install(app, &release, &target, state.install_cancel.clone()).await?;

    state.update_config(|cfg| {
        cfg.byedpi_dir = Some(target);
        cfg.byedpi_managed = managed;
        cfg.byedpi_version = Some(version);
        if cfg.byedpi_preset.is_none() {
            cfg.byedpi_preset = byedpi::builtin().first().map(|p| p.id.clone());
        }
    })?;

    if let Some(id) = was {
        let _ = start_byedpi(app, &state, Some(id));
    }
    Ok(build_snapshot(&state))
}

/// Пресеты устроены одинаково у ByeDPI и GoodbyeDPI, поэтому команды общие —
/// адресат выбирается по текущему движку.
fn goodbye_engine(state: &AppState) -> bool {
    state.config().engine == "goodbyedpi"
}

#[tauri::command(async)]
fn select_preset(id: String, state: State<'_, AppState>) -> Result<Snapshot, String> {
    let engine = state.config().engine.clone();
    let mut cfg = state.config();
    match engine.as_str() {
        "goodbyedpi" => cfg.goodbye_preset = Some(id),
        e if CORES.contains(&e) => core_cfg_mut(&mut cfg, e).preset = Some(id),
        _ => cfg.byedpi_preset = Some(id),
    }
    state.set_config(cfg)?;
    Ok(build_snapshot(&state))
}

/// У прокси-ядер пресет — это способ собрать конфиг, а не строка ключей:
/// подставить туда свои параметры нельзя, и надо сказать об этом словами.
fn refuse_custom_presets(state: &AppState) -> Result<(), String> {
    let engine = state.config().engine;
    if CORES.contains(&engine.as_str()) {
        return Err(format!(
            "У {} пресеты собираются из конфига, а не из ключей запуска — свой набор сюда не подставить. Свой сервер задаётся ссылкой в настройках",
            engine_name(&engine)
        ));
    }
    Ok(())
}

/// Создаёт или переписывает свой пресет. Пустой `id` — создаём новый.
#[tauri::command(async)]
fn save_preset(
    id: Option<String>,
    name: String,
    args: String,
    state: State<'_, AppState>,
) -> Result<Snapshot, String> {
    refuse_custom_presets(&state)?;
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("У пресета должно быть название".into());
    }
    let parsed = preset::parse_args(&args);
    if parsed.is_empty() {
        return Err("Пустой набор параметров — так ByeDPI ничего не сделает".into());
    }
    if !parsed[0].starts_with('-') {
        return Err(format!("Параметры должны начинаться с ключа, а не с «{}»", parsed[0]));
    }
    let goodbye = goodbye_engine(&state);
    let mut cfg = state.config();
    let list = if goodbye { &mut cfg.goodbye_custom } else { &mut cfg.byedpi_custom };
    let mut created = None;
    match id.filter(|i| !i.is_empty()) {
        Some(id) => {
            let item = list.iter_mut().find(|p| p.id == id).ok_or("Такого пресета нет")?;
            item.name = name;
            item.args = args.trim().to_string();
        }
        None => {
            // id должен пережить переименование, поэтому он не из названия
            let next = format!("custom-{}", chrono::Local::now().timestamp_millis());
            list.push(preset::CustomPreset {
                id: next.clone(),
                name,
                args: args.trim().to_string(),
            });
            created = Some(next);
        }
    }
    if let Some(next) = created {
        if goodbye {
            cfg.goodbye_preset = Some(next);
        } else {
            cfg.byedpi_preset = Some(next);
        }
    }
    state.set_config(cfg)?;
    Ok(build_snapshot(&state))
}

#[tauri::command(async)]
fn delete_preset(id: String, state: State<'_, AppState>) -> Result<Snapshot, String> {
    refuse_custom_presets(&state)?;
    let goodbye = goodbye_engine(&state);
    let mut cfg = state.config();
    let list = if goodbye { &mut cfg.goodbye_custom } else { &mut cfg.byedpi_custom };
    let before = list.len();
    list.retain(|p| p.id != id);
    if list.len() == before {
        return Err("Встроенные пресеты удалить нельзя".into());
    }
    let fallback = if goodbye { goodbye::builtin() } else { byedpi::builtin() }
        .first()
        .map(|p| p.id.clone());
    let (selected, best) = if goodbye {
        (&mut cfg.goodbye_preset, &mut cfg.goodbye_best)
    } else {
        (&mut cfg.byedpi_preset, &mut cfg.byedpi_best)
    };
    if selected.as_deref() == Some(id.as_str()) {
        *selected = fallback;
    }
    if best.as_deref() == Some(id.as_str()) {
        *best = None;
    }
    state.set_config(cfg)?;
    Ok(build_snapshot(&state))
}

// ------------------------------------------------------- команды GoodbyeDPI

#[tauri::command(async)]
fn set_goodbye_dir(path: String, state: State<'_, AppState>) -> Result<Snapshot, String> {
    let root = goodbye::resolve_root(&PathBuf::from(&path))
        .ok_or("В этой папке нет x86_64\\goodbyedpi.exe — выбери папку с распакованным GoodbyeDPI")?;
    let mut cfg = state.config();
    cfg.goodbye_version = goodbye::installed_version(&root);
    cfg.goodbye_dir = Some(root);
    cfg.goodbye_managed = false;
    if cfg.goodbye_preset.is_none() {
        cfg.goodbye_preset = goodbye::builtin().first().map(|p| p.id.clone());
    }
    state.set_config(cfg)?;
    Ok(build_snapshot(&state))
}

#[tauri::command]
async fn check_goodbye_update(state: State<'_, AppState>) -> Result<updater::UpdateCheck, String> {
    Ok(goodbye::check(state.config().goodbye_version).await)
}

#[tauri::command]
async fn install_goodbye(app: AppHandle, fresh: bool) -> Result<Snapshot, String> {
    do_install_goodbye(&app, fresh).await
}

async fn do_install_goodbye(app: &AppHandle, fresh: bool) -> Result<Snapshot, String> {
    let state = app.state::<AppState>();
    let cfg = state.config();
    let target = if fresh || cfg.goodbye_dir.is_none() {
        state.managed_goodbye_dir()
    } else {
        cfg.goodbye_dir.clone().unwrap()
    };
    let managed = fresh || cfg.goodbye_managed || cfg.goodbye_dir.is_none();

    state.install_cancel.store(false, Ordering::SeqCst);
    updater::emit(app, "download", 0.0, "Спрашиваю GitHub про последний релиз…");
    let release =
        updater::cancellable(state.install_cancel.clone(), goodbye::latest_release()).await?;

    // Работающий процесс держит exe и драйвер — заменить файлы не дадут
    let was = state.goodbye.current();
    {
        let (engine, runner) = (state.goodbye.clone(), state.runner.clone());
        let app2 = app.clone();
        tokio::task::spawn_blocking(move || engine.stop(&runner, Some(&app2)))
            .await
            .map_err(|e| e.to_string())?;
    }

    let version = goodbye::install(app, &release, &target, state.install_cancel.clone()).await?;

    state.update_config(|cfg| {
        cfg.goodbye_dir = Some(target);
        cfg.goodbye_managed = managed;
        cfg.goodbye_version = Some(version);
        if cfg.goodbye_preset.is_none() {
            cfg.goodbye_preset = goodbye::builtin().first().map(|p| p.id.clone());
        }
    })?;

    if let Some(id) = was {
        let _ = start_goodbye(app, &state, Some(id));
    }
    Ok(build_snapshot(&state))
}

#[tauri::command]
async fn update_goodbye_blacklist(app: AppHandle, state: State<'_, AppState>) -> Result<ActionResult, String> {
    let root = state
        .config()
        .goodbye_dir
        .clone()
        .filter(|d| goodbye::looks_like_goodbye(d))
        .ok_or("GoodbyeDPI не установлен")?;
    let count = goodbye::update_blacklist(&root).await?;
    let mut messages = vec![format!("Список заблокированного обновлён: {count} доменов")];

    // Списки читаются только при старте — перезапускаем, если обход работает
    if state.goodbye.is_running() {
        let id = state.goodbye.current();
        match start_goodbye(&app, &state, id) {
            Ok(_) => messages.push("GoodbyeDPI перезапущен с новым списком".into()),
            Err(e) => messages.push(format!("Не удалось перезапустить GoodbyeDPI: {e}")),
        }
    }
    Ok(ActionResult { snapshot: build_snapshot(&state), messages })
}

// ------------------------------------------------ команды Xray и sing-box

#[tauri::command(async)]
fn set_core_dir(engine: String, path: String, state: State<'_, AppState>) -> Result<Snapshot, String> {
    let spec = core_spec(&engine)?;
    let root = proxycore::resolve_root(spec, &PathBuf::from(&path)).ok_or_else(|| {
        format!("В этой папке нет {} — выбери папку с распакованным {}", spec.exe, spec.name)
    })?;
    let first = core_presets(&engine).first().map(|p| p.id.clone());
    let mut cfg = state.config();
    let cc = core_cfg_mut(&mut cfg, &engine);
    cc.version = proxycore::installed_version(&root);
    cc.dir = Some(root);
    cc.managed = false;
    if cc.preset.is_none() {
        cc.preset = first;
    }
    state.set_config(cfg)?;
    Ok(build_snapshot(&state))
}

#[tauri::command]
async fn check_core_update(
    engine: String,
    state: State<'_, AppState>,
) -> Result<updater::UpdateCheck, String> {
    let spec = core_spec(&engine)?;
    let current = core_cfg(&state.config(), &engine).version.clone();
    Ok(proxycore::check(spec, current).await)
}

#[tauri::command]
async fn install_core(app: AppHandle, engine: String, fresh: bool) -> Result<Snapshot, String> {
    do_install_core(&app, &engine, fresh).await
}

async fn do_install_core(app: &AppHandle, engine: &str, fresh: bool) -> Result<Snapshot, String> {
    let spec = core_spec(engine)?;
    let state = app.state::<AppState>();
    let (target, managed, was) = {
        let cfg = state.config();
        let cc = core_cfg(&cfg, engine);
        let core = state.core(engine).expect("ядро из списка ядер");
        (
            if fresh || cc.dir.is_none() {
                state.managed_core_dir(engine)
            } else {
                cc.dir.clone().unwrap()
            },
            fresh || cc.managed || cc.dir.is_none(),
            core.current().filter(|_| core.is_running()),
        )
    };

    state.install_cancel.store(false, Ordering::SeqCst);
    updater::emit(app, "download", 0.0, "Спрашиваю GitHub про последний релиз…");
    let release =
        updater::cancellable(state.install_cancel.clone(), proxycore::latest_release(spec)).await?;

    // Работающее ядро держит свой exe — заменить файл не дадут
    {
        let (core, runner) = (state.core(engine).unwrap().clone(), state.runner.clone());
        let app2 = app.clone();
        tokio::task::spawn_blocking(move || core.stop(&runner, Some(&app2)))
            .await
            .map_err(|e| e.to_string())?;
    }

    let version = proxycore::install(spec, app, &release, &target, state.install_cancel.clone()).await?;

    let first = core_presets(engine).first().map(|p| p.id.clone());
    state.update_config(|cfg| {
        let cc = core_cfg_mut(cfg, engine);
        cc.dir = Some(target);
        cc.managed = managed;
        cc.version = Some(version);
        if cc.preset.is_none() {
            cc.preset = first;
        }
    })?;

    if let Some(id) = was {
        let _ = start_core(app, &state, engine, Some(id));
    }
    Ok(build_snapshot(&state))
}

#[tauri::command(async)]
fn set_core_port(
    app: AppHandle,
    engine: String,
    port: u16,
    state: State<'_, AppState>,
) -> Result<ActionResult, String> {
    let _guard = state.engine_guard();
    let spec = core_spec(&engine)?;
    if port < 1024 {
        return Err("Порты ниже 1024 заняты системой — возьми что-нибудь от 1024".into());
    }
    let core = state.core(&engine).expect("ядро из списка ядер").clone();
    let restart = core.current().filter(|_| core.is_running());
    // Старый адрес в системных настройках сразу перестаёт работать,
    // поэтому ядро гасим до смены порта
    stop_core(&app, &state, &engine)?;

    let mut cfg = state.config();
    core_cfg_mut(&mut cfg, &engine).port = port;
    state.set_config(cfg)?;

    let mut messages = vec![format!("{} переехал на порт {port}", spec.name)];
    if let Some(id) = restart {
        messages.extend(start_core(&app, &state, &engine, Some(id))?);
    }
    Ok(ActionResult { snapshot: build_snapshot(&state), messages })
}

/// Сохраняет ссылку на свой сервер. Пустая строка — забыть сервер: тогда
/// у ядра остаётся только фрагментация.
#[tauri::command(async)]
fn set_core_server(
    app: AppHandle,
    engine: String,
    url: String,
    state: State<'_, AppState>,
) -> Result<ActionResult, String> {
    core_spec(&engine)?;
    let url = url.trim().to_string();
    // Разбираем до сохранения: незачем хранить строку, которая не заработает
    let parsed = if url.is_empty() { None } else { Some(link::parse(&url)?) };

    let mut cfg = state.config();
    {
        let cc = core_cfg_mut(&mut cfg, &engine);
        // Вставленный руками сервер попадает в общий список: иначе он
        // потеряется, стоит выбрать другой из подписки
        if let Some(url) = (!url.is_empty()).then_some(url.clone()) {
            if !cc.servers.contains(&url) {
                cc.servers.push(url);
            }
        }
        cc.server = (!url.is_empty()).then_some(url);
    }
    state.set_config(cfg)?;

    let mut messages = vec![match &parsed {
        Some(l) => format!("Сервер сохранён: {}", l.summary()),
        None => "Сервер забыт — у ядра остаётся только фрагментация".to_string(),
    }];

    // Конфиг читается только при запуске: если ядро работает на серверном
    // пресете, без перезапуска новая ссылка никуда не попадёт
    let core = state.core(&engine).expect("ядро из списка ядер");
    if let Some(id) = core.current().filter(|_| core.is_running()) {
        if proxycore::needs_server(&id) {
            match start_core(&app, &state, &engine, Some(id)) {
                Ok(m) => messages.extend(m),
                Err(e) => messages.push(format!("Не удалось перезапустить с новым сервером: {e}")),
            }
        }
    }
    Ok(ActionResult { snapshot: build_snapshot(&state), messages })
}

/// Отклик сервера: время установки TCP-соединения до него. Это не скорость
/// и не гарантия, что сервер работает, — но мёртвый или далёкий виден сразу.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ServerPing {
    index: usize,
    ms: Option<u64>,
}

/// Скачивает подписку и раскладывает её в список серверов.
#[tauri::command]
async fn load_subscription(
    engine: String,
    url: String,
    state: State<'_, AppState>,
) -> Result<ActionResult, String> {
    let spec = core_spec(&engine)?;
    let url = url.trim().to_string();
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("Ссылка на подписку должна начинаться с http:// или https://".into());
    }

    let client = reqwest::Client::builder()
        .user_agent(concat!("ZapretStudio/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client.get(&url).send().await.map_err(|e| {
        format!("Подписка не скачалась: {e}. Если её тоже режут, включи обход и попробуй снова")
    })?;
    if !resp.status().is_success() {
        return Err(format!("Панель ответила {}", resp.status()));
    }
    let text = resp.text().await.map_err(|e| format!("ответ не прочитан: {e}"))?;

    let found = link::parse_many(&text);
    if found.is_empty() {
        return Err("В подписке не нашлось ни одного сервера — проверь ссылку".into());
    }

    let raw: Vec<String> = {
        // Сохраняем строки как есть: разобранная ссылка теряет мелочи,
        // которых мы не знаем, а конфиг собирается из исходной
        let decoded = if text.contains("://") {
            text.clone()
        } else {
            String::from_utf8_lossy(&link::b64(text.trim()).unwrap_or_default()).to_string()
        };
        decoded
            .lines()
            .map(str::trim)
            .filter(|l| link::parse(l).is_ok())
            .map(str::to_string)
            .collect()
    };

    let mut cfg = state.config();
    {
        let cc = core_cfg_mut(&mut cfg, &engine);
        cc.subscription = Some(url);
        cc.servers = raw;
        // Прежний выбор мог исчезнуть из подписки — тогда берём первый
        if !cc.server.as_ref().is_some_and(|s| cc.servers.contains(s)) {
            cc.server = cc.servers.first().cloned();
        }
    }
    state.set_config(cfg)?;
    Ok(ActionResult {
        snapshot: build_snapshot(&state),
        messages: vec![format!("{}: серверов из подписки — {}", spec.name, found.len())],
    })
}

/// Выбирает сервер из списка. Если ядро работает на серверном пресете,
/// перезапускаем: конфиг читается только при старте.
#[tauri::command(async)]
fn select_server(
    app: AppHandle,
    engine: String,
    index: usize,
    state: State<'_, AppState>,
) -> Result<ActionResult, String> {
    let _guard = state.engine_guard();
    core_spec(&engine)?;
    let mut cfg = state.config();
    let chosen = {
        let cc = core_cfg_mut(&mut cfg, &engine);
        let chosen = cc.servers.get(index).cloned().ok_or("Такого сервера в списке нет")?;
        cc.server = Some(chosen.clone());
        chosen
    };
    state.set_config(cfg)?;

    let summary = link::parse(&chosen).map(|l| l.summary()).unwrap_or_default();
    let mut messages = vec![format!("Сервер выбран: {summary}")];
    let core = state.core(&engine).expect("ядро из списка ядер");
    if let Some(id) = core.current().filter(|_| core.is_running()) {
        if proxycore::needs_server(&id) {
            match start_core(&app, &state, &engine, Some(id)) {
                Ok(m) => messages.extend(m),
                Err(e) => messages.push(format!("Не удалось перейти на новый сервер: {e}")),
            }
        }
    }
    Ok(ActionResult { snapshot: build_snapshot(&state), messages })
}

/// Меряет отклик до каждого сервера сразу до всех — по очереди это заняло бы
/// минуты на подписке из двух десятков узлов.
#[tauri::command]
async fn ping_servers(engine: String, state: State<'_, AppState>) -> Result<Vec<ServerPing>, String> {
    core_spec(&engine)?;
    let servers = core_cfg(&state.config(), &engine).servers.clone();

    let probes = servers.into_iter().enumerate().map(|(index, raw)| async move {
        let Ok(l) = link::parse(&raw) else { return ServerPing { index, ms: None } };
        let started = std::time::Instant::now();
        let addr = format!("{}:{}", l.host, l.port);
        let ok = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            tokio::net::TcpStream::connect(addr),
        )
        .await
        .map(|r| r.is_ok())
        .unwrap_or(false);
        ServerPing { index, ms: ok.then(|| started.elapsed().as_millis() as u64) }
    });
    Ok(futures_util::future::join_all(probes).await)
}

#[tauri::command(async)]
fn set_byedpi_port(app: AppHandle, port: u16, state: State<'_, AppState>) -> Result<ActionResult, String> {
    let _guard = state.engine_guard();
    if port < 1024 {
        return Err("Порты ниже 1024 заняты системой — возьми что-нибудь от 1024".into());
    }
    let restart = state.proxy.current().filter(|_| state.proxy.is_running());
    // Старый адрес в системных настройках сразу перестаёт работать,
    // поэтому прокси гасим до смены порта
    stop_byedpi(&app, &state)?;

    let mut cfg = state.config();
    cfg.byedpi_port = port;
    state.set_config(cfg)?;

    let mut messages = vec![format!("ByeDPI переехал на порт {port}")];
    if let Some(id) = restart {
        messages.extend(start_byedpi(&app, &state, Some(id))?);
    }
    Ok(ActionResult { snapshot: build_snapshot(&state), messages })
}

/// Настройка есть у каждого локального прокси — ByeDPI, Xray, sing-box, — и у
/// каждого она своя. Движок приходит явно: в настройках карточки показываются
/// все сразу, и «тот, который выбран» тут не подходит.
#[tauri::command(async)]
fn set_system_proxy(
    engine: String,
    enable: bool,
    state: State<'_, AppState>,
) -> Result<ActionResult, String> {
    let mut cfg = state.config();
    let (port, running, who) = match engine.as_str() {
        e if CORES.contains(&e) => {
            core_cfg_mut(&mut cfg, e).system_proxy = enable;
            (
                core_port(&cfg, e),
                state.core(e).map(|c| c.is_running()).unwrap_or(false),
                core_spec(e)?.name,
            )
        }
        "byedpi" => {
            cfg.byedpi_system_proxy = enable;
            (cfg.byedpi_port, state.proxy.is_running(), "ByeDPI")
        }
        other => return Err(format!("У «{other}» своего прокси нет — заворачивать нечего")),
    };
    state.set_config(cfg)?;

    let mut messages = Vec::new();
    if enable && running {
        messages.push(apply_system_proxy(&state, port, who)?);
    } else if !enable {
        if let Some(m) = clear_system_proxy(&state)? {
            messages.push(m);
        }
        if running {
            messages.push(format!(
                "Трафик больше не заворачивается сам — прокси ждёт на 127.0.0.1:{port}"
            ));
        }
    }
    Ok(ActionResult { snapshot: build_snapshot(&state), messages })
}

/// Проверка пресетов активного движка — ByeDPI или GoodbyeDPI.
#[tauri::command]
async fn run_preset_tests(
    app: AppHandle,
    ids: Vec<String>,
    include_baseline: bool,
    state: State<'_, AppState>,
) -> Result<Vec<tester::StrategyResult>, String> {
    if state.testing.swap(true, Ordering::SeqCst) {
        return Err("Проверка уже идёт".into());
    }
    state.cancel.store(false, Ordering::SeqCst);
    let finish = || state.testing.store(false, Ordering::SeqCst);

    let cfg = state.config();
    // Иначе проверка мерила бы чужой движок: он правит те же пакеты
    if let Err(e) = stop_others(&app, &state, &cfg.engine) {
        finish();
        return Err(e);
    }
    let engine = cfg.engine.clone();
    let core_mode = CORES.contains(&engine.as_str());
    let goodbye_mode = engine == "goodbyedpi";

    let dir = if core_mode {
        let spec = match core_spec(&engine) {
            Ok(s) => s,
            Err(e) => {
                finish();
                return Err(e);
            }
        };
        core_cfg(&cfg, &engine).dir.clone().filter(|d| proxycore::installed(spec, d))
    } else if goodbye_mode {
        cfg.goodbye_dir.clone().filter(|d| goodbye::looks_like_goodbye(d))
    } else {
        cfg.byedpi_dir.clone().filter(|d| byedpi::looks_like_byedpi(d))
    };
    let Some(root) = dir else {
        finish();
        return Err(format!("{} не установлен", engine_name(&engine)));
    };

    let result = if core_mode {
        // Конфиги собираем заранее. Пресеты «через свой сервер» без ссылки
        // собрать нельзя — они просто выпадают из проверки, а не роняют её
        let port = core_port(&cfg, &engine);
        let server = match core_server(&cfg, &engine) {
            Ok(s) => s,
            Err(e) => {
                finish();
                return Err(e);
            }
        };
        let catalog: Vec<_> = core_presets(&engine)
            .into_iter()
            .filter_map(|p| {
                core_config(&engine, &p.id, port, server.as_ref()).ok().map(|c| (p, c))
            })
            .collect();
        let core = state.core(&engine).expect("ядро из списка ядер").clone();
        tester::run_core_suite(
            app.clone(),
            root,
            port,
            check_targets(&cfg),
            catalog,
            ids,
            include_baseline,
            core,
            state.runner.clone(),
            state.cancel.clone(),
        )
        .await
    } else if goodbye_mode {
        tester::run_goodbye_suite(
            app.clone(),
            root,
            check_targets(&cfg),
            goodbye::all(&cfg.goodbye_custom),
            ids,
            include_baseline,
            state.goodbye.clone(),
            state.runner.clone(),
            state.cancel.clone(),
        )
        .await
    } else {
        tester::run_byedpi_suite(
            app.clone(),
            root,
            cfg.byedpi_port,
            check_targets(&cfg),
            byedpi::all(&cfg.byedpi_custom),
            ids,
            include_baseline,
            state.proxy.clone(),
            state.runner.clone(),
            state.cancel.clone(),
        )
        .await
    };

    finish();

    if let Ok(results) = &result {
        let best = results
            .iter()
            .filter(|r| !r.baseline && r.started)
            .max_by(|a, b| {
                a.score
                    .partial_cmp(&b.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| b.avg_ms.unwrap_or(9999).cmp(&a.avg_ms.unwrap_or(9999)))
            })
            .map(|r| r.strategy.clone());
        let _ = state.update_config(|cfg| {
            let now = sysutil::now_iso();
            if core_mode {
                let cc = core_cfg_mut(cfg, &engine);
                cc.best = best;
                cc.last_test_at = Some(now);
            } else if goodbye_mode {
                cfg.goodbye_best = best;
                cfg.goodbye_last_test_at = Some(now);
            } else {
                cfg.byedpi_best = best;
                cfg.byedpi_last_test_at = Some(now);
            }
        });
    }
    result
}

#[tauri::command]
async fn run_tests(
    app: AppHandle,
    names: Vec<String>,
    include_baseline: bool,
    state: State<'_, AppState>,
) -> Result<Vec<tester::StrategyResult>, String> {
    if state.testing.swap(true, Ordering::SeqCst) {
        return Err("Проверка уже идёт".into());
    }
    state.cancel.store(false, Ordering::SeqCst);

    let cfg = state.config();
    let root = match cfg.zapret_dir.clone() {
        Some(r) => r,
        None => {
            state.testing.store(false, Ordering::SeqCst);
            return Err("Папка zapret не выбрана".into());
        }
    };
    if service::status().running {
        state.testing.store(false, Ordering::SeqCst);
        return Err("Работает служба zapret — останови её, иначе проверка будет мерить службу, а не стратегии".into());
    }
    // Прокси ByeDPI и GoodbyeDPI тоже влияют на результат
    if let Err(e) = stop_others(&app, &state, "zapret") {
        state.testing.store(false, Ordering::SeqCst);
        return Err(e);
    }

    let result = tester::run_suite(
        app.clone(),
        root,
        check_targets(&cfg),
        cfg.game_filter.clone(),
        names,
        include_baseline,
        state.runner.clone(),
        state.cancel.clone(),
    )
    .await;

    state.testing.store(false, Ordering::SeqCst);

    if let Ok(results) = &result {
        let best = results
            .iter()
            .filter(|r| !r.baseline && r.started)
            .max_by(|a, b| {
                a.score
                    .partial_cmp(&b.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| b.avg_ms.unwrap_or(9999).cmp(&a.avg_ms.unwrap_or(9999)))
            })
            .map(|r| r.strategy.clone());
        let _ = state.update_config(|cfg| {
            cfg.best_strategy = best;
            cfg.last_test_at = Some(sysutil::now_iso());
        });
    }
    result
}

/// Сохраняет список своих сайтов. Строки, которые нельзя разобрать,
/// не сохраняем молча — иначе человек будет ждать проверки того, чего
/// в списке не окажется.
#[tauri::command(async)]
fn set_custom_targets(items: Vec<String>, state: State<'_, AppState>) -> Result<Snapshot, String> {
    let mut clean: Vec<String> = Vec::new();
    for raw in items {
        let raw = raw.trim().to_string();
        if raw.is_empty() {
            continue;
        }
        if tester::parse_own(&raw).is_none() {
            return Err(format!(
                "«{raw}» не похоже на адрес. Жду «example.com», «https://example.com/путь» или «example.com:443»"
            ));
        }
        if !clean.contains(&raw) {
            clean.push(raw);
        }
    }
    let mut cfg = state.config();
    cfg.custom_targets = clean;
    state.set_config(cfg)?;
    Ok(build_snapshot(&state))
}

/// Тот же осмотр, что делает сторож, но по нажатию.
#[tauri::command]
async fn health_check(state: State<'_, AppState>) -> Result<tester::Health, String> {
    let (proxy, own) = {
        let cfg = state.config();
        (state.own_proxy_port(), own_targets(&cfg))
    };
    Ok(tester::health(proxy, &own).await)
}

#[tauri::command]
async fn check_app_update(state: State<'_, AppState>) -> Result<updater::UpdateCheck, String> {
    let repo = app_repo(&state.config());
    Ok(selfupdate::check(&repo).await)
}

/// Качает установщик, сверяет сумму, запускает его и уходит: заменить свои
/// же файлы на ходу нельзя. Само по таймеру это не делается никогда —
/// только по нажатию.
#[tauri::command]
async fn install_app_update(app: AppHandle) -> Result<String, String> {
    let (repo, data_dir, cancel) = {
        let state = app.state::<AppState>();
        // По отдельности, а не одним кортежем: замок на data_dir иначе
        // переживёт сам state, и компилятор справедливо против
        let repo = app_repo(&state.config());
        let data_dir = state.data_dir.lock().unwrap().clone();
        let cancel = state.install_cancel.clone();
        (repo, data_dir, cancel)
    };
    cancel.store(false, Ordering::SeqCst);
    updater::emit(&app, "download", 0.0, "Спрашиваю GitHub про версию приложения…");
    let release = updater::cancellable(cancel.clone(), selfupdate::latest(&repo)).await?;

    let (path, note) = selfupdate::download(&app, &release, &data_dir, cancel).await?;
    if let Some(note) = note {
        let state = app.state::<AppState>();
        state.runner.log(Some(&app), "err", format!("Обновление приложения: {note}"));
    }
    updater::emit(&app, "apply", 98.0, "Запускаю установщик…");

    // Уходим красиво: снимаем системный прокси и гасим обход, иначе
    // пользователь останется с прокси в никуда, пока идёт установка
    {
        let state = app.state::<AppState>();
        let _ = stop_others(&app, &state, "");
    }
    selfupdate::launch(&path)?;
    let handle = app.clone();
    // Даём установщику подняться, прежде чем освободить свои файлы
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(900));
        handle.exit(0);
    });
    Ok(format!("Устанавливаю версию {} — приложение сейчас закроется", release.version))
}

/// Профиль настроек текстом — его пересылают другому человеку.
#[tauri::command(async)]
fn export_profile(state: State<'_, AppState>) -> Result<String, String> {
    profile::to_text(&state.config())
}

/// Накладывает присланный профиль. Свои папки, версии и ссылки на серверы
/// остаются на месте: в профиле их нет и быть не должно.
#[tauri::command(async)]
fn import_profile(text: String, state: State<'_, AppState>) -> Result<ActionResult, String> {
    let incoming = profile::parse(&text)?;
    let mut cfg = state.config();
    let changed = profile::apply(&incoming, &mut cfg);
    state.set_config(cfg)?;

    let mut messages = vec![match changed.len() {
        0 => "Профиль применён — у тебя уже были те же настройки".to_string(),
        _ => format!("Профиль применён: {}", changed.join(", ")),
    }];
    messages.push("Обход не перезапускала — включи его сама, когда будешь готова".into());
    Ok(ActionResult { snapshot: build_snapshot(&state), messages })
}

#[tauri::command]
fn cancel_tests(state: State<'_, AppState>) {
    state.cancel.store(true, Ordering::SeqCst);
}

#[tauri::command(async)]
fn install_service(name: Option<String>, state: State<'_, AppState>) -> Result<Snapshot, String> {
    let cfg = state.config();
    let root = cfg.zapret_dir.clone().ok_or("Папка zapret не выбрана")?;
    let name = name.or(cfg.selected_strategy.clone()).ok_or("Стратегия не выбрана")?;
    let list = strategies::list(&root, &cfg.game_filter)?;
    let s = strategies::find(&list, &name).ok_or("Стратегия не найдена")?;
    state.runner.stop(None);
    service::install(&root, &s.name, &s.args)?;
    let mut cfg = state.config();
    cfg.autostart_bypass = true;
    cfg.selected_strategy = Some(s.name.clone());
    state.set_config(cfg)?;
    Ok(build_snapshot(&state))
}

#[tauri::command(async)]
fn remove_service(state: State<'_, AppState>) -> Result<Snapshot, String> {
    service::remove()?;
    let mut cfg = state.config();
    cfg.autostart_bypass = false;
    state.set_config(cfg)?;
    Ok(build_snapshot(&state))
}

#[tauri::command(async)]
fn set_app_autostart(enable: bool, state: State<'_, AppState>) -> Result<Snapshot, String> {
    service::set_autostart(enable)?;
    let mut cfg = state.config();
    cfg.app_autostart = enable;
    state.set_config(cfg)?;
    Ok(build_snapshot(&state))
}

#[tauri::command(async)]
fn set_option(key: String, value: serde_json::Value, state: State<'_, AppState>) -> Result<Snapshot, String> {
    let cfg = state.config();
    let mut raw = serde_json::to_value(&cfg).map_err(|e| e.to_string())?;
    if let Some(obj) = raw.as_object_mut() {
        obj.insert(key, value);
    }
    let next: AppConfig = serde_json::from_value(raw).map_err(|e| e.to_string())?;
    state.set_config(next)?;
    Ok(build_snapshot(&state))
}

/// Главный экран дёргает это раз в пару секунд, пока открыт.
#[tauri::command]
async fn ping(state: State<'_, AppState>) -> Result<tester::NetSample, String> {
    Ok(tester::ping(state.own_proxy_port()).await)
}

#[tauri::command]
async fn measure_speed(state: State<'_, AppState>) -> Result<f64, String> {
    tester::speed(state.own_proxy_port()).await
}

#[tauri::command]
fn get_logs(state: State<'_, AppState>) -> Vec<LogLine> {
    state.runner.logs()
}

#[tauri::command]
fn clear_logs(state: State<'_, AppState>) {
    state.runner.clear_logs();
}

#[tauri::command]
async fn diagnostics(state: State<'_, AppState>) -> Result<Vec<diag::Check>, String> {
    let (root, port, own) = (state.root(), state.own_proxy_port(), state.own_processes());
    let mut checks = diag::run(root.as_deref(), port, &own);
    // Сетевые проверки идут отдельно и параллельно: каждая ждёт ответа
    // до нескольких секунд, а вместе укладываются в те же секунды
    let (address, ipv6) = tokio::join!(diag::external_address(), diag::ipv6_leak());
    checks.push(address);
    checks.push(ipv6);
    Ok(checks)
}

/// Гасит движки, которые работают вопреки выбору, — обычно остаток от
/// прошлого запуска приложения.
#[tauri::command(async)]
fn stop_stray(app: AppHandle, state: State<'_, AppState>) -> Result<ActionResult, String> {
    let _guard = state.engine_guard();
    let engine = state.config().engine.clone();
    let mut messages = stop_others(&app, &state, &engine)?;
    if messages.is_empty() {
        messages.push("Лишних движков не работало".into());
    }
    for m in &messages {
        state.runner.log(Some(&app), "info", m.clone());
    }
    Ok(ActionResult { snapshot: build_snapshot(&state), messages })
}

#[tauri::command(async)]
fn warp_connect(state: State<'_, AppState>) -> Result<ActionResult, String> {
    let message = warp::connect()?;
    Ok(ActionResult { snapshot: build_snapshot(&state), messages: vec![message] })
}

#[tauri::command(async)]
fn warp_disconnect(state: State<'_, AppState>) -> Result<ActionResult, String> {
    let message = warp::disconnect()?;
    Ok(ActionResult { snapshot: build_snapshot(&state), messages: vec![message] })
}

/// Спрашивает у Cloudflare, идёт ли трафик через туннель на самом деле.
/// `warp-cli status` знает лишь то, что думает о себе служба: она может
/// считать себя подключённой, пока туннель молчит, — и наоборот.
#[tauri::command]
async fn warp_check() -> warp::WarpProbe {
    warp::probe().await
}

/// Открывает страницу в браузере. Список закрытый: адреса приходят из
/// интерфейса, но проверить их всё равно дешевле, чем доверять.
#[tauri::command(async)]
fn open_url(url: String) -> Result<(), String> {
    const ALLOWED: [&str; 4] = [
        warp::DOWNLOAD_URL,
        "https://github.com/XTLS/Xray-core",
        "https://github.com/SagerNet/sing-box",
        "https://github.com/hufrea/byedpi",
    ];
    if !ALLOWED.contains(&url.as_str()) {
        return Err(format!("Эту ссылку приложение не открывает: {url}"));
    }
    // explorer.exe отдаёт ссылку браузеру по умолчанию и своего окна не рисует
    std::process::Command::new("explorer")
        .arg(&url)
        .spawn()
        .map_err(|e| format!("не удалось открыть браузер: {e}"))?;
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionResult {
    snapshot: Snapshot,
    messages: Vec<String>,
}

/// Останавливает чужие обходы, найденные диагностикой, — чтобы не искать их
/// по системе руками.
#[tauri::command(async)]
fn stop_conflicts(
    app: AppHandle,
    items: Option<Vec<diag::ConflictItem>>,
    state: State<'_, AppState>,
) -> Result<ActionResult, String> {
    let _guard = state.engine_guard();
    let own = state.own_processes();
    let items = items.unwrap_or_else(|| diag::scan_conflicts(&own));
    if items.is_empty() {
        return Err("Конфликтующих программ не найдено".into());
    }
    let was_running = state.engine_running();

    let mut messages = diag::stop_conflicts(&items);
    for m in &messages {
        state.runner.log(Some(&app), "info", m.clone());
    }

    // Чужой обход, уходя, утаскивает за собой драйвер WinDivert — а вместе с
    // ним падает и наш. Поэтому свой обход поднимаем обратно сами.
    if was_running && !state.engine_running() {
        match start_active_engine(&app, &state) {
            Ok(_) => messages.push("Свой обход упал вместе с чужим и поднят заново".into()),
            Err(e) => messages.push(format!("Свой обход не поднялся обратно: {e}")),
        }
    }

    if diag::scan_conflicts(&state.own_processes()).is_empty() {
        messages.push("Чужих обходов больше не осталось".into());
    }
    Ok(ActionResult { snapshot: build_snapshot(&state), messages })
}

/// Игровой фильтр меняет диапазон портов в аргументах winws — после смены
/// обход надо перезапустить, иначе настройка не применится.
#[tauri::command(async)]
fn set_game_filter(app: AppHandle, mode: String, state: State<'_, AppState>) -> Result<ActionResult, String> {
    if !["off", "all", "tcp", "udp"].contains(&mode.as_str()) {
        return Err("неизвестный режим игрового фильтра".into());
    }
    let mut cfg = state.config();
    cfg.game_filter = mode.clone();
    state.set_config(cfg)?;

    // Держим флаг-файл в синхроне: батники и service.bat читают именно его
    if let Some(root) = state.root() {
        let flag = root.join("utils").join("game_filter.enabled");
        let _ = std::fs::create_dir_all(root.join("utils"));
        if mode == "off" {
            let _ = std::fs::remove_file(&flag);
        } else {
            let _ = std::fs::write(&flag, format!("{mode}\n"));
        }
    }

    let mut messages = Vec::new();
    if let Some(m) = restart_if_running(&app, &state) {
        messages.push(m);
    }
    Ok(ActionResult { snapshot: build_snapshot(&state), messages })
}

#[tauri::command(async)]
fn set_ipset_mode(app: AppHandle, mode: String, state: State<'_, AppState>) -> Result<ActionResult, String> {
    let root = state.root().ok_or("Папка zapret не выбрана")?;
    tools::set_ipset_mode(&root, &mode)?;
    let mut messages = vec![match mode.as_str() {
        "loaded" => "Список адресов включён".to_string(),
        "none" => "Правила по списку адресов отключены".to_string(),
        _ => "Правила применяются к любым адресам".to_string(),
    }];
    if let Some(m) = restart_if_running(&app, &state) {
        messages.push(m);
    }
    Ok(ActionResult { snapshot: build_snapshot(&state), messages })
}

#[tauri::command]
async fn update_ipset(app: AppHandle, state: State<'_, AppState>) -> Result<ActionResult, String> {
    let root = state.root().ok_or("Папка zapret не выбрана")?;
    let count = tools::update_ipset(&root).await?;
    let mut messages = vec![format!("Список адресов обновлён: {count} записей")];
    if let Some(m) = restart_if_running(&app, &state) {
        messages.push(m);
    }
    Ok(ActionResult { snapshot: build_snapshot(&state), messages })
}

#[tauri::command(async)]
fn set_active_fake(
    app: AppHandle,
    kind: String,
    source: String,
    state: State<'_, AppState>,
) -> Result<ActionResult, String> {
    let root = state.root().ok_or("Папка zapret не выбрана")?;
    tools::set_active_fake(&root, &kind, &source)?;
    let mut messages = vec![format!("Активный фейк заменён на «{source}»")];
    if let Some(m) = restart_if_running(&app, &state) {
        messages.push(m);
    }
    Ok(ActionResult { snapshot: build_snapshot(&state), messages })
}

#[tauri::command(async)]
fn clear_discord_cache(app: AppHandle, state: State<'_, AppState>) -> ActionResult {
    let messages = tools::clear_discord_cache();
    for m in &messages {
        state.runner.log(Some(&app), "info", m.clone());
    }
    ActionResult { snapshot: build_snapshot(&state), messages }
}

#[tauri::command]
async fn hosts_status() -> Result<tools::HostsStatus, String> {
    tools::hosts_status().await
}

#[tauri::command]
async fn apply_hosts() -> Result<String, String> {
    match tools::apply_hosts().await? {
        0 => Ok("В hosts уже всё на месте".into()),
        n => Ok(format!("Дописано записей в hosts: {n}")),
    }
}

#[tauri::command(async)]
fn open_folder(state: State<'_, AppState>) -> Result<(), String> {
    let root = state.root().ok_or("Папка не выбрана")?;
    std::process::Command::new("explorer")
        .arg(root)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn quit_app(app: AppHandle, state: State<'_, AppState>) {
    // Без этого системный прокси остался бы указывать на мёртвый порт
    let _ = stop_others(&app, &state, "");
    app.exit(0);
}

// ------------------------------------------------- фоновые обновления

/// Пора ли снова спрашивать GitHub: сверяем время прошлой проверки с интервалом.
fn check_due(cfg: &AppConfig) -> bool {
    let Some(last) = cfg.last_update_check.as_deref() else { return true };
    let Ok(parsed) = chrono::NaiveDateTime::parse_from_str(last, "%Y-%m-%d %H:%M:%S") else {
        return true;
    };
    let elapsed = chrono::Local::now().naive_local() - parsed;
    elapsed.num_minutes() >= (cfg.update_interval_hours.max(1) as i64) * 60
}

fn notify(app: &AppHandle, title: &str, body: &str) {
    use tauri_plugin_notification::NotificationExt;
    let _ = app.notification().builder().title(title).body(body).show();
}

async fn background_check(app: &AppHandle) {
    let (current, installed, auto_install) = {
        let state = app.state::<AppState>();
        let cfg = state.config();
        (
            cfg.installed_version.clone(),
            cfg.zapret_dir.as_deref().map(sysutil::looks_like_zapret).unwrap_or(false),
            cfg.auto_install_updates,
        )
    };
    if !installed {
        return;
    }

    let result = updater::check(current).await;
    let _ = app.emit("update-check", result.clone());

    if !result.has_update {
        return;
    }
    let version = result.latest.clone().unwrap_or_default();

    if auto_install {
        notify(app, "Zapret Studio", &format!("Ставлю обновление zapret {version}…"));
        match do_install(app, false).await {
            Ok(_) => {
                notify(app, "Zapret Studio", &format!("Обновление {version} установлено"));
                let _ = app.emit("refresh", ());
            }
            Err(e) => {
                notify(app, "Zapret Studio", &format!("Обновление не установилось: {e}"));
                let state = app.state::<AppState>();
                state.runner.log(Some(app), "err", format!("Автообновление не удалось: {e}"));
            }
        }
    } else {
        notify(
            app,
            "Вышло обновление zapret",
            &format!("Версия {version} готова к установке — открой Zapret Studio"),
        );
    }
}

/// У ByeDPI свой репозиторий и свой файл в релизе, поэтому проверяется отдельно.
async fn background_check_byedpi(app: &AppHandle) {
    let (current, installed, auto_install) = {
        let state = app.state::<AppState>();
        let cfg = state.config();
        (
            cfg.byedpi_version.clone(),
            cfg.byedpi_dir.as_deref().map(byedpi::looks_like_byedpi).unwrap_or(false),
            cfg.auto_install_updates,
        )
    };
    if !installed {
        return;
    }

    let result = byedpi::check(current).await;
    let _ = app.emit("byedpi-update-check", result.clone());
    if !result.has_update {
        return;
    }
    let version = result.latest.clone().unwrap_or_default();

    if auto_install {
        match do_install_byedpi(app, false).await {
            Ok(_) => {
                notify(app, "Zapret Studio", &format!("ByeDPI обновлён до {version}"));
                let _ = app.emit("refresh", ());
            }
            Err(e) => {
                let state = app.state::<AppState>();
                state.runner.log(Some(app), "err", format!("ByeDPI не обновился: {e}"));
            }
        }
    } else {
        notify(
            app,
            "Вышло обновление ByeDPI",
            &format!("Версия {version} готова к установке — открой Zapret Studio"),
        );
    }
}

/// GoodbyeDPI обновляется предрелизами и тоже отдельно от остальных.
async fn background_check_goodbye(app: &AppHandle) {
    let (current, installed, auto_install) = {
        let state = app.state::<AppState>();
        let cfg = state.config();
        (
            cfg.goodbye_version.clone(),
            cfg.goodbye_dir.as_deref().map(goodbye::looks_like_goodbye).unwrap_or(false),
            cfg.auto_install_updates,
        )
    };
    if !installed {
        return;
    }

    let result = goodbye::check(current).await;
    let _ = app.emit("goodbye-update-check", result.clone());
    if !result.has_update {
        return;
    }
    let version = result.latest.clone().unwrap_or_default();

    if auto_install {
        match do_install_goodbye(app, false).await {
            Ok(_) => {
                notify(app, "Zapret Studio", &format!("GoodbyeDPI обновлён до {version}"));
                let _ = app.emit("refresh", ());
            }
            Err(e) => {
                let state = app.state::<AppState>();
                state.runner.log(Some(app), "err", format!("GoodbyeDPI не обновился: {e}"));
            }
        }
    } else {
        notify(
            app,
            "Вышло обновление GoodbyeDPI",
            &format!("Версия {version} готова к установке — открой Zapret Studio"),
        );
    }
}

/// Xray и sing-box проверяются одним кодом: у них и репозитории, и настройки
/// устроены одинаково — различает их только `engine`.
async fn background_check_core(app: &AppHandle, engine: &str) {
    let Ok(spec) = core_spec(engine) else { return };
    let (current, installed, auto_install) = {
        let state = app.state::<AppState>();
        let cfg = state.config();
        let cc = core_cfg(&cfg, engine);
        (
            cc.version.clone(),
            cc.dir.as_deref().map(|d| proxycore::installed(spec, d)).unwrap_or(false),
            cfg.auto_install_updates,
        )
    };
    if !installed {
        return;
    }

    let result = proxycore::check(spec, current).await;
    let _ = app.emit(&format!("{engine}-update-check"), result.clone());
    if !result.has_update {
        return;
    }
    let version = result.latest.clone().unwrap_or_default();

    if auto_install {
        match do_install_core(app, engine, false).await {
            Ok(_) => {
                notify(app, "Zapret Studio", &format!("{} обновлён до {version}", spec.name));
                let _ = app.emit("refresh", ());
            }
            Err(e) => {
                let state = app.state::<AppState>();
                state.runner.log(Some(app), "err", format!("{} не обновился: {e}", spec.name));
            }
        }
    } else {
        notify(
            app,
            &format!("Вышло обновление {}", spec.name),
            &format!("Версия {version} готова к установке — открой Zapret Studio"),
        );
    }
}

/// Сторож. Блокировки меняются молча: вчера стратегия работала, сегодня нет,
/// и человек узнаёт об этом, когда не грузится YouTube. Раз в несколько минут
/// тихо стучимся в пару целей и говорим, если обход перестал пробивать.
///
/// Главное здесь — не поднимать ложную тревогу. Поэтому осмотр пропускается,
/// когда обход выключен, идёт проверка стратегий или качается обновление,
/// а упавшая сеть отличается от упавшего обхода по контрольной точке.
fn spawn_watchdog(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        // Первый осмотр не сразу после запуска: обход должен подняться
        tokio::time::sleep(std::time::Duration::from_secs(90)).await;
        loop {
            let minutes = {
                let state = app.state::<AppState>();
                state.config().watchdog_interval_min.clamp(2, 240)
            };
            watchdog_round(&app).await;
            tokio::time::sleep(std::time::Duration::from_secs(minutes as u64 * 60)).await;
        }
    });
}

async fn watchdog_round(app: &AppHandle) {
    let (enabled, auto_fix, engine, proxy, own, best, current, busy, tun) = {
        let state = app.state::<AppState>();
        let cfg = state.config();
        // В режиме TUN через туннель идёт вообще всё, включая контрольную
        // точку. Значит, «интернета нет» и «обход сломался» перестают
        // различаться — и списывать всё на сеть больше нельзя
        let tun = cfg.engine == "singbox"
            && state
                .core("singbox")
                .and_then(|c| c.current())
                .is_some_and(|id| singbox::is_tun(&id));
        (
            cfg.watchdog,
            cfg.watchdog_auto_fix,
            cfg.engine.clone(),
            state.own_proxy_port(),
            own_targets(&cfg),
            engine_best(&cfg),
            engine_current(&state, &cfg),
            state.testing.load(Ordering::Relaxed) || !state.engine_running(),
            tun,
        )
    };
    // Выключенный обход сторожить нечего, а во время проверки стратегий
    // движки перезапускаются по кругу — там любой осмотр соврёт
    if !enabled || busy {
        return;
    }

    let health = tester::health(proxy, &own).await;
    let _ = app.emit("watchdog", health.clone());

    let state = app.state::<AppState>();
    // Сеть лежит целиком — обход тут ни при чём, молчим. Но только не в
    // режиме TUN: там нечему лежать отдельно от туннеля
    if !health.control_ok && !tun {
        state.runner.log(Some(app), "info", "Сторож: интернет не отвечает, обход не виноват");
        return;
    }

    if health.ok > 0 {
        // Отпустило: сказать об этом не менее важно, чем пожаловаться
        if state.watchdog_fails.swap(0, Ordering::SeqCst) > 0 {
            state.runner.log(Some(app), "info", "Сторож: обход снова пробивает");
            notify(app, "Zapret Studio", "Обход снова работает");
        }
        return;
    }

    let fails = state.watchdog_fails.fetch_add(1, Ordering::SeqCst) + 1;
    let what = health.failed.join(", ");
    state
        .runner
        .log(Some(app), "err", format!("Сторож: обход не пробивает ({what})"));

    if fails == 1 {
        notify(
            app,
            "Обход перестал работать",
            &format!("Не отвечают: {what}. Открой Zapret Studio и запусти проверку"),
        );
    }
    if !auto_fix {
        return;
    }

    // Первый раз — просто поднять заново: чаще всего движок просто умер.
    // Второй раз подряд — значит, дело в самой стратегии, и надо брать ту,
    // которую последняя проверка сочла лучшей
    let switch_to = (fails >= 2)
        .then_some(best)
        .flatten()
        .filter(|b| Some(b) != current.as_ref());
    let handle = app.clone();
    let engine_name = engine_name(&engine);
    std::thread::spawn(move || {
        let state = handle.state::<AppState>();
        // Если человек прямо сейчас сам переключает движок, чинить поверх
        // него нельзя — дожидаемся своей очереди
        let _guard = state.engine_guard();
        let outcome = match &switch_to {
            Some(id) => start_bypass_named(&handle, &state, Some(id.clone()))
                .map(|_| format!("Сторож переключил {engine_name} на «{id}»")),
            None => start_active_engine(&handle, &state)
                .map(|_| format!("Сторож перезапустил {engine_name}")),
        };
        match outcome {
            Ok(message) => {
                state.runner.log(Some(&handle), "info", message.clone());
                notify(&handle, "Zapret Studio", &message);
                let _ = handle.emit("refresh", ());
            }
            Err(e) => state.runner.log(Some(&handle), "err", format!("Сторож не смог починить: {e}")),
        }
    });
}

/// Запуск обхода по имени стратегии или id пресета — то же, что делает
/// команда `start_bypass`, но доступное изнутри.
fn start_bypass_named(
    app: &AppHandle,
    state: &AppState,
    name: Option<String>,
) -> Result<Vec<String>, String> {
    let engine = state.config().engine.clone();
    match engine.as_str() {
        "byedpi" => start_byedpi(app, state, name),
        "goodbyedpi" => start_goodbye(app, state, name),
        e if CORES.contains(&e) => start_core(app, state, e, name),
        _ => start_zapret(app, state, name).map(|_| Vec::new()),
    }
}

/// Обновление приложения только показываем: скачанный установщик запускает
/// человек, а не таймер.
async fn background_check_app(app: &AppHandle) {
    let repo = {
        let state = app.state::<AppState>();
        app_repo(&state.config())
    };
    let result = selfupdate::check(&repo).await;
    let _ = app.emit("app-update-check", result.clone());
    if result.has_update {
        notify(
            app,
            "Вышло обновление Zapret Studio",
            &format!(
                "Версия {} готова к установке — открой настройки",
                result.latest.clone().unwrap_or_default()
            ),
        );
    }
}

/// Раз в 15 минут смотрит, не подошёл ли срок очередной проверки.
/// Сам интервал живёт в настройках, поэтому его смена подхватывается на лету.
fn spawn_update_watcher(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(25)).await;
        loop {
            let due = {
                let state = app.state::<AppState>();
                let cfg = state.config();
                cfg.auto_check_updates && check_due(&cfg)
            };
            if due {
                // Отметку ставим до похода в сеть: если GitHub откажет, круг
                // не должен повторяться каждые 15 минут и добивать лимит
                {
                    let state = app.state::<AppState>();
                    let _ = state.update_config(|cfg| {
                        cfg.last_update_check = Some(sysutil::now_iso())
                    });
                }
                background_check(&app).await;
                background_check_byedpi(&app).await;
                background_check_goodbye(&app).await;
                for engine in CORES {
                    background_check_core(&app, engine).await;
                }
                background_check_app(&app).await;
            }
            tokio::time::sleep(std::time::Duration::from_secs(15 * 60)).await;
        }
    });
}

// ---------------------------------------------------------------- запуск

fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Открыть Zapret Studio", true, None::<&str>)?;
    let stop = MenuItem::with_id(app, "stop", "Выключить обход", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Выход", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &stop, &PredefinedMenuItem::separator(app)?, &quit])?;

    TrayIconBuilder::with_id("main-tray")
        .icon(app.default_window_icon().unwrap().clone())
        .tooltip("Zapret Studio")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main(app),
            "stop" => {
                if let Some(state) = app.try_state::<AppState>() {
                    let _ = stop_others(app, &state, "");
                    let _ = app.emit("refresh", ());
                }
            }
            "quit" => {
                if let Some(state) = app.try_state::<AppState>() {
                    let _ = stop_others(app, &state, "");
                }
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

fn show_main(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let handle = app.handle().clone();
            let data_dir = handle.path().app_data_dir().unwrap_or_else(|_| PathBuf::from("."));
            std::fs::create_dir_all(&data_dir).ok();
            let cfg_path = data_dir.join("config.json");
            let mut cfg = config::load(&cfg_path);

            // Папка могла быть удалена между запусками. Отметку «первый
            // запуск пройден» при этом не сбрасываем: она про человека, а не
            // про папку. Иначе после обновления или переезда папки приложение
            // встречало бы знакомить с собой заново того, кто им давно
            // пользуется, — а это первое, что видишь вместо своей главной
            if let Some(dir) = cfg.zapret_dir.clone() {
                if !sysutil::looks_like_zapret(&dir) {
                    cfg.zapret_dir = None;
                }
            }
            if let Some(dir) = cfg.byedpi_dir.clone() {
                if !byedpi::looks_like_byedpi(&dir) {
                    cfg.byedpi_dir = None;
                }
            }
            if let Some(dir) = cfg.goodbye_dir.clone() {
                if !goodbye::looks_like_goodbye(&dir) {
                    cfg.goodbye_dir = None;
                }
            }
            for engine in CORES {
                let spec = core_spec(engine).expect("движок из списка ядер");
                let cc = core_cfg_mut(&mut cfg, engine);
                if cc.dir.as_deref().is_some_and(|d| !proxycore::installed(spec, d)) {
                    cc.dir = None;
                }
            }
            // Прошлый запуск мог закончиться аварийно и оставить систему с
            // прокси в никуда — без интернета вообще. Возвращаем как было;
            // если ByeDPI нужен, он пропишет прокси заново при старте.
            if sysproxy::is_ours_any(&sysproxy::read()) {
                let _ = sysproxy::restore(cfg.saved_proxy.as_ref());
                cfg.saved_proxy = None;
            }
            cfg.app_autostart = service::autostart_enabled();
            // Ни у ByeDPI, ни у GoodbyeDPI нет своей службы, поэтому
            // «включать вместе с Windows» для них — это поднять обход при старте
            let autostart_engine = match cfg.engine.as_str() {
                "byedpi" => cfg.byedpi_autostart,
                "goodbyedpi" => cfg.goodbye_autostart,
                e if CORES.contains(&e) => core_cfg(&cfg, e).autostart,
                _ => false,
            };
            let (xray_port, singbox_port) = (core_port(&cfg, "xray"), core_port(&cfg, "singbox"));

            app.manage(AppState {
                cfg: Mutex::new(cfg),
                cfg_path: Mutex::new(cfg_path),
                data_dir: Mutex::new(data_dir),
                runner: Arc::new(Runner::new()),
                proxy: Arc::new(byedpi::Proxy::new()),
                goodbye: Arc::new(goodbye::Engine::new()),
                xray: Arc::new(proxycore::Core::new(xray::SPEC, xray_port)),
                singbox: Arc::new(proxycore::Core::new(singbox::SPEC, singbox_port)),
                cancel: Arc::new(AtomicBool::new(false)),
                install_cancel: Arc::new(AtomicBool::new(false)),
                testing: Arc::new(AtomicBool::new(false)),
                watchdog_fails: Arc::new(AtomicUsize::new(0)),
                engine_lock: Mutex::new(()),
            });

            setup_tray(&handle)?;
            spawn_update_watcher(handle.clone());
            spawn_watchdog(handle.clone());

            if autostart_engine {
                let h = handle.clone();
                std::thread::spawn(move || {
                    let state = h.state::<AppState>();
                    match start_active_engine(&h, &state) {
                        Ok(messages) => {
                            for m in messages {
                                state.runner.log(Some(&h), "info", m);
                            }
                            let _ = h.emit("refresh", ());
                        }
                        Err(e) => {
                            state.runner.log(Some(&h), "err", format!("Автозапуск обхода не удался: {e}"))
                        }
                    }
                });
            }

            let hidden = std::env::args().any(|a| a == "--tray");
            if let Some(win) = app.get_webview_window("main") {
                if hidden {
                    let _ = win.hide();
                }
                let handle2 = handle.clone();
                win.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        if let Some(w) = handle2.get_webview_window("main") {
                            let _ = w.hide();
                        }
                    }
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            snapshot,
            set_zapret_dir,
            check_update,
            install_zapret,
            cancel_install,
            select_strategy,
            set_engine,
            set_byedpi_dir,
            check_byedpi_update,
            install_byedpi,
            select_preset,
            save_preset,
            delete_preset,
            set_byedpi_port,
            set_system_proxy,
            run_preset_tests,
            set_custom_targets,
            check_app_update,
            install_app_update,
            health_check,
            export_profile,
            import_profile,
            set_goodbye_dir,
            check_goodbye_update,
            install_goodbye,
            update_goodbye_blacklist,
            set_core_dir,
            check_core_update,
            install_core,
            set_core_port,
            set_core_server,
            load_subscription,
            select_server,
            ping_servers,
            start_bypass,
            stop_bypass,
            run_tests,
            cancel_tests,
            install_service,
            remove_service,
            set_app_autostart,
            set_option,
            ping,
            measure_speed,
            get_logs,
            clear_logs,
            diagnostics,
            stop_stray,
            warp_connect,
            warp_disconnect,
            warp_check,
            open_url,
            stop_conflicts,
            set_game_filter,
            set_ipset_mode,
            update_ipset,
            set_active_fake,
            clear_discord_cache,
            hosts_status,
            apply_hosts,
            open_folder,
            quit_app
        ])
        .run(tauri::generate_context!())
        .expect("не удалось запустить Zapret Studio");
}
