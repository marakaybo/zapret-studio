use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

const REPO: &str = "Flowseal/zapret-discord-youtube";
const UA: &str = concat!("ZapretStudio/", env!("CARGO_PKG_VERSION"));

/// Сколько ждать очередной кусок, прежде чем признать загрузку зависшей.
/// DPI умеет не рвать соединение, а просто перестать пропускать данные —
/// без этого ограничения приложение висело бы до общего таймаута в 10 минут.
const STALL_LIMIT: Duration = Duration::from_secs(25);
/// Сколько раз пробовать: сброс от DPI часто разовый и со второй попытки проходит.
const ATTEMPTS: u32 = 3;

pub const CANCELLED: &str = "Установка отменена";

/// Позволяет прервать долгое сетевое ожидание по флагу отмены: сам запрос
/// остановить нельзя, но ждать его результата — уже не обязательно.
pub async fn cancellable<T>(
    cancel: Arc<AtomicBool>,
    work: impl std::future::Future<Output = Result<T, String>>,
) -> Result<T, String> {
    tokio::pin!(work);
    loop {
        tokio::select! {
            done = &mut work => return done,
            _ = tokio::time::sleep(Duration::from_millis(200)) => {
                if cancel.load(Ordering::Relaxed) {
                    return Err(CANCELLED.into());
                }
            }
        }
    }
}

/// Ошибки сети здесь почти всегда об одном и том же, поэтому пишем словами,
/// а не пересказываем текст reqwest.
fn net_error(what: &str, e: &reqwest::Error) -> String {
    let hint = "Похоже, провайдер режет GitHub: включи обход или VPN и попробуй снова";
    if e.is_timeout() {
        return format!("{what}: истекло время ожидания. {hint}");
    }
    if e.is_connect() {
        return format!("{what}: не удалось соединиться. {hint}");
    }
    let text = e.to_string().to_lowercase();
    if text.contains("reset") || text.contains("closed") {
        return format!("{what}: соединение сброшено. {hint}");
    }
    if text.contains("dns") || text.contains("resolve") {
        return format!("{what}: имя api.github.com не разрешается — проверь интернет и DNS");
    }
    format!("{what}: {e}")
}

/// GitHub отвечает 403 и на «нельзя», и на «слишком часто», а без токена
/// разрешено всего 60 запросов в час на один адрес — и адрес этот общий, если
/// сидишь за NAT провайдера. Голое «GitHub ответил 403 Forbidden» в такой
/// ситуации не говорит ни что случилось, ни что делать.
fn http_error(what: &str, status: u16, headers: &reqwest::header::HeaderMap) -> String {
    let header = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
    let exhausted = header("x-ratelimit-remaining") == Some("0");

    if (status == 403 || status == 429) && exhausted {
        let when = header("x-ratelimit-reset")
            .and_then(|v| v.parse::<i64>().ok())
            .and_then(|ts| chrono::DateTime::from_timestamp(ts, 0))
            .map(|t| t.with_timezone(&chrono::Local).format("%H:%M").to_string());
        return match when {
            Some(t) => format!(
                "{what}: исчерпан лимит запросов к GitHub — без входа их всего 60 в час на один адрес. Лимит вернётся в {t}, до тех пор проверки обновлений подождут"
            ),
            None => format!(
                "{what}: исчерпан лимит запросов к GitHub — без входа их всего 60 в час на один адрес. Подожди примерно час"
            ),
        };
    }
    if status == 403 || status == 429 {
        let after = header("retry-after").map(|v| format!(" Повтори через {v} с.")).unwrap_or_default();
        return format!("{what}: GitHub отказал ({status}).{after} Обычно это временно");
    }
    if status == 404 {
        return format!("{what}: GitHub отвечает «не найдено» (404) — проверь адрес репозитория");
    }
    format!("{what}: GitHub ответил {status}")
}

#[derive(Deserialize)]
struct GhAsset {
    name: String,
    size: u64,
    browser_download_url: String,
}

