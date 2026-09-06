use crate::runner::Runner;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

/// Чего ждём от ответа. Без этого «доступен» значит лишь «что-то ответило»:
/// заглушка провайдера, страница блокировки и 404 от чужого сервера выглядят
/// для программы точно так же, как настоящий сервис.
#[derive(Clone)]
pub enum Expect {
    /// Ровно такой код ответа
    Status(u16),
    /// Подстрока в заголовках ответа, регистр не важен: "cf-ray", "server: esf"
    Header(String),
    /// Подстрока в теле — самая строгая проверка
    Body(String),
    /// Любой ответ по TLS. Для своих сайтов: как выглядит их настоящий ответ,
    /// приложение знать не может, а рвут именно рукопожатие — значит,
    /// сам факт ответа уже кое-что говорит. Заглушку провайдера ловим
    /// отдельно, по знакомым словам в теле.
    Any,
}

/// Что именно проверяем.
/// Http — TLS-рукопожатие плюс ответ, который мы умеем узнать.
/// Ws — рукопожатие WebSocket: именно им живёт приложение Discord, и именно
/// его DPI рвёт чаще всего, оставляя сайт доступным.
/// Tcp — контрольная точка «интернет вообще жив».
#[derive(Clone)]
pub enum Probe {
    Http(String, Expect),
    Ws(String),
    Tcp(String, u16),
}

#[derive(Clone)]
pub struct Target {
    pub id: String,
    pub group: String,
    pub label: String,
    pub probe: Probe,
}

fn target(id: &str, group: &str, label: &str, probe: Probe) -> Target {
    Target { id: id.into(), group: group.into(), label: label.into(), probe }
}

fn http(url: &str, expect: Expect) -> Probe {
    Probe::Http(url.into(), expect)
}

fn head(needle: &str) -> Expect {
    Expect::Header(needle.into())
}

fn body(needle: &str) -> Expect {
    Expect::Body(needle.into())
}

/// Адреса и признаки подобраны по живым ответам: у Discord — его собственный
/// API и гейтвей, у YouTube — то, что дёргает плеер, у Cloudflare — хосты,
/// без которых не поднимается WARP.
pub fn builtin() -> Vec<Target> {
    vec![
        target("discord-api", "Discord", "discord.com/api (gateway)",
            http("https://discord.com/api/v10/gateway", body("wss://"))),
        target("discord-ws", "Discord", "gateway.discord.gg (WebSocket)",
            Probe::Ws("https://gateway.discord.gg/?v=10&encoding=json".into())),
        target("discord-updates", "Discord", "updates.discord.com",
            http("https://updates.discord.com/distributions/app/manifests/latest?channel=stable&platform=win&arch=x64",
                body("host_version"))),
        target("discord-cdn", "Discord", "cdn.discordapp.com",
            http("https://cdn.discordapp.com/embed/avatars/0.png", head("content-type: image/png"))),
        target("yt", "YouTube", "www.youtube.com",
            http("https://www.youtube.com/generate_204", Expect::Status(204))),
        target("yt-page", "YouTube", "страница youtube.com",
            http("https://www.youtube.com/", head("server: esf"))),
        target("ytimg", "YouTube", "i.ytimg.com",
            http("https://i.ytimg.com/vi/dQw4w9WgXcQ/default.jpg", head("content-type: image/jpeg"))),
        target("ytvideo", "YouTube", "redirector.googlevideo.com",
            http("https://redirector.googlevideo.com/videoplayback", head("server: clientmapserver"))),
        target("google", "Google", "www.google.com",
            http("https://www.google.com/generate_204", Expect::Status(204))),
        target("gstatic", "Google", "www.gstatic.com",
            http("https://www.gstatic.com/generate_204", Expect::Status(204))),
        target("cf-trace", "Cloudflare", "www.cloudflare.com",
            http("https://www.cloudflare.com/cdn-cgi/trace", body("fl="))),
        target("cf-warp", "Cloudflare", "engage.cloudflareclient.com (WARP)",
            http("https://engage.cloudflareclient.com/", head("cf-ray"))),
        target("cf-dns", "Cloudflare", "cloudflare-dns.com",
            http("https://cloudflare-dns.com/dns-query?name=example.com", head("cf-ray"))),
        target("dns1", "Связь", "1.1.1.1:53", Probe::Tcp("1.1.1.1".into(), 53)),
        target("dns2", "Связь", "8.8.8.8:53", Probe::Tcp("8.8.8.8".into(), 53)),
    ]
}

/// Группа, в которую попадают сайты, добавленные пользователем.
pub const OWN_GROUP: &str = "Свои";

