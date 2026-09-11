use crate::sysutil::{out_text, run_hidden};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConflictItem {
    /// service | process
    pub kind: String,
    pub name: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    pub title: String,
    /// ok | warn | fail
    pub level: String,
    pub detail: String,
    pub hint: Option<String>,
    /// Что именно можно остановить кнопкой — заполнено только у проверки конфликтов
    #[serde(default)]
    pub items: Vec<ConflictItem>,
}

const CONFLICT_PROCESSES: &[&str] = &[
    "goodbyedpi.exe",
    "winws2.exe",
    "zapret.exe",
    "ByeDPI.exe",
    "ciadpi.exe",
    "spoofdpi.exe",
    // Прокси-ядра из чужих оболочек (v2rayN, Nekoray, Hiddify): они занимают
    // те же порты и уводят трафик в свой туннель — проверка тогда мерит его,
    // а не обход. Свои копии сюда не попадают, их отсеивает own_processes.
    "xray.exe",
    "sing-box.exe",
];

const CONFLICT_SERVICES: &[&str] = &[
    "GoodbyeDPI",
    "ZapretDirectService",
    "ZapretTwo",
    "ByeDPI",
];

// Служб WinDivert (`WinDivert`, `WinDivert1.4`) в этом списке нет намеренно:
// это не чужая программа, а общий драйвер, на котором работают и zapret, и
// GoodbyeDPI. Остановить его кнопкой «Остановить чужие обходы» — значит убить
// собственный обход, что раньше и происходило.

/// Все запущенные процессы одним вызовом. Раньше на каждое имя из списка
/// шёл свой `tasklist` — восемь запусков процессов только на проверку
/// конфликтов, и так на каждый снимок состояния. На Windows запуск процесса
/// стоит десятки миллисекунд, и это было заметно глазом.
pub fn running_processes() -> Vec<String> {
    let Ok(out) = run_hidden("tasklist", &["/NH", "/FO", "CSV"]) else {
        return Vec::new();
    };
    out_text(&out)
        .lines()
        // Строка вида "chrome.exe","1234","Console","1","123 456 КБ"
        .filter_map(|l| l.trim().strip_prefix('"'))
        .filter_map(|l| l.split('"').next())
        .map(str::to_lowercase)
        .collect()
}

fn tasklist_has(name: &str) -> bool {
    run_hidden("tasklist", &["/FI", &format!("IMAGENAME eq {name}"), "/NH"])
        .map(|o| out_text(&o).to_lowercase().contains(&name.to_lowercase()))
        .unwrap_or(false)
}

/// Мешает не сам факт установки, а работа: остановленная служба, переведённая
/// в ручной запуск, никого не трогает. Раньше приложение считало конфликтом
/// её существование — и красная плашка не гасла даже после «Остановить».
fn service_conflicts(name: &str) -> bool {
    let Ok(out) = run_hidden("sc", &["query", name]) else { return false };
    if !out.status.success() {
        return false;
    }
    let text = out_text(&out);
    let running = text.contains("RUNNING") || text.contains("START_PENDING");
    // Остановленная, но с автозапуском — вернётся после перезагрузки,
    // так что предупредить о ней всё-таки стоит
    running
        || run_hidden("sc", &["qc", name])
            .map(|o| out_text(&o).contains("AUTO_START"))
            .unwrap_or(false)
}

/// Другие обходы блокировок, которые мешают работать нашему.
///
/// `own` — процессы, которые запустили мы сами (ciadpi.exe, goodbyedpi.exe):
/// жаловаться на них, а тем более предлагать их остановить, было бы странно.
pub fn scan_conflicts(own: &[&str]) -> Vec<ConflictItem> {
    let mut found = Vec::new();
    for s in CONFLICT_SERVICES {
        if service_conflicts(s) {
            found.push(ConflictItem { kind: "service".into(), name: s.to_string() });
        }
    }
    let running = running_processes();
    for p in CONFLICT_PROCESSES {
        if own.iter().any(|o| o.eq_ignore_ascii_case(p)) {
            continue;
        }
        if running.iter().any(|r| r == &p.to_lowercase()) {
            found.push(ConflictItem { kind: "process".into(), name: p.to_string() });
        }
    }
    found
}