/// Где лежит контрольная сумма архива. Файлы качаются через то же
/// соединение, которое провайдер и портит, а внутри — исполняемый код:
/// если релиз публикует сумму, проверить её ничего не стоит.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Digest {
    pub url: String,
    /// Как назывался файл суммы — только чтобы объяснить в ошибке
    pub name: String,
}

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    published_at: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseInfo {
    pub version: String,
    pub published_at: String,
    pub notes: String,
    pub zip_url: String,
    pub size: u64,
    /// Интерфейсу не нужна, а вот загрузке — да
    #[serde(skip)]
    pub digest: Option<Digest>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheck {
    pub current: Option<String>,
    pub latest: Option<String>,
    pub has_update: bool,
    pub release: Option<ReleaseInfo>,
    pub error: Option<String>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub phase: String,
    pub percent: f64,
    pub detail: String,
}

pub fn emit(app: &AppHandle, phase: &str, percent: f64, detail: impl Into<String>) {
    let _ = app.emit(
        "update-progress",
        Progress { phase: phase.into(), percent, detail: detail.into() },
    );
}

/// Хвост версии после чисел: "0.2.3rc3" -> "rc3", "1.10.2" -> "".
fn suffix(v: &str) -> &str {
    v.trim_start_matches(['v', 'V'])
        .rsplit(['.', '-', '_'])
        .next()
        .unwrap_or("")
        .trim_start_matches(|c: char| c.is_ascii_digit())
}

/// "1.10.2" новее "1.9.9": сравниваем по числовым сегментам, а не как строки.
/// При равенстве чисел решает хвост: готовый релиз новее предрелиза
/// ("0.2.3" > "0.2.3rc3"), а среди предрелизов — по алфавиту (rc3 > rc2).
pub fn is_newer(latest: &str, current: &str) -> bool {
    let parse = |s: &str| -> Vec<u32> {
        s.trim_start_matches(['v', 'V'])
            .split(['.', '-', '_'])
            .map(|p| p.chars().take_while(|c| c.is_ascii_digit()).collect::<String>())
            .map(|p| p.parse::<u32>().unwrap_or(0))
            .collect()
    };
    let (a, b) = (parse(latest), parse(current));
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (*a.get(i).unwrap_or(&0), *b.get(i).unwrap_or(&0));
        if x != y {
            return x > y;
        }
    }
    match (suffix(latest), suffix(current)) {
        (l, c) if l == c => false,
        ("", _) => true,
        (_, "") => false,
        (l, c) => l > c,
    }
}

/// Движки живут в разных репозиториях и кладут в релиз разные файлы,
/// поэтому нужный архив выбирает вызывающая сторона.
fn to_info(rel: &GhRelease, want: fn(&str) -> bool) -> Option<ReleaseInfo> {
    let asset = rel.assets.iter().find(|a| want(&a.name))?;
    // Суммы публикуют по-разному: у Xray это `<файл>.dgst`, у остальных чаще
    // `<файл>.sha256`. Нет ни того ни другого — проверять просто нечего
    let digest = rel
        .assets
        .iter()
        .find(|a| {
            let n = a.name.to_lowercase();
            let base = asset.name.to_lowercase();
            n == format!("{base}.dgst") || n == format!("{base}.sha256")
        })
        .map(|a| Digest { url: a.browser_download_url.clone(), name: a.name.clone() });

    Some(ReleaseInfo {
        version: rel.tag_name.trim_start_matches('v').to_string(),
        published_at: rel.published_at.chars().take(10).collect(),
        notes: rel.body.chars().take(4000).collect(),
        zip_url: asset.browser_download_url.clone(),
        size: asset.size,
        digest,
    })
}

/// Выуживает SHA-256 из файла суммы. Форматов много — `SHA256= abc…` у Xray,
/// `abc…  имя-файла` у sha256sum, — но во всех есть ровно то, что нам нужно:
/// 64 шестнадцатеричных символа подряд. Предпочитаем строку, где рядом
/// написано «sha256», иначе берём первую подходящую.
fn find_sha256(text: &str) -> Option<String> {
    let token = |line: &str| -> Option<String> {
        line.split(|c: char| !c.is_ascii_hexdigit())
            .find(|w| w.len() == 64)
            .map(|w| w.to_lowercase())
    };
    text.lines()
        .find(|l| l.to_lowercase().contains("sha256"))
        .and_then(token)
        .or_else(|| text.lines().find_map(token))
}