/// Разбирает строку своего сайта: `example.com`, `https://example.com/path`
/// или `host:443`. Пустые строки и комментарии пропускаем — файл
/// `utils/targets.txt` люди ведут руками.
pub fn parse_own(raw: &str) -> Option<Target> {
    let raw = raw.trim();
    if raw.is_empty() || raw.starts_with('#') || raw.contains(char::is_whitespace) {
        return None;
    }
    let id = format!("own:{raw}");
    if raw.contains("://") {
        return Some(target(&id, OWN_GROUP, raw, http(raw, Expect::Any)));
    }
    // `host:port` — проверяем, что соединение вообще устанавливается
    if let Some((host, port)) = raw.rsplit_once(':') {
        if let Ok(port) = port.parse::<u16>() {
            if !host.is_empty() && !host.contains('/') && port > 0 {
                return Some(target(&id, OWN_GROUP, raw, Probe::Tcp(host.into(), port)));
            }
        }
    }
    Some(target(&id, OWN_GROUP, raw, http(&format!("https://{raw}/"), Expect::Any)))
}

pub fn own(list: &[String]) -> Vec<Target> {
    list.iter().filter_map(|raw| parse_own(raw)).collect()
}

/// Полный список целей: встроенные плюс свои. Свои идут последними, чтобы
/// привычные группы не съезжали.
pub fn all(custom: &[String]) -> Vec<Target> {
    let mut list = builtin();
    list.extend(own(custom));
    list
}

/// Группы в том порядке, в каком их показывает интерфейс.
pub const GROUPS: &[&str] = &["Discord", "YouTube", "Google", "Cloudflare", "Связь", OWN_GROUP];