/// Останавливает конфликтующие обходы: службы — стоп плюс ручной запуск,
/// чтобы не вернулись после перезагрузки; процессы — завершение.
/// Сначала службы: иначе менеджер службы (nssm) тут же перезапустит убитый процесс.
pub fn stop_conflicts(items: &[ConflictItem]) -> Vec<String> {
    let mut report = Vec::new();
    for it in items.iter().filter(|i| i.kind == "service") {
        let stop = run_hidden("sc", &["stop", &it.name]);
        let cfg = run_hidden("sc", &["config", &it.name, "start=", "demand"]);
        let stopped = stop.as_ref().map(|o| o.status.success()).unwrap_or(false);
        let reconfigured = cfg.as_ref().map(|o| o.status.success()).unwrap_or(false);
        report.push(match (stopped, reconfigured) {
            (true, true) => format!("Служба {} остановлена, автозапуск отключён", it.name),
            (true, false) => format!("Служба {} остановлена, но автозапуск отключить не удалось", it.name),
            (false, true) => format!("Служба {} уже не работала, автозапуск отключён", it.name),
            (false, false) => format!("Служба {}: не удалось ни остановить, ни отключить", it.name),
        });
    }
    // Даём службам время завершить свои процессы, потом добиваем остатки
    std::thread::sleep(std::time::Duration::from_millis(1200));
    for it in items.iter().filter(|i| i.kind == "process") {
        let killed = run_hidden("taskkill", &["/F", "/IM", &it.name])
            .map(|o| o.status.success())
            .unwrap_or(false);
        report.push(if killed || !tasklist_has(&it.name) {
            format!("Процесс {} завершён", it.name)
        } else {
            format!("Процесс {} завершить не удалось", it.name)
        });
    }
    report
}

/// `own_proxy` — порт нашего ByeDPI, если он сейчас работает, `own` — имена
/// наших собственных процессов: с ними часть проверок смотрит на систему иначе.
pub fn run(root: Option<&Path>, own_proxy: Option<u16>, own: &[&str]) -> Vec<Check> {
    let mut checks: Vec<Check> = Vec::new();
    let plain = |title: &str, level: &str, detail: String, hint: Option<String>| Check {
        title: title.into(),
        level: level.into(),
        detail,
        hint,
        items: Vec::new(),
    };

    // 1. Драйвер WinDivert на месте
    match root {
        Some(dir) if dir.join("bin").join("WinDivert64.sys").is_file() => checks.push(plain(
            "Драйвер WinDivert",
            "ok",
            "Файл WinDivert64.sys на месте".into(),
            None,
        )),
        Some(_) => checks.push(plain(
            "Драйвер WinDivert",
            "fail",
            "WinDivert64.sys отсутствует в папке bin".into(),
            Some("Скорее всего его удалил антивирус. Добавь папку в исключения и переустанови zapret.".into()),
        )),
        None => {}
    }

    // 2. Кириллица в пути ломает запуск winws.exe
    if let Some(dir) = root {
        let path = dir.display().to_string();
        let has_cyrillic = path.chars().any(|c| ('А'..='я').contains(&c) || c == 'ё' || c == 'Ё');
        checks.push(plain(
            "Путь к zapret",
            if has_cyrillic { "warn" } else { "ok" },
            path,
            has_cyrillic.then(|| {
                "В пути есть кириллица — часть стратегий может не запуститься. Лучше перенести папку в путь из латинских букв.".to_string()
            }),
        ));
    }

    // 3. Конфликтующие обходы
    let conflicts = scan_conflicts(own);
    checks.push(if conflicts.is_empty() {
        plain(
            "Конфликты с другими обходами",
            "ok",
            "Других программ обхода не найдено".into(),
            None,
        )
    } else {
        let names: Vec<String> = conflicts
            .iter()
            .map(|c| if c.kind == "service" { format!("служба {}", c.name) } else { c.name.clone() })
            .collect();
        Check {
            title: "Конфликты с другими обходами".into(),
            level: "fail".into(),
            detail: format!("Обнаружено: {}", names.join(", ")),
            hint: Some("Два обхода одновременно мешают друг другу, а если оба на драйвере WinDivert — ещё и делят его. Останови лишние: свой обход приложение поднимет заново само.".into()),
            items: conflicts,
        }
    });

    // 4. Ответы DNS и антивирус — до проверок про сам драйвер: если врёт DNS
    // или драйвер съели, всё остальное уже неважно
    checks.push(dns_substitution());
    checks.push(antivirus());

    // 5. Драйвер WinDivert живёт службой, пока им кто-то пользуется.
    // Это не конфликт: на нём работают и zapret, и GoodbyeDPI.
    if let Some(name) = ["WinDivert", "WinDivert1.4"]
        .into_iter()
        .find(|n| run_hidden("sc", &["query", n]).map(|o| o.status.success()).unwrap_or(false))
    {
        checks.push(plain(
            "Служба драйвера WinDivert",
            "ok",
            format!("Загружена как {name}"),
            None,
        ));
    }

    // 5. TCP timestamps — нужны части стратегий (--dpi-desync-fooling=ts)
    let ts_ok = run_hidden("netsh", &["interface", "tcp", "show", "global"])
        .map(|o| {
            let t = out_text(&o).to_lowercase();
            t.contains("enabled") || t.contains("включен")
        })
        .unwrap_or(false);
    checks.push(plain(
        "TCP timestamps",
        if ts_ok { "ok" } else { "warn" },
        if ts_ok { "Включены".into() } else { "Похоже, выключены".into() },
        (!ts_ok).then(|| "Приложение включает их автоматически при каждом запуске обхода.".to_string()),
    ));

    // 6. Системный прокси: для zapret он помеха, для ByeDPI — наоборот, способ
    // доставки трафика, поэтому свой собственный прокси не ругаем
    let proxy = crate::sysproxy::read();
    let ours = own_proxy.map(|port| crate::sysproxy::is_ours(&proxy, port)).unwrap_or(false);
    checks.push(match (ours, proxy.enabled) {
        (true, _) => plain(
            "Системный прокси",
            "ok",
            format!("Направлен в ByeDPI: {}", proxy.server),
            None,
        ),
        (false, true) => plain(
            "Системный прокси",
            "warn",
            format!("Включён в настройках Windows: {}", proxy.server),
            Some("Чужой прокси или VPN уводит трафик мимо обхода — результаты проверок будут недостоверными.".into()),
        ),
        (false, false) => plain("Системный прокси", "ok", "Выключен".into(), None),
    });

    // 7. Служба базовой фильтрации, без неё WinDivert не поднимется
    let bfe_ok = run_hidden("sc", &["query", "BFE"])
        .map(|o| out_text(&o).contains("RUNNING"))
        .unwrap_or(false);
    checks.push(plain(
        "Служба фильтрации BFE",
        if bfe_ok { "ok" } else { "fail" },
        if bfe_ok { "Работает".into() } else { "Не запущена".into() },
        (!bfe_ok).then(|| "Base Filtering Engine обязателен для WinDivert. Запусти службу BFE в services.msc.".to_string()),
    ));

    // 8. WARP уводит в туннель весь трафик, включая наши проверки
    let warp = crate::warp::status();
    if warp.installed {
        checks.push(plain(
            "Cloudflare WARP",
            if warp.connected { "warn" } else { "ok" },
            warp.detail.clone(),
            warp.connected.then(|| {
                "Пока WARP подключён, весь трафик идёт через него — проверка покажет качество туннеля, а не обхода. Для честного замера WARP лучше отключить.".to_string()
            }),
        ));
    }

    checks
}