fn sha256_file(path: &Path) -> Result<String, String> {
    use sha2::{Digest as _, Sha256};
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher).map_err(|e| e.to_string())?;
    Ok(format!("{:x}", hasher.finalize()))
}

/// Сверяет скачанный файл с суммой из релиза. Если сумму не удалось ни
/// скачать, ни разобрать — это не повод рушить установку: у большинства
/// релизов её нет вовсе, и раньше не было тем более.
async fn verify(file: &Path, digest: &Digest) -> Result<Option<String>, String> {
    let client = reqwest::Client::builder()
        .user_agent(UA)
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;
    let Ok(resp) = client.get(&digest.url).send().await else {
        return Ok(Some(format!("не удалось скачать {} — установка без проверки", digest.name)));
    };
    let Ok(text) = resp.text().await else {
        return Ok(Some(format!("{} не читается — установка без проверки", digest.name)));
    };
    let Some(want) = find_sha256(&text) else {
        return Ok(Some(format!("в {} нет SHA-256 — установка без проверки", digest.name)));
    };
    let got = sha256_file(file)?;
    if got == want {
        Ok(None)
    } else {
        Err(format!(
            "Контрольная сумма не сошлась: ждали {}…, получили {}…. Файл повреждён или подменён по дороге — установка отменена",
            &want[..12],
            &got[..12]
        ))
    }
}

/// Запрос к GitHub с повторами: разовый сброс от DPI со второй попытки
/// обычно проходит.
async fn github<T: serde::de::DeserializeOwned>(url: &str) -> Result<T, String> {
    let client = reqwest::Client::builder()
        .user_agent(UA)
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    let mut last = String::new();
    let mut resp = None;
    for attempt in 1..=ATTEMPTS {
        match client.get(url).header("Accept", "application/vnd.github+json").send().await {
            Ok(r) => {
                resp = Some(r);
                break;
            }
            Err(e) => {
                last = net_error("GitHub недоступен", &e);
                if attempt < ATTEMPTS {
                    tokio::time::sleep(Duration::from_millis(700)).await;
                }
            }
        }
    }
    let resp = resp.ok_or(last)?;

    if !resp.status().is_success() {
        return Err(http_error("Не удалось спросить GitHub", resp.status().as_u16(), resp.headers()));
    }
    resp.json().await.map_err(|e| format!("не разобрать ответ GitHub: {e}"))
}

/// Последний стабильный релиз репозитория.
pub async fn latest_release_of(repo: &str, want: fn(&str) -> bool) -> Result<ReleaseInfo, String> {
    let url = format!("https://api.github.com/repos/{repo}/releases/latest");
    let rel: GhRelease = github(&url).await?;
    to_info(&rel, want).ok_or_else(|| "в релизе нет подходящего архива".into())
}

/// Самый свежий релиз, включая предрелизы. Нужен GoodbyeDPI: стабильный там
/// висит с 2022 года, а всё живое выходит как release candidate.
pub async fn latest_release_any(repo: &str, want: fn(&str) -> bool) -> Result<ReleaseInfo, String> {
    let url = format!("https://api.github.com/repos/{repo}/releases?per_page=10");
    let list: Vec<GhRelease> = github(&url).await?;
    list.iter()
        .filter(|r| !r.draft)
        .find_map(|r| to_info(r, want))
        .ok_or_else(|| "в релизах нет подходящего архива".into())
}

pub async fn latest_release() -> Result<ReleaseInfo, String> {
    latest_release_of(REPO, |name| name.to_lowercase().ends_with(".zip")).await
}

const USER_FILES: [&str; 6] = [
    "lists/list-general-user.txt",
    "lists/list-exclude-user.txt",
    "lists/ipset-exclude-user.txt",
    "utils/game_filter.enabled",
    "utils/check_updates.enabled",
    "utils/targets.txt",
];

fn copy_user_files(from: &Path, to: &Path) {
    for rel in USER_FILES {
        let src = from.join(rel);
        if src.is_file() {
            let dst = to.join(rel);
            if let Some(p) = dst.parent() {
                let _ = std::fs::create_dir_all(p);
            }
            let _ = std::fs::copy(&src, &dst);
        }
    }
}