const SPEED_URL: &str = "https://i.ytimg.com/vi/dQw4w9WgXcQ/maxresdefault.jpg";

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TargetResult {
    pub id: String,
    pub group: String,
    pub label: String,
    pub ok: bool,
    pub status: Option<u16>,
    pub ms: Option<u64>,
    pub error: Option<String>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GroupScore {
    pub group: String,
    pub ok: usize,
    pub total: usize,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct StrategyResult {
    pub strategy: String,
    pub label: String,
    pub baseline: bool,
    pub started: bool,
    pub error: Option<String>,
    pub ok: usize,
    pub total: usize,
    pub score: f64,
    pub avg_ms: Option<u64>,
    pub speed_kbs: Option<f64>,
    pub groups: Vec<GroupScore>,
    pub targets: Vec<TargetResult>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TestStage {
    pub index: usize,
    pub total: usize,
    pub strategy: String,
    pub label: String,
    pub phase: String,
}

/// `proxy` — порт локального SOCKS5 ByeDPI, если проверяем его пресеты.
/// Для zapret и для замера «без обхода» прокси отключён явно: иначе замер
/// поехал бы через системный прокси, если тот включён.
fn client(proxy: Option<u16>) -> Result<reqwest::Client, String> {
    let base = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) ZapretStudio/1.0")
        .timeout(Duration::from_secs(7))
        .connect_timeout(Duration::from_secs(4))
        .pool_max_idle_per_host(0)
        .redirect(reqwest::redirect::Policy::none());
    let base = match proxy {
        // socks5h — имя сайта разрешает сам прокси, а значит ByeDPI видит SNI
        // и может его ломать
        Some(port) => base.proxy(
            reqwest::Proxy::all(format!("socks5h://127.0.0.1:{port}")).map_err(|e| e.to_string())?,
        ),
        None => base.no_proxy(),
    };
    base.build().map_err(|e| e.to_string())
}

/// TCP-проверка через SOCKS5: без неё «Связь» у ByeDPI мерила бы прямое
/// соединение и всегда была бы зелёной, что бы ни творил прокси.
async fn socks5_connect(port: u16, host: &str, dest_port: u16) -> Result<(), String> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut s = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .map_err(|e| format!("прокси не отвечает: {e}"))?;
    s.write_all(&[5, 1, 0]).await.map_err(|e| e.to_string())?;
    let mut hello = [0u8; 2];
    s.read_exact(&mut hello).await.map_err(|e| e.to_string())?;
    if hello != [5, 0] {
        return Err("прокси не принял рукопожатие SOCKS5".into());
    }

    let mut req = vec![5u8, 1, 0];
    match host.parse::<std::net::Ipv4Addr>() {
        Ok(ip) => {
            req.push(1);
            req.extend_from_slice(&ip.octets());
        }
        Err(_) => {
            req.push(3);
            req.push(host.len() as u8);
            req.extend_from_slice(host.as_bytes());
        }
    }
    req.extend_from_slice(&dest_port.to_be_bytes());
    s.write_all(&req).await.map_err(|e| e.to_string())?;

    let mut head = [0u8; 4];
    s.read_exact(&mut head).await.map_err(|e| e.to_string())?;
    if head[1] != 0 {
        return Err("прокси не смог соединиться".into());
    }
    Ok(())
}

/// Куда стучимся для замера задержки: anycast-адрес Cloudflare отвечает
/// отовсюду и не требует разрешения имени — иначе в числе была бы ещё и
/// задержка DNS, которая к каналу отношения не имеет.
const PING_HOST: (&str, u16) = ("1.1.1.1", 443);

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct NetSample {
    /// Задержка в миллисекундах, None — не достучались
    pub ms: Option<u64>,
    /// Через что мерили: у ByeDPI это сам прокси, у остальных — напрямую
    pub via: String,
}

fn via_label(proxy: Option<u16>) -> String {
    match proxy {
        Some(port) => format!("через ByeDPI · 127.0.0.1:{port}"),
        None => "напрямую".into(),
    }
}

/// Одиночный замер задержки — то, что главный экран повторяет раз в пару секунд.
pub async fn ping(proxy: Option<u16>) -> NetSample {
    let started = Instant::now();
    let connect = async {
        match proxy {
            Some(p) => socks5_connect(p, PING_HOST.0, PING_HOST.1).await,
            None => tokio::net::TcpStream::connect(format!("{}:{}", PING_HOST.0, PING_HOST.1))
                .await
                .map(|_| ())
                .map_err(|e| e.to_string()),
        }
    };
    let ok = matches!(tokio::time::timeout(Duration::from_secs(3), connect).await, Ok(Ok(())));
    NetSample {
        ms: ok.then(|| started.elapsed().as_millis() as u64),
        via: via_label(proxy),
    }
}

/// Замер скорости по запросу. Постоянно его гонять нельзя: он сам занимает
/// канал и портит и себя, и всё остальное.
pub async fn speed(proxy: Option<u16>) -> Result<f64, String> {
    let client = client(proxy)?;
    measure_speed(&client).await.ok_or_else(|| "не удалось скачать пробный файл".to_string())
}

fn describe(err: &reqwest::Error) -> String {
    if err.is_timeout() {
        return "таймаут — трафик режется".into();
    }
    if err.is_connect() {
        return "соединение не установлено".into();
    }
    let text = err.to_string().to_lowercase();
    if text.contains("reset") {
        return "соединение сброшено (RST)".into();
    }
    if text.contains("dns") || text.contains("resolve") {
        return "имя не разрешается (DNS)".into();
    }
    "ошибка соединения".into()
}

/// Слова, по которым узнаётся страница блокировки. Заглушки провайдеров
/// отвечают обычным кодом 200, и без этого списка «свой сайт» засчитывался бы
/// доступным ровно тогда, когда его как раз и заблокировали.
const STUB_MARKERS: &[&str] = &[
    "доступ ограничен",
    "доступ к запрашиваемому ресурсу закрыт",
    "запрещена на территории",
    "единый реестр",
    "eais.rkn.gov.ru",
    "zapret-info",
    "blocked.",
];

fn looks_like_stub(body: &str) -> bool {
    let lower = body.to_lowercase();
    STUB_MARKERS.iter().any(|m| lower.contains(m))
}

/// Проверяет, что ответ похож на настоящий, и объясняет словами, если нет.
async fn matches(resp: reqwest::Response, expect: &Expect) -> (bool, u16, Option<String>) {
    let code = resp.status().as_u16();
    match expect {
        Expect::Status(want) => {
            if code == *want {
                (true, code, None)
            } else {
                (false, code, Some(format!("ответ {code} вместо {want} — отвечает не тот сервер")))
            }
        }
        Expect::Header(needle) => {
            let joined: String = resp
                .headers()
                .iter()
                .map(|(k, v)| format!("{k}: {}\n", v.to_str().unwrap_or_default()))
                .collect::<String>()
                .to_lowercase();
            if joined.contains(needle.as_str()) {
                (true, code, None)
            } else {
                (false, code, Some(format!("в ответе нет признака «{needle}» — отвечает не тот сервер")))
            }
        }
        Expect::Body(needle) => match resp.text().await {
            Ok(body) if body.contains(needle.as_str()) => (true, code, None),
            Ok(_) => (false, code, Some(format!("в ответе нет «{needle}» — похоже на заглушку"))),
            Err(e) => (false, code, Some(describe(&e))),
        },
        // Свой сайт: рукопожатие прошло и сервер что-то ответил. Настоящий
        // это ответ или заглушка — различаем по тексту, больше нам не на что
        // опереться
        Expect::Any => match resp.text().await {
            Ok(body) if looks_like_stub(&body) => {
                (false, code, Some("ответила страница блокировки провайдера".into()))
            }
            Ok(_) => (true, code, None),
            // Заголовки пришли, тело оборвалось — для DPI это тоже сигнал
            Err(e) => (false, code, Some(describe(&e))),
        },
    }
}

async fn probe_one(client: &reqwest::Client, t: &Target, proxy: Option<u16>) -> TargetResult {
    let started = Instant::now();
    let (ok, status, error) = match &t.probe {
        Probe::Http(url, expect) => {
            // Тело ограничиваем: нам нужен признак, а не вся страница
            match client.get(url.as_str()).header("Range", "bytes=0-4095").send().await {
                Ok(r) => {
                    let (ok, code, err) = matches(r, expect).await;
                    (ok, Some(code), err)
                }
                Err(e) => (false, None, Some(describe(&e))),
            }
        }
        Probe::Ws(url) => {
            // Полноценное рукопожатие: сервер обязан ответить 101
            match client
                .get(url.as_str())
                .header("Connection", "Upgrade")
                .header("Upgrade", "websocket")
                .header("Sec-WebSocket-Version", "13")
                .header("Sec-WebSocket-Key", "dGhlIHNhbXBsZSBub25jZQ==")
                .send()
                .await
            {
                Ok(r) if r.status().as_u16() == 101 => (true, Some(101), None),
                Ok(r) => (
                    false,
                    Some(r.status().as_u16()),
                    Some("сервер не согласился на WebSocket — соединение рвут".into()),
                ),
                Err(e) => (false, None, Some(describe(&e))),
            }
        }
        Probe::Tcp(host, port) => {
            let connect = async {
                match proxy {
                    Some(p) => socks5_connect(p, host, *port).await,
                    None => tokio::net::TcpStream::connect(format!("{host}:{port}"))
                        .await
                        .map(|_| ())
                        .map_err(|e| e.to_string()),
                }
            };
            match tokio::time::timeout(Duration::from_secs(4), connect).await {
                Ok(Ok(())) => (true, None, None),
                Ok(Err(e)) => (false, None, Some(e)),
                Err(_) => (false, None, Some("таймаут".into())),
            }
        }
    };
    TargetResult {
        id: t.id.clone(),
        group: t.group.clone(),
        label: t.label.clone(),
        ok,
        status,
        ms: ok.then(|| started.elapsed().as_millis() as u64),
        error,
    }
}

async fn measure_speed(client: &reqwest::Client) -> Option<f64> {
    let started = Instant::now();
    let mut bytes = 0usize;
    for _ in 0..2 {
        match client.get(SPEED_URL).send().await {
            Ok(r) => match r.bytes().await {
                Ok(b) => bytes += b.len(),
                Err(_) => return None,
            },
            Err(_) => return None,
        }
    }
    let secs = started.elapsed().as_secs_f64();
    if bytes == 0 || secs <= 0.0 {
        return None;
    }
    Some((bytes as f64 / 1024.0) / secs)
}

#[allow(clippy::too_many_arguments)]
async fn probe_all(
    baseline: bool,
    strategy: String,
    label: String,
    started_ok: bool,
    error: Option<String>,
    proxy: Option<u16>,
    // Встроенные цели плюс свои сайты — список собирает вызывающая сторона
    list: &[Target],
) -> StrategyResult {
    let mut result = StrategyResult {
        strategy,
        label,
        baseline,
        started: started_ok,
        error,
        ok: 0,
        total: list.len(),
        score: 0.0,
        avg_ms: None,
        speed_kbs: None,
        groups: Vec::new(),
        targets: Vec::new(),
    };
    if !started_ok {
        return result;
    }
    let Ok(client) = client(proxy) else {
        result.error = Some("не удалось создать HTTP-клиент".into());
        return result;
    };

    let futures = list.iter().map(|t| probe_one(&client, t, proxy));
    let targets: Vec<TargetResult> = futures_util::future::join_all(futures).await;
    result.speed_kbs = measure_speed(&client).await;

    result.ok = targets.iter().filter(|t| t.ok).count();
    result.score = (result.ok as f64 / result.total.max(1) as f64) * 100.0;
    let lat: Vec<u64> = targets.iter().filter_map(|t| t.ms).collect();
    if !lat.is_empty() {
        result.avg_ms = Some(lat.iter().sum::<u64>() / lat.len() as u64);
    }
    for group in GROUPS {
        let items: Vec<&TargetResult> = targets.iter().filter(|t| t.group == *group).collect();
        // Пустую группу не показываем: «Свои 0/0» — это не результат
        if items.is_empty() {
            continue;
        }
        result.groups.push(GroupScore {
            group: (*group).into(),
            ok: items.iter().filter(|t| t.ok).count(),
            total: items.len(),
        });
    }
    result.targets = targets;
    result
}

/// Короткий осмотр для сторожа. Две цели, которые рвут чаще всего, плюс
/// первые из своих сайтов — и отдельно контрольная точка «интернет вообще
/// жив». Без неё сторож будил бы человека каждый раз, когда моргнёт Wi-Fi:
/// упавший обход и пропавшая сеть выглядят одинаково.
#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Health {
    pub ok: usize,
    pub total: usize,
    /// Контрольная точка ответила — значит, сеть на месте
    pub control_ok: bool,
    /// Что именно не отвечает — списком, для журнала
    pub failed: Vec<String>,
    pub checked_at: String,
}