/// Адреса, которые у Cloudflare и Google не меняются годами. По ним видно,
/// подменяет ли провайдер ответы DNS: настоящий ответ известен заранее.
const DNS_ANCHORS: &[(&str, &[&str])] = &[
    ("one.one.one.one", &["1.1.1.1", "1.0.0.1"]),
    ("dns.google", &["8.8.8.8", "8.8.4.4"]),
];

/// Сверяет системный резолвер с тем, что заведомо верно. Провайдеры,
/// подменяющие DNS, обычно отвечают адресом своей заглушки — и тогда обход
/// бессилен: браузер идёт не туда ещё до всякого DPI.
pub fn dns_substitution() -> Check {
    use std::net::ToSocketAddrs;

    let mut checked = 0;
    let mut wrong: Vec<String> = Vec::new();
    for (host, expected) in DNS_ANCHORS {
        let Ok(addrs) = (*host, 443u16).to_socket_addrs() else { continue };
        let got: Vec<String> = addrs.map(|a| a.ip().to_string()).collect();
        if got.is_empty() {
            continue;
        }
        checked += 1;
        // Хотя бы один настоящий адрес — значит, резолвер отвечает честно
        if !got.iter().any(|ip| expected.contains(&ip.as_str())) {
            wrong.push(format!("{host} → {}", got.join(", ")));
        }
    }

    if checked == 0 {
        return Check {
            title: "Ответы DNS".into(),
            level: "warn".into(),
            detail: "Имена не разрешаются вообще — проверить нечего".into(),
            hint: Some("Похоже, DNS не работает: без него не откроется ни один сайт, независимо от обхода.".into()),
            items: Vec::new(),
        };
    }
    if wrong.is_empty() {
        return Check {
            title: "Ответы DNS".into(),
            level: "ok".into(),
            detail: format!("Проверено имён: {checked}, адреса настоящие"),
            hint: None,
            items: Vec::new(),
        };
    }
    Check {
        title: "Ответы DNS".into(),
        level: "fail".into(),
        detail: format!("Подменены: {}", wrong.join("; ")),
        hint: Some(
            "Провайдер отвечает на запросы DNS чужими адресами — браузер уходит не туда ещё до DPI, и обход тут не поможет. Настройки → Система → DNS: там это чинится одной кнопкой."
                .into(),
        ),
        items: Vec::new(),
    }
}