/// Скачивает архив релиза и раскладывает его в target, сохраняя пользовательские списки.
pub async fn install(
    app: &AppHandle,
    release: &ReleaseInfo,
    target: &Path,
    cancel: Arc<AtomicBool>,
) -> Result<String, String> {
    let parent = target.parent().ok_or("некорректный путь установки")?.to_path_buf();
    std::fs::create_dir_all(&parent).map_err(|e| e.to_string())?;

    let staging = parent.join("zapret-new");
    let backup = parent.join("zapret-old");
    for p in [&staging, &backup] {
        if p.exists() {
            let _ = std::fs::remove_dir_all(p);
        }
    }

    download_and_extract(app, release, &staging, "zapret-download.zip", cancel).await?;

    let root = crate::sysutil::resolve_zapret_root(&staging).ok_or("в архиве не найден bin/winws.exe")?;

    // 3. Замена: старая папка уезжает в бэкап, новая встаёт на её место.
    // Если второй шаг сорвётся, прежнюю папку надо вернуть: иначе человек
    // остаётся вообще без zapret, и хуже того — не понимает, куда он делся
    emit(app, "apply", 92.0, "Переношу настройки…");
    let mut backed_up = false;
    if target.exists() {
        copy_user_files(target, &root);
        std::fs::rename(target, &backup).map_err(|e| format!("не удалось освободить папку: {e}"))?;
        backed_up = true;
    }
    let from = if root != staging { &root } else { &staging };
    if let Err(e) = std::fs::rename(from, target) {
        if backed_up {
            let _ = std::fs::rename(&backup, target);
        }
        let _ = std::fs::remove_dir_all(&staging);
        return Err(format!("не удалось установить: {e}. Прежняя версия оставлена на месте"));
    }
    let _ = std::fs::remove_dir_all(&staging);
    let _ = std::fs::remove_dir_all(&backup);

    crate::strategies::ensure_user_lists(target);
    emit(app, "done", 100.0, format!("Версия {} установлена", release.version));
    Ok(release.version.clone())
}

/// Качает архив релиза во временный файл рядом с dest и распаковывает его туда.
/// Одинаково для zapret и ByeDPI — различается только то, что делают с
/// распакованным дальше.
/// Качает файл, сообщая о ходе через `progress`, и умеет остановиться:
/// по флагу `cancel` — почти сразу, по молчанию сети — через `STALL_LIMIT`.
/// Отдельно от `download_and_extract`, чтобы можно было проверить тестом.
async fn download_to_file(
    url: &str,
    expected: u64,
    tmp_zip: &Path,
    cancel: &AtomicBool,
    progress: &(dyn Fn(f64, String) + Send + Sync),
) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .user_agent(UA)
        .connect_timeout(Duration::from_secs(12))
        .build()
        .map_err(|e| e.to_string())?;

    let mut last = String::new();
    let mut resp = None;
    for attempt in 1..=ATTEMPTS {
        if cancel.load(Ordering::Relaxed) {
            return Err(CANCELLED.into());
        }
        progress(
            0.0,
            if attempt == 1 {
                "Соединяюсь с GitHub…".to_string()
            } else {
                format!("Соединяюсь с GitHub… попытка {attempt} из {ATTEMPTS}")
            },
        );
        // Общий таймаут на запрос не ставим — он оборвал бы и саму закачку;
        // ограничиваем только ожидание заголовков.
        let sent = match tokio::time::timeout(Duration::from_secs(20), client.get(url).send()).await {
            Ok(r) => r,
            Err(_) => {
                last = "Не скачивается: GitHub не ответил за 20 секунд. Похоже, провайдер режет GitHub: включи обход или VPN и попробуй снова".into();
                if attempt < ATTEMPTS {
                    tokio::time::sleep(Duration::from_millis(700)).await;
                }
                continue;
            }
        };
        match sent {
            Ok(r) if r.status().is_success() => {
                resp = Some(r);
                break;
            }
            Ok(r) => {
                last = http_error("Не скачивается", r.status().as_u16(), r.headers());
                break;
            }
            Err(e) => {
                last = net_error("Не скачивается", &e);
                if attempt < ATTEMPTS {
                    tokio::time::sleep(Duration::from_millis(700)).await;
                }
            }
        }
    }
    let resp = resp.ok_or(last)?;

    let total = resp.content_length().unwrap_or(expected).max(1);
    let mut file = std::fs::File::create(tmp_zip).map_err(|e| e.to_string())?;
    let mut got: u64 = 0;
    let mut moved_at = Instant::now();
    let mut stream = resp.bytes_stream();

    // Куски ждём маленькими шагами: так «Отмена» срабатывает почти сразу и
    // видно, что данные перестали идти. Иначе приложение молча висело бы,
    // а именно так DPI и мешает — не рвёт соединение, а перестаёт пропускать.
    let failure = loop {
        if cancel.load(Ordering::Relaxed) {
            break Some(CANCELLED.to_string());
        }
        match tokio::time::timeout(Duration::from_millis(300), stream.next()).await {
            Err(_) => {
                if moved_at.elapsed() > STALL_LIMIT {
                    break Some(format!(
                        "загрузка встала: за {} секунд не пришло ни байта. Похоже, провайдер режет GitHub — включи обход или VPN и попробуй снова",
                        STALL_LIMIT.as_secs()
                    ));
                }
            }
            Ok(None) => break None,
            Ok(Some(Err(e))) => break Some(net_error("Обрыв загрузки", &e)),
            Ok(Some(Ok(chunk))) => {
                use std::io::Write;
                if let Err(e) = file.write_all(&chunk) {
                    break Some(e.to_string());
                }
                got += chunk.len() as u64;
                moved_at = Instant::now();
                progress(
                    (got as f64 / total as f64) * 100.0,
                    format!("{:.1} из {:.1} МБ", got as f64 / 1e6, total as f64 / 1e6),
                );
            }
        }
    };
    drop(file);
    match failure {
        Some(message) => Err(message),
        None => Ok(()),
    }
}