pub async fn health(proxy: Option<u16>, own: &[String]) -> Health {
    let all = builtin();
    let pick = |id: &str| all.iter().find(|t| t.id == id).cloned();
    let mut list: Vec<Target> = ["discord-ws", "yt"].iter().filter_map(|i| pick(i)).collect();
    list.extend(own.iter().take(2).filter_map(|raw| parse_own(raw)));

    // Контрольную точку спрашиваем НАПРЯМУЮ, мимо прокси, и клиент для неё
    // заводим заранее. Через прокси она соврала бы ровно тогда, когда важнее
    // всего не соврать: умерший прокси уронил бы и её, а сторож решил бы,
    // что пропал интернет, — и промолчал
    let direct = client(None).ok();
    let Ok(via) = client(proxy) else {
        return Health { checked_at: crate::sysutil::hhmmss(), ..Default::default() };
    };
    let probes = list.iter().map(|t| probe_one(&via, t, proxy));
    let results: Vec<TargetResult> = futures_util::future::join_all(probes).await;

    let control_ok = match (pick("dns1"), direct) {
        (Some(t), Some(direct)) => probe_one(&direct, &t, None).await.ok,
        _ => true,
    };

    Health {
        ok: results.iter().filter(|r| r.ok).count(),
        total: results.len(),
        control_ok,
        failed: results.iter().filter(|r| !r.ok).map(|r| r.label.clone()).collect(),
        checked_at: crate::sysutil::hhmmss(),
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn run_suite(
    app: AppHandle,
    root: PathBuf,
    targets: Vec<Target>,
    game_filter: String,
    names: Vec<String>,
    include_baseline: bool,
    runner: Arc<Runner>,
    cancel: Arc<AtomicBool>,
) -> Result<Vec<StrategyResult>, String> {
    let all = crate::strategies::list(&root, &game_filter)?;
    let restore = runner.current.lock().unwrap().clone();

    let mut queue: Vec<Option<crate::strategies::Strategy>> = Vec::new();
    if include_baseline {
        queue.push(None);
    }
    for n in &names {
        if let Some(s) = crate::strategies::find(&all, n) {
            queue.push(Some(s.clone()));
        }
    }
    if queue.is_empty() {
        return Err("нечего проверять".into());
    }

    let total = queue.len();
    let mut results: Vec<StrategyResult> = Vec::new();

    for (index, item) in queue.into_iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let (strategy, label) = match &item {
            None => ("__baseline__".to_string(), "Без обхода".to_string()),
            Some(s) => (s.name.clone(), s.label.clone()),
        };

        let _ = app.emit(
            "test-stage",
            TestStage { index, total, strategy: strategy.clone(), label: label.clone(), phase: "start".into() },
        );

        let (started_ok, error) = match &item {
            None => {
                let r = runner.clone();
                let _ = tokio::task::spawn_blocking(move || r.stop(None)).await;
                tokio::time::sleep(Duration::from_millis(900)).await;
                (true, None)
            }
            Some(s) => {
                let r = runner.clone();
                let app2 = app.clone();
                let root2 = root.clone();
                let name = s.name.clone();
                let args = s.args.clone();
                let res = tokio::task::spawn_blocking(move || r.start(&app2, &root2, &name, &args, true))
                    .await
                    .map_err(|e| e.to_string())?;
                match res {
                    Ok(()) => {
                        // winws.exe подхватывает WinDivert не мгновенно
                        tokio::time::sleep(Duration::from_millis(2200)).await;
                        (true, None)
                    }
                    Err(e) => (false, Some(e)),
                }
            }
        };

        let _ = app.emit(
            "test-stage",
            TestStage { index, total, strategy: strategy.clone(), label: label.clone(), phase: "probe".into() },
        );

        let result =
            probe_all(item.is_none(), strategy, label, started_ok, error, None, &targets).await;
        let _ = app.emit("test-result", result.clone());
        results.push(result);
    }

    // Возвращаем то, что работало до проверки
    let r = runner.clone();
    let app2 = app.clone();
    let root2 = root.clone();
    let all2 = all.clone();
    let _ = tokio::task::spawn_blocking(move || {
        r.stop(None);
        if let Some(name) = restore {
            if let Some(s) = crate::strategies::find(&all2, &name) {
                let _ = r.start(&app2, &root2, &s.name, &s.args, true);
            }
        }
    })
    .await;

    let _ = app.emit("test-done", results.clone());
    Ok(results)
}

/// То же самое для ByeDPI: поднимаем пресеты по очереди и стучимся к тем же
/// целям, но уже через сам прокси. Системный прокси при этом не трогаем —
/// проверка не должна зависеть от того, включён он или нет.
#[allow(clippy::too_many_arguments)]
pub async fn run_byedpi_suite(
    app: AppHandle,
    root: PathBuf,
    port: u16,
    targets: Vec<Target>,
    catalog: Vec<crate::byedpi::Preset>,
    ids: Vec<String>,
    include_baseline: bool,
    proxy: Arc<crate::byedpi::Proxy>,
    runner: Arc<Runner>,
    cancel: Arc<AtomicBool>,
) -> Result<Vec<StrategyResult>, String> {
    let restore = proxy.current();

    let mut queue: Vec<Option<crate::byedpi::Preset>> = Vec::new();
    if include_baseline {
        queue.push(None);
    }
    for id in &ids {
        if let Some(p) = crate::byedpi::find(&catalog, id) {
            queue.push(Some(p.clone()));
        }
    }
    if queue.is_empty() {
        return Err("нечего проверять".into());
    }

    let total = queue.len();
    let mut results: Vec<StrategyResult> = Vec::new();

    for (index, item) in queue.into_iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let (id, label) = match &item {
            None => ("__baseline__".to_string(), "Без обхода".to_string()),
            Some(p) => (p.id.clone(), p.name.clone()),
        };

        let _ = app.emit(
            "test-stage",
            TestStage { index, total, strategy: id.clone(), label: label.clone(), phase: "start".into() },
        );

        let (started_ok, error, via) = match &item {
            None => {
                let (p, r) = (proxy.clone(), runner.clone());
                let _ = tokio::task::spawn_blocking(move || p.stop(&r, None)).await;
                tokio::time::sleep(Duration::from_millis(400)).await;
                (true, None, None)
            }
            Some(preset) => {
                let (p, r) = (proxy.clone(), runner.clone());
                let app2 = app.clone();
                let root2 = root.clone();
                let preset2 = preset.clone();
                let res = tokio::task::spawn_blocking(move || {
                    p.start(&app2, &r, &root2, &preset2, port, true)
                })
                .await
                .map_err(|e| e.to_string())?;
                match res {
                    Ok(()) => {
                        tokio::time::sleep(Duration::from_millis(400)).await;
                        (true, None, Some(port))
                    }
                    Err(e) => (false, Some(e), None),
                }
            }
        };

        let _ = app.emit(
            "test-stage",
            TestStage { index, total, strategy: id.clone(), label: label.clone(), phase: "probe".into() },
        );

        let result =
            probe_all(item.is_none(), id, label, started_ok, error, via, &targets).await;
        let _ = app.emit("test-result", result.clone());
        results.push(result);
    }

    // Возвращаем то, что работало до проверки
    let (p, r) = (proxy.clone(), runner.clone());
    let app2 = app.clone();
    let root2 = root.clone();
    let _ = tokio::task::spawn_blocking(move || {
        p.stop(&r, None);
        if let Some(id) = restore {
            if let Some(preset) = crate::byedpi::find(&catalog, &id) {
                let _ = p.start(&app2, &r, &root2, preset, port, true);
            }
        }
    })
    .await;

    let _ = app.emit("test-done", results.clone());
    Ok(results)
}