/// Антивирусы регулярно принимают WinDivert за вредоносный драйвер и тихо
/// удаляют его или блокируют загрузку. Выглядит это как «zapret не
/// запускается» без всяких объяснений, и догадаться почти невозможно.
pub fn antivirus() -> Check {
    let out = run_hidden(
        "powershell",
        &[
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Get-CimInstance -Namespace root/SecurityCenter2 -ClassName AntiVirusProduct | Select-Object -ExpandProperty displayName",
        ],
    );
    let names: Vec<String> = match &out {
        Ok(o) if o.status.success() => out_text(o)
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect(),
        _ => Vec::new(),
    };
    if names.is_empty() {
        return Check {
            title: "Антивирус".into(),
            level: "ok".into(),
            detail: "Сторонних антивирусов не видно".into(),
            hint: None,
            items: Vec::new(),
        };
    }
    // Защитник Windows с WinDivert уживается, чужие — через раз
    let third_party: Vec<&String> =
        names.iter().filter(|n| !n.to_lowercase().contains("defender")).collect();
    if third_party.is_empty() {
        return Check {
            title: "Антивирус".into(),
            level: "ok".into(),
            detail: names.join(", "),
            hint: None,
            items: Vec::new(),
        };
    }
    Check {
        title: "Антивирус".into(),
        level: "warn".into(),
        detail: third_party.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "),
        hint: Some(
            "Сторонние антивирусы регулярно принимают драйвер WinDivert за вредоносный: удаляют файл или не дают ему загрузиться, а выглядит это как «zapret не запускается». Если обход не поднимается — добавь папку zapret в исключения."
                .into(),
        ),
        items: Vec::new(),
    }
}

/// Проверяет, есть ли у машины живой IPv6. Отдельно от `run`, потому что
/// требует сети.
pub async fn ipv6_leak() -> Check {
    // Обычное TCP-соединение по литеральному адресу: без DNS и без TLS, так
    // что ответ однозначный — есть маршрут по IPv6 или нет
    let reachable = tokio::time::timeout(
        std::time::Duration::from_secs(4),
        tokio::net::TcpStream::connect("[2606:4700:4700::1111]:443"),
    )
    .await
    .map(|r| r.is_ok())
    .unwrap_or(false);

    if !reachable {
        return Check {
            title: "IPv6".into(),
            level: "ok".into(),
            detail: "Не работает — весь трафик идёт по IPv4".into(),
            hint: None,
            items: Vec::new(),
        };
    }
    Check {
        title: "IPv6".into(),
        level: "warn".into(),
        detail: "Работает — часть трафика может пойти мимо обхода".into(),
        hint: Some(
            "Стратегии обхода настроены в основном на IPv4. Если сайт открывается по IPv6, DPI видит его как есть, и обход не срабатывает — это частая причина «обход включён, а сайт не открывается». Проверь: если без IPv6 сайт открывается, отключи его в свойствах сетевого адаптера."
                .into(),
        ),
        items: Vec::new(),
    }
}

/// Куда провайдер видит наш трафик на самом деле. Отдельно от `run`, потому
/// что требует сети: если тут чужая страна, значит трафик уже идёт через VPN,
/// и любая стратегия покажет отличный результат независимо от обхода.
pub async fn external_address() -> Check {
    let fail = |detail: String| Check {
        title: "Внешний адрес".into(),
        level: "warn".into(),
        detail,
        hint: Some("Без этого не понять, меряет проверка обход или чужой туннель.".into()),
        items: Vec::new(),
    };

    let client = match reqwest::Client::builder()
        .user_agent("ZapretStudio/1.0")
        .timeout(std::time::Duration::from_secs(8))
        .build()
    {
        Ok(c) => c,
        Err(e) => return fail(e.to_string()),
    };
    let text = match client.get("https://www.cloudflare.com/cdn-cgi/trace").send().await {
        Ok(r) => match r.text().await {
            Ok(t) => t,
            Err(e) => return fail(format!("ответ не прочитан: {e}")),
        },
        Err(_) => return fail("Cloudflare не ответил — проверить не удалось".into()),
    };

    let field = |key: &str| {
        text.lines()
            .find_map(|l| l.strip_prefix(key))
            .map(|v| v.trim().to_string())
            .unwrap_or_default()
    };
    let (ip, loc, warp) = (field("ip="), field("loc="), field("warp="));
    let tunnelled = warp == "on" || warp == "plus";

    Check {
        title: "Внешний адрес".into(),
        level: if tunnelled { "warn".into() } else { "ok".into() },
        detail: format!(
            "{ip}{}{}",
            if loc.is_empty() { String::new() } else { format!(" · страна {loc}") },
            if tunnelled { " · через WARP" } else { "" }
        ),
        hint: Some(if tunnelled {
            "Трафик идёт через WARP — проверка меряет туннель, а не обход.".into()
        } else {
            "Если страна не твоя, трафик уже идёт через VPN или прокси — тогда проверка стратегий меряет их, а не обход.".to_string()
        }),
        items: Vec::new(),
    }
}