/// Качает файл релиза и сверяет контрольную сумму, если она опубликована.
/// Отдельно от распаковки: установщик самого приложения — обычный `.exe`,
/// распаковывать там нечего.
pub async fn download_release_file(
    app: &AppHandle,
    release: &ReleaseInfo,
    dest: &Path,
    cancel: Arc<AtomicBool>,
) -> Result<Option<String>, String> {
    let report = |percent: f64, detail: String| emit(app, "download", percent, detail);
    download_to_file(&release.zip_url, release.size, dest, &cancel, &report).await?;

    let Some(digest) = &release.digest else { return Ok(None) };
    emit(app, "extract", 0.0, "Сверяю контрольную сумму…");
    match verify(dest, digest).await {
        Ok(note) => Ok(note),
        Err(e) => {
            let _ = std::fs::remove_file(dest);
            Err(e)
        }
    }
}

/// Качает архив релиза во временный файл рядом с dest и распаковывает его туда.
/// Одинаково для zapret и ByeDPI — различается только то, что делают с
/// распакованным дальше.
pub async fn download_and_extract(
    app: &AppHandle,
    release: &ReleaseInfo,
    dest: &Path,
    tmp_name: &str,
    cancel: Arc<AtomicBool>,
) -> Result<(), String> {
    let parent = dest.parent().ok_or("некорректный путь установки")?.to_path_buf();
    std::fs::create_dir_all(&parent).map_err(|e| e.to_string())?;
    let tmp_zip = parent.join(tmp_name);

    // Мусор от сорванной попытки только помешает следующей
    let cleanup = || {
        let _ = std::fs::remove_file(&tmp_zip);
        let _ = std::fs::remove_dir_all(dest);
    };

    match download_release_file(app, release, &tmp_zip, cancel.clone()).await {
        Ok(Some(note)) => emit(app, "extract", 0.0, note),
        Ok(None) => {}
        Err(e) => {
            cleanup();
            return Err(e);
        }
    }

    emit(app, "extract", 0.0, "Распаковываю…");
    let dest_owned = dest.to_path_buf();
    let zip_clone = tmp_zip.clone();
    let app_clone = app.clone();
    let cancel_clone = cancel.clone();
    let result =
        tokio::task::spawn_blocking(move || extract_zip(&app_clone, &zip_clone, &dest_owned, &cancel_clone))
            .await
            .map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&tmp_zip);
    if result.is_err() {
        let _ = std::fs::remove_dir_all(dest);
    }
    result
}