/// GoodbyeDPI правит пакеты драйвером, как и zapret, поэтому проверяем его
/// прямыми запросами — прокси тут ни при чём.
#[allow(clippy::too_many_arguments)]
pub async fn run_goodbye_suite(
    app: AppHandle,
    root: PathBuf,
    targets: Vec<Target>,
    catalog: Vec<crate::preset::Preset>,
    ids: Vec<String>,
    include_baseline: bool,
    engine: Arc<crate::goodbye::Engine>,
    runner: Arc<Runner>,
    cancel: Arc<AtomicBool>,
) -> Result<Vec<StrategyResult>, String> {
    let restore = engine.current();

    let mut queue: Vec<Option<crate::preset::Preset>> = Vec::new();
    if include_baseline {
        queue.push(None);
    }
    for id in &ids {
        if let Some(p) = crate::preset::find(&catalog, id) {
            queue.push(Some(p.clone()));
        }
    }
    if queue.is_empty() {
        return Err("нечего проверять".into());
    }

    let total = queue.len();
    let mut results: Vec<StrategyResult> = Vec::new();

    for (index, item) in queue.into_iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let (id, label) = match &item {
            None => ("__baseline__".to_string(), "Без обхода".to_string()),
            Some(p) => (p.id.clone(), p.name.clone()),
        };

        let _ = app.emit(
            "test-stage",
            TestStage { index, total, strategy: id.clone(), label: label.clone(), phase: "start".into() },
        );

        let (started_ok, error) = match &item {
            None => {
                let (e, r) = (engine.clone(), runner.clone());
                let _ = tokio::task::spawn_blocking(move || e.stop(&r, None)).await;
                tokio::time::sleep(Duration::from_millis(900)).await;
                (true, None)
            }
            Some(p) => {
                let (e, r) = (engine.clone(), runner.clone());
                let app2 = app.clone();
                let root2 = root.clone();
                let preset = p.clone();
                let res = tokio::task::spawn_blocking(move || e.start(&app2, &r, &root2, &preset, true))
                    .await
                    .map_err(|e| e.to_string())?;
                match res {
                    Ok(()) => {
                        // WinDivert подхватывает фильтр не мгновенно — как у zapret
                        tokio::time::sleep(Duration::from_millis(2200)).await;
                        (true, None)
                    }
                    Err(e) => (false, Some(e)),
                }
            }
        };

        let _ = app.emit(
            "test-stage",
            TestStage { index, total, strategy: id.clone(), label: label.clone(), phase: "probe".into() },
        );

        let result =
            probe_all(item.is_none(), id, label, started_ok, error, None, &targets).await;
        let _ = app.emit("test-result", result.clone());
        results.push(result);
    }

    // Возвращаем то, что работало до проверки
    let (e, r) = (engine.clone(), runner.clone());
    let app2 = app.clone();
    let root2 = root.clone();
    let _ = tokio::task::spawn_blocking(move || {
        e.stop(&r, None);
        if let Some(id) = restore {
            if let Some(p) = crate::preset::find(&catalog, &id) {
                let _ = e.start(&app2, &r, &root2, p, true);
            }
        }
    })
    .await;

    let _ = app.emit("test-done", results.clone());
    Ok(results)
}