fn extract_zip(app: &AppHandle, zip_path: &Path, dest: &Path, cancel: &AtomicBool) -> Result<(), String> {
    let file = std::fs::File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("битый архив: {e}"))?;
    let total = archive.len().max(1);
    std::fs::create_dir_all(dest).map_err(|e| e.to_string())?;

    for i in 0..archive.len() {
        if cancel.load(Ordering::Relaxed) {
            return Err(CANCELLED.into());
        }
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let Some(rel) = entry.enclosed_name() else {
            continue;
        };
        let out_path: PathBuf = dest.join(rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out_path).map_err(|e| e.to_string())?;
        } else {
            if let Some(p) = out_path.parent() {
                std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
            }
            let mut out = std::fs::File::create(&out_path).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
        }
        if i % 5 == 0 {
            emit(app, "extract", (i as f64 / total as f64) * 100.0, "Распаковываю…");
        }
    }
    Ok(())
}

pub async fn check(current: Option<String>) -> UpdateCheck {
    check_repo(current, REPO, |name| name.to_lowercase().ends_with(".zip")).await
}

pub async fn check_any(current: Option<String>, repo: &str, want: fn(&str) -> bool) -> UpdateCheck {
    verdict(current, latest_release_any(repo, want).await)
}

pub async fn check_repo(current: Option<String>, repo: &str, want: fn(&str) -> bool) -> UpdateCheck {
    verdict(current, latest_release_of(repo, want).await)
}