/// Проверка пресетов прокси-ядра — Xray или sing-box.
///
/// От ByeDPI отличается тем, что пресет здесь не набор ключей, а готовый
/// конфиг: собрать его умеет только вызывающая сторона (ей известны и ссылка
/// на сервер, и выбранное ядро), поэтому конфиги приходят сюда уже готовыми.
/// Стучимся, как и к ByeDPI, через сам прокси — системный не трогаем.
#[allow(clippy::too_many_arguments)]
pub async fn run_core_suite(
    app: AppHandle,
    root: PathBuf,
    port: u16,
    targets: Vec<Target>,
    // Все пресеты, которые вообще можно поднять, вместе с их конфигами
    catalog: Vec<(crate::preset::Preset, serde_json::Value)>,
    ids: Vec<String>,
    include_baseline: bool,
    core: Arc<crate::proxycore::Core>,
    runner: Arc<Runner>,
    cancel: Arc<AtomicBool>,
) -> Result<Vec<StrategyResult>, String> {
    let restore = core.current();
    let pick = |id: &str| catalog.iter().find(|(p, _)| p.id == id).cloned();

    let mut queue: Vec<Option<(crate::preset::Preset, serde_json::Value)>> = Vec::new();
    if include_baseline {
        queue.push(None);
    }
    queue.extend(ids.iter().filter_map(|id| pick(id)).map(Some));
    if queue.is_empty() {
        return Err("нечего проверять".into());
    }

    let total = queue.len();
    let mut results: Vec<StrategyResult> = Vec::new();

    for (index, item) in queue.into_iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let (id, label) = match &item {
            None => ("__baseline__".to_string(), "Без обхода".to_string()),
            Some((p, _)) => (p.id.clone(), p.name.clone()),
        };

        let _ = app.emit(
            "test-stage",
            TestStage { index, total, strategy: id.clone(), label: label.clone(), phase: "start".into() },
        );

        let (started_ok, error, via) = match item.clone() {
            None => {
                let (c, r) = (core.clone(), runner.clone());
                let _ = tokio::task::spawn_blocking(move || c.stop(&r, None)).await;
                tokio::time::sleep(Duration::from_millis(400)).await;
                (true, None, None)
            }
            Some((preset, config)) => {
                let (c, r) = (core.clone(), runner.clone());
                let app2 = app.clone();
                let root2 = root.clone();
                let res = tokio::task::spawn_blocking(move || {
                    c.start(&app2, &r, &root2, &preset, &config, port, true)
                })
                .await
                .map_err(|e| e.to_string())?;
                match res {
                    // Ядро уже дождалось своего порта, лишней паузы не нужно
                    Ok(()) => (true, None, Some(port)),
                    Err(e) => (false, Some(e), None),
                }
            }
        };

        let _ = app.emit(
            "test-stage",
            TestStage { index, total, strategy: id.clone(), label: label.clone(), phase: "probe".into() },
        );

        let result =
            probe_all(item.is_none(), id, label, started_ok, error, via, &targets).await;
        let _ = app.emit("test-result", result.clone());
        results.push(result);
    }

    // Возвращаем то, что работало до проверки
    let restore = restore.and_then(|id| pick(&id));
    let (c, r) = (core.clone(), runner.clone());
    let app2 = app.clone();
    let root2 = root.clone();
    let _ = tokio::task::spawn_blocking(move || {
        c.stop(&r, None);
        if let Some((preset, config)) = restore {
            let _ = c.start(&app2, &r, &root2, &preset, &config, port, true);
        }
    })
    .await;

    let _ = app.emit("test-done", results.clone());
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Свой сайт можно написать тремя способами, и все три должны привести
    /// к осмысленной проверке, а не к молчаливому пропуску.
    #[test]
    fn reads_every_shape_of_own_target() {
        let site = parse_own("example.com").expect("голое имя");
        assert_eq!(site.group, OWN_GROUP);
        assert_eq!(site.label, "example.com");
        match &site.probe {
            Probe::Http(url, Expect::Any) => assert_eq!(url, "https://example.com/"),
            _ => panic!("голое имя должно проверяться по HTTPS"),
        }

        let full = parse_own("https://example.com/путь").expect("ссылка целиком");
        match &full.probe {
            Probe::Http(url, Expect::Any) => assert_eq!(url, "https://example.com/путь"),
            _ => panic!("ссылку берём как есть"),
        }

        let port = parse_own("example.com:8443").expect("хост с портом");
        match &port.probe {
            Probe::Tcp(host, p) => {
                assert_eq!(host, "example.com");
                assert_eq!(*p, 8443);
            }
            _ => panic!("хост с портом проверяется соединением"),
        }

        // Пустое, комментарии и строки с пробелами пропускаем: файл
        // targets.txt люди ведут руками
        assert!(parse_own("").is_none());
        assert!(parse_own("   ").is_none());
        assert!(parse_own("# мой сайт").is_none());
        assert!(parse_own("две штуки").is_none());
    }

    #[test]
    fn own_targets_join_the_builtin_ones_without_clashing() {
        let list = all(&["example.com".into(), "# коммент".into(), "".into()]);
        assert_eq!(list.len(), builtin().len() + 1);
        // id должны остаться уникальными, иначе результаты перепутаются
        for t in &list {
            assert_eq!(list.iter().filter(|o| o.id == t.id).count(), 1, "дубль {}", t.id);
        }
    }

    /// Заглушка провайдера отвечает обычным кодом 200 — если её не узнавать,
    /// заблокированный сайт засчитывался бы доступным.
    #[test]
    fn recognises_a_provider_stub_page() {
        assert!(looks_like_stub(
            "<html><body>Доступ ограничен. Ресурс внесён в Единый реестр</body></html>"
        ));
        assert!(looks_like_stub("Страница заблокирована: eais.rkn.gov.ru"));
        assert!(!looks_like_stub("<html><body>Обычная страница сайта</body></html>"));
    }

    /// Живая проверка движка: ZAPRET_NET_TEST=1 cargo test --lib -- --nocapture
    #[tokio::test]
    async fn probes_reach_targets() {
        if std::env::var("ZAPRET_NET_TEST").is_err() {
            return;
        }
        let client = client(None).expect("клиент");
        let list = all(&["example.com".to_string()]);
        let mut ok = 0;
        for t in &list {
            let r = probe_one(&client, t, None).await;
            println!(
                "{:<34} {:<9} {}",
                r.label,
                if r.ok { "ДОСТУПЕН" } else { "БЛОК" },
                r.ok.then(|| format!("{} мс, код {:?}", r.ms.unwrap_or(0), r.status))
                    .unwrap_or_else(|| format!("код {:?}: {}", r.status, r.error.clone().unwrap_or_default()))
            );
            if r.ok {
                ok += 1;
            }
        }
        println!("итого доступно: {ok} из {}", list.len());
        if let Some(kbs) = measure_speed(&client).await {
            println!("скорость на i.ytimg.com: {:.0} КБ/с", kbs);
        }
        // Контрольная точка: публичные DNS по TCP должны отвечать всегда
        assert!(ok >= 2, "сеть недоступна вообще — проверять нечего");
    }
}