fn verdict(current: Option<String>, found: Result<ReleaseInfo, String>) -> UpdateCheck {
    match found {
        Ok(rel) => {
            let has = match &current {
                Some(c) => is_newer(&rel.version, c),
                None => true,
            };
            UpdateCheck {
                current,
                latest: Some(rel.version.clone()),
                has_update: has,
                release: Some(rel),
                error: None,
            }
        }
        Err(e) => UpdateCheck { current, latest: None, has_update: false, release: None, error: Some(e) },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ответ 403 без объяснений — самая частая жалоба: пользователь видит
    /// «Forbidden» и не понимает, что просто надо подождать.
    #[test]
    fn explains_the_rate_limit_instead_of_saying_forbidden() {
        let make = |remaining: &str| {
            let mut h = reqwest::header::HeaderMap::new();
            h.insert("x-ratelimit-remaining", remaining.parse().unwrap());
            h.insert("x-ratelimit-reset", "4102444800".parse().unwrap());
            h
        };

        let limited = http_error("Не скачивается", 403, &make("0"));
        assert!(limited.contains("лимит"), "{limited}");
        assert!(limited.contains("60 в час"), "{limited}");
        assert!(!limited.contains("403"), "код без объяснения не помогает: {limited}");

        // 403 с оставшимся запасом — это уже не лимит, и врать про него нельзя
        let denied = http_error("Не скачивается", 403, &make("17"));
        assert!(denied.contains("403"), "{denied}");
        assert!(!denied.contains("лимит"), "{denied}");

        let missing = http_error("Не удалось спросить GitHub", 404, &make("50"));
        assert!(missing.contains("репозитор"), "{missing}");
    }

    /// Суммы публикуют в разных форматах — из всех нужно достать одно и то же.
    #[test]
    fn digs_sha256_out_of_any_format() {
        let hex = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        // формат Xray: несколько строк, нужная помечена
        let dgst = format!("MD5= 1234\nSHA1= abcd\nSHA256= {hex}\nSHA512= ffff");
        assert_eq!(find_sha256(&dgst).as_deref(), Some(hex));
        // формат sha256sum: сумма, пробелы, имя файла
        assert_eq!(find_sha256(&format!("{hex}  setup.exe")).as_deref(), Some(hex));
        // регистр не важен
        assert_eq!(find_sha256(&hex.to_uppercase()).as_deref(), Some(hex));
        // ничего похожего — и не надо выдумывать
        assert!(find_sha256("никаких сумм тут нет").is_none());
        assert!(find_sha256("SHA256= слишкомкороткая").is_none());
    }

    /// Сумма считается по содержимому файла, а не по чему-то ещё.
    #[test]
    fn hashes_the_file_itself() {
        let path = std::env::temp_dir().join("zs-sha-test.bin");
        std::fs::write(&path, b"zapret").unwrap();
        let first = sha256_file(&path).unwrap();
        assert_eq!(first.len(), 64);
        // тот же файл — та же сумма, изменённый — другая
        assert_eq!(first, sha256_file(&path).unwrap());
        std::fs::write(&path, b"zapret!").unwrap();
        assert_ne!(first, sha256_file(&path).unwrap());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn compares_versions_by_numeric_segments() {
        assert!(is_newer("1.10.2", "1.9.9"));
        assert!(is_newer("v0.17.3", "0.17.2"));
        assert!(!is_newer("1.10.2", "1.10.2"));
        assert!(!is_newer("1.9.9", "1.10.0"));
    }

    /// У GoodbyeDPI живут предрелизы, и rc3 должен считаться новее rc2.
    #[test]
    fn understands_release_candidates() {
        assert!(is_newer("0.2.3rc3", "0.2.3rc2"));
        assert!(is_newer("0.2.3rc1", "0.2.2"));
        assert!(!is_newer("0.2.3rc3", "0.2.3rc3"));
        // готовый релиз новее любого своего предрелиза, и наоборот — нет
        assert!(is_newer("0.2.3", "0.2.3rc3"));
        assert!(!is_newer("0.2.3rc3", "0.2.3"));
    }

    /// Отмена должна сработать, даже если загрузка идёт полным ходом.
    #[tokio::test]
    async fn cancel_stops_download() {
        if std::env::var("ZAPRET_NET_TEST").is_err() {
            return;
        }
        let release = latest_release().await.expect("релиз zapret");
        let tmp = std::env::temp_dir().join("zs-cancel-test.zip");
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(120)).await;
            flag.store(true, Ordering::SeqCst);
        });
        let started = Instant::now();
        let err = download_to_file(&release.zip_url, release.size, &tmp, &cancel, &|_, _| {})
            .await
            .expect_err("загрузка должна была отмениться");
        println!("отмена за {} мс: {err}", started.elapsed().as_millis());
        assert_eq!(err, CANCELLED);
        // Реакция на кнопку должна быть быстрой, а не «когда-нибудь потом»
        assert!(started.elapsed() < Duration::from_secs(5), "отмена заняла {:?}", started.elapsed());
        let _ = std::fs::remove_file(&tmp);
    }

    /// Маленький архив ByeDPI качается целиком и совпадает по размеру.
    #[tokio::test]
    async fn downloads_byedpi_archive() {
        if std::env::var("ZAPRET_NET_TEST").is_err() {
            return;
        }
        let release = latest_release_of("hufrea/byedpi", crate::byedpi::is_windows_asset)
            .await
            .expect("релиз byedpi");
        let tmp = std::env::temp_dir().join("zs-download-test.zip");
        let cancel = AtomicBool::new(false);
        download_to_file(&release.zip_url, release.size, &tmp, &cancel, &|p, d| {
            if p >= 100.0 {
                println!("загружено: {d}");
            }
        })
        .await
        .expect("архив должен скачаться");
        let size = std::fs::metadata(&tmp).unwrap().len();
        assert_eq!(size, release.size, "размер не совпал");
        let _ = std::fs::remove_file(&tmp);
    }

    /// Живая проверка обоих репозиториев тем же клиентом, что и в приложении:
    /// ZAPRET_NET_TEST=1 cargo test --lib -- --nocapture
    #[tokio::test]
    async fn reaches_both_repositories() {
        if std::env::var("ZAPRET_NET_TEST").is_err() {
            return;
        }
        for (repo, want) in [
            ("Flowseal/zapret-discord-youtube", (|n: &str| n.to_lowercase().ends_with(".zip")) as fn(&str) -> bool),
            ("hufrea/byedpi", crate::byedpi::is_windows_asset),
        ] {
            match latest_release_of(repo, want).await {
                Ok(r) => println!("{repo}: {} — {} ({} байт)", r.version, r.zip_url, r.size),
                Err(e) => println!("{repo}: ОШИБКА — {e}"),
            }
        }
    }
}
