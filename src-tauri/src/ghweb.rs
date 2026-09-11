//! Релизы GitHub без обращения к API.
//!
//! Зачем. У `api.github.com` без входа 60 запросов в час, и считаются они
//! **не на приложение и не на пользователя, а на IP-адрес**. За одним
//! адресом обычно сидит не один человек: мобильный оператор, домовой CGNAT,
//! офисная сеть, а чаще всего — VPN, через который к GitHub и ходят, когда
//! он режется напрямую. Все они делят одни и те же 60 запросов. Поэтому
//! новый человек мог увидеть «слишком много запросов» на первом же нажатии,
//! не сделав до этого ни одного: лимит израсходовали чужие люди с того же
//! адреса. Сколько ни экономь запросы внутри приложения, при общем адресе
//! этого мало — экономия одного пользователя ничего не значит, когда квоту
//! тратит вся сеть за ним.
//!
//! Приём. Обычные страницы `github.com` в лимит API не входят вовсе. Это
//! не догадка: замер до и после — `X-RateLimit-Remaining` не двигается.
//! Заодно выяснилось, что привычный совет «шли условные запросы с ETag,
//! ответ 304 бесплатный» сегодня не работает: 304 списывает запрос так же,
//! как обычный ответ. Поэтому экономить нечего — надо не ходить в API.
//!
//! Что откуда берётся:
//!
//! - `/<repo>/releases/latest` отвечает перенаправлением на `/releases/tag/<тег>`
//!   — отсюда номер последней **стабильной** версии;
//! - `/<repo>/releases.atom` — лента последних релизов **вместе с предрелизами**,
//!   с датами и описаниями. Ровно то, что нужно GoodbyeDPI: стабильный релиз
//!   там висит с 2022 года, а всё живое выходит как release candidate;
//! - `/<repo>/releases/expanded_assets/<тег>` — список файлов релиза.
//!
//! Чего это стоит. Первые два — обычные, десятилетиями неизменные адреса.
//! Третий служебный, и GitHub может его переделать, поэтому он здесь не
//! единственная опора: если разбор не удался, `updater` возвращается к API,
//! то есть к прежнему поведению. Хуже, чем было, не станет.

use crate::updater::{Digest, ReleaseInfo};
use std::time::Duration;

const UA: &str = concat!("ZapretStudio/", env!("CARGO_PKG_VERSION"));

fn client(follow: bool) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(UA)
        .timeout(Duration::from_secs(12))
        .redirect(if follow {
            reqwest::redirect::Policy::limited(5)
        } else {
            reqwest::redirect::Policy::none()
        })
        .build()
        .map_err(|e| e.to_string())
}

/// Тег из адреса, на который увёл GitHub: `.../releases/tag/v1.2.3` → `v1.2.3`.
fn tag_from_location(location: &str) -> Option<String> {
    let tag = location.rsplit_once("/releases/tag/")?.1;
    let tag = tag.split(['?', '#']).next().unwrap_or_default().trim();
    (!tag.is_empty()).then(|| tag.to_string())
}

/// Одна запись ленты релизов.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub tag: String,
    /// Дата в виде `2024-09-15`
    pub updated: String,
    pub notes: String,
}

fn between<'a>(text: &'a str, open: &str, close: &str) -> Option<&'a str> {
    let start = text.find(open)? + open.len();
    let rest = &text[start..];
    let end = rest.find(close)?;
    Some(&rest[..end])
}

/// Разворачивает то немногое, что экранирует Atom. Полноценный разбор HTML
/// тут ни к чему: описание релиза идёт в интерфейс текстом, и достаточно,
/// чтобы в нём не мелькали `&lt;p&gt;`.
fn unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
        // Амперсанд — последним, иначе он распакует то, что сам же создал
        .replace("&amp;", "&")
}

/// Выкидывает теги, оставляя текст. Переводы строк на месте абзацев и
/// списков — иначе описание слипается в одну строку.
fn strip_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut inside = false;
    let mut tag = String::new();
    for ch in html.chars() {
        match ch {
            '<' => {
                inside = true;
                tag.clear();
            }
            '>' => {
                inside = false;
                let name = tag.trim_start_matches('/').to_lowercase();
                if name.starts_with("p") || name.starts_with("li") || name.starts_with("br") {
                    out.push('\n');
                }
            }
            c if inside => tag.push(c),
            c => out.push(c),
        }
    }
    // Пустые строки, оставшиеся от разметки, схлопываем
    let mut text = String::new();
    for line in out.lines() {
        let line = line.trim();
        if !line.is_empty() {
            text.push_str(line);
            text.push('\n');
        }
    }
    text.trim().to_string()
}

/// Разбирает ленту релизов. Порядок сохраняется: GitHub отдаёт свежие первыми,
/// и предрелизы идут вперемешку со стабильными — отличить их лента не даёт,
/// поэтому «последнюю стабильную» узнаём отдельно, перенаправлением.
pub fn entries_from_atom(xml: &str) -> Vec<Entry> {
    let mut out = Vec::new();
    for chunk in xml.split("<entry>").skip(1) {
        let entry = chunk.split("</entry>").next().unwrap_or(chunk);
        let Some(href) = between(entry, "href=\"", "\"") else { continue };
        let Some(tag) = tag_from_location(href) else { continue };
        let updated = between(entry, "<updated>", "</updated>")
            .map(|d| d.chars().take(10).collect())
            .unwrap_or_default();
        let notes = between(entry, "<content type=\"html\">", "</content>")
            .map(|c| strip_tags(&unescape(c)))
            .unwrap_or_default();
        out.push(Entry { tag, updated, notes: notes.chars().take(4000).collect() });
    }
    out
}

/// Файл релиза: имя и прямая ссылка.
#[derive(Debug, Clone, PartialEq)]
pub struct WebAsset {
    pub name: String,
    pub url: String,
}

/// Вытаскивает файлы релиза из служебной страницы. Опираемся на единственное,
/// что там точно есть и не может измениться, не сломав сам GitHub, — ссылки
/// вида `/<owner>/<repo>/releases/download/<тег>/<файл>`.
pub fn assets_from_html(html: &str) -> Vec<WebAsset> {
    let mut out: Vec<WebAsset> = Vec::new();
    for chunk in html.split("href=\"").skip(1) {
        let Some(href) = chunk.split('"').next() else { continue };
        if !href.contains("/releases/download/") {
            continue;
        }
        let Some(name) = href.rsplit('/').next() else { continue };
        let name = name.trim();
        if name.is_empty() {
            continue;
        }
        // В разметке путь записан без домена, но встречается и целый адрес
        let url = if href.starts_with("http") {
            href.to_string()
        } else {
            format!("https://github.com{href}")
        };
        // Один и тот же файл в разметке попадается дважды — ссылкой и значком
        if out.iter().any(|a: &WebAsset| a.name == name) {
            continue;
        }
        out.push(WebAsset { name: name.to_string(), url });
    }
    out
}

/// Собирает `ReleaseInfo` из уже известных тега, описания и списка файлов.
fn to_info(
    tag: &str,
    updated: &str,
    notes: &str,
    assets: &[WebAsset],
    want: fn(&str) -> bool,
) -> Option<ReleaseInfo> {
    let asset = assets.iter().find(|a| want(&a.name))?;
    let base = asset.name.to_lowercase();
    let digest = assets
        .iter()
        .find(|a| {
            let n = a.name.to_lowercase();
            n == format!("{base}.dgst") || n == format!("{base}.sha256")
        })
        .map(|a| Digest { url: a.url.clone(), name: a.name.clone() });

    Some(ReleaseInfo {
        version: tag.trim_start_matches('v').to_string(),
        published_at: updated.to_string(),
        notes: notes.to_string(),
        zip_url: asset.url.clone(),
        // Настоящий размер придёт в `content-length` при самой загрузке —
        // он и берётся в первую очередь; здесь его знать неоткуда
        size: 0,
        digest,
    })
}

async fn get_text(url: &str) -> Result<String, String> {
    let resp = client(true)?
        .get(url)
        .send()
        .await
        .map_err(|e| format!("GitHub недоступен: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("GitHub ответил {}", resp.status().as_u16()));
    }
    resp.text().await.map_err(|e| e.to_string())
}

/// Тег последнего стабильного релиза — по перенаправлению, без API.
pub async fn latest_stable_tag(repo: &str) -> Result<String, String> {
    let url = format!("https://github.com/{repo}/releases/latest");
    let resp = client(false)?
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("GitHub недоступен: {e}"))?;
    let location = resp
        .headers()
        .get(reqwest::header::LOCATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    tag_from_location(&location)
        .ok_or_else(|| "GitHub не назвал последнюю версию".to_string())
}

pub async fn entries(repo: &str) -> Result<Vec<Entry>, String> {
    let xml = get_text(&format!("https://github.com/{repo}/releases.atom")).await?;
    let list = entries_from_atom(&xml);
    if list.is_empty() {
        return Err("в ленте релизов пусто".into());
    }
    Ok(list)
}

pub async fn assets(repo: &str, tag: &str) -> Result<Vec<WebAsset>, String> {
    let html =
        get_text(&format!("https://github.com/{repo}/releases/expanded_assets/{tag}")).await?;
    let list = assets_from_html(&html);
    if list.is_empty() {
        return Err("в релизе не видно файлов".into());
    }
    Ok(list)
}

/// Последний стабильный релиз, целиком и без единого запроса к API.
pub async fn latest_release_of(repo: &str, want: fn(&str) -> bool) -> Result<ReleaseInfo, String> {
    let tag = latest_stable_tag(repo).await?;
    let files = assets(repo, &tag).await?;
    // Описание и дату берём из ленты — там они есть для всех свежих релизов.
    // Не нашлись, лента не открылась — не беда: без описания релиз ставится
    // ничуть не хуже, а вот без файла не ставится вовсе
    let found = entries(repo).await.ok().and_then(|list| list.into_iter().find(|e| e.tag == tag));
    let (updated, notes) = found.map(|e| (e.updated, e.notes)).unwrap_or_default();
    to_info(&tag, &updated, &notes, &files, want)
        .ok_or_else(|| "в релизе нет подходящего архива".into())
}

/// Самый свежий релиз, включая предрелизы: лента отдаёт их первыми.
pub async fn latest_release_any(repo: &str, want: fn(&str) -> bool) -> Result<ReleaseInfo, String> {
    let list = entries(repo).await?;
    let mut last = String::from("в релизах нет подходящего архива");
    // Идём по ленте сверху вниз: у самого свежего релиза может не оказаться
    // нужного файла — например, выложили только исходники
    for entry in list.iter().take(10) {
        match assets(repo, &entry.tag).await {
            Ok(files) => {
                if let Some(info) = to_info(&entry.tag, &entry.updated, &entry.notes, &files, want) {
                    return Ok(info);
                }
            }
            Err(e) => last = e,
        }
    }
    Err(last)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Живые ответы GitHub, снятые целиком. Разбор строится на их разметке,
    /// и держать её в тесте — единственный способ заметить, что GitHub её
    /// поменял, раньше, чем это заметят люди.
    const ATOM: &str = include_str!("../tests/fixtures/goodbyedpi-releases.atom");
    const ZAPRET_HTML: &str = include_str!("../tests/fixtures/zapret-assets.html");
    const XRAY_HTML: &str = include_str!("../tests/fixtures/xray-assets.html");

    #[test]
    fn reads_the_tag_out_of_a_redirect() {
        assert_eq!(
            tag_from_location("https://github.com/hufrea/byedpi/releases/tag/v0.17.3").as_deref(),
            Some("v0.17.3")
        );
        assert_eq!(
            tag_from_location("/ValdikSS/GoodbyeDPI/releases/tag/0.2.3rc3").as_deref(),
            Some("0.2.3rc3")
        );
        // Не то перенаправление — лучше честно не знать версию, чем выдумать
        assert!(tag_from_location("https://github.com/login").is_none());
        assert!(tag_from_location("https://github.com/a/b/releases/tag/").is_none());
    }

    /// Лента даёт предрелизы, и именно они там свежие: у GoodbyeDPI
    /// стабильный висит с 2022 года. Перепутать порядок — значит предлагать
    /// человеку версию трёхлетней давности как «последнюю».
    #[test]
    fn atom_gives_prereleases_newest_first() {
        let list = entries_from_atom(ATOM);
        assert!(list.len() >= 3, "в ленте должно быть несколько релизов");
        assert_eq!(list[0].tag, "0.2.3rc3", "свежий релиз — первый");
        assert_eq!(list[0].updated, "2024-09-15", "дата обрезана до дня");
        assert!(list[0].notes.contains("Release candidate 3"), "описание разобрано: {:?}", list[0].notes);
        assert!(!list[0].notes.contains("&lt;"), "экранирование должно быть развёрнуто");
        assert!(!list[0].notes.contains("<p>"), "теги должны быть убраны");
        // Стабильный 0.2.2 в ленте есть, но ниже предрелизов
        assert!(list.iter().any(|e| e.tag == "0.2.2"), "стабильный релиз тоже в ленте");
    }

    /// Имя и прямая ссылка — всё, ради чего страница читается. Ссылка должна
    /// быть пригодна для скачивания как есть: в разметке она записана без
    /// домена, и забыть его — значит получить нерабочий адрес.
    #[test]
    fn reads_assets_with_usable_links() {
        let list = assets_from_html(ZAPRET_HTML);
        let zip = list
            .iter()
            .find(|a| a.name.ends_with(".zip"))
            .expect("архив zip обязан найтись");
        assert_eq!(zip.name, "zapret-discord-youtube-1.10.2.zip");
        assert_eq!(
            zip.url,
            "https://github.com/Flowseal/zapret-discord-youtube/releases/download/1.10.2/zapret-discord-youtube-1.10.2.zip"
        );
        // Каждый файл ровно один раз: в разметке ссылка повторяется
        let zips = list.iter().filter(|a| a.name == zip.name).count();
        assert_eq!(zips, 1, "файлы не должны дублироваться");
    }

    /// Контрольная сумма у Xray лежит рядом файлом `.dgst`. Без неё скачанное
    /// ставится без проверки — терять её на новом пути нельзя.
    #[test]
    fn keeps_the_checksum_next_to_the_archive() {
        let files = assets_from_html(XRAY_HTML);
        let info = to_info("v26.3.27", "2026-03-27", "", &files, |n| n == "Xray-windows-64.zip")
            .expect("архив Xray");
        assert_eq!(info.version, "26.3.27", "префикс v в версии не нужен");
        assert!(info.zip_url.ends_with("/Xray-windows-64.zip"));
        let digest = info.digest.expect("сумма обязана найтись");
        assert_eq!(digest.name, "Xray-windows-64.zip.dgst");
        assert!(digest.url.starts_with("https://github.com/"), "ссылка на сумму: {}", digest.url);
    }

    /// Если подходящего файла нет, честнее вернуть пусто, чем подсунуть
    /// первый попавшийся: поставить не тот архив хуже, чем не поставить.
    #[test]
    fn refuses_to_guess_when_the_archive_is_missing() {
        let files = assets_from_html(ZAPRET_HTML);
        assert!(to_info("1.10.2", "", "", &files, |n| n.ends_with(".msi")).is_none());
    }

    /// Разметка может измениться — тогда разбор обязан вернуть пусто, а не
    /// мусор: пустой список уводит `updater` на запасной путь через API.
    #[test]
    fn returns_nothing_on_unfamiliar_markup() {
        assert!(assets_from_html("<html><body>ничего похожего</body></html>").is_empty());
        assert!(entries_from_atom("<feed></feed>").is_empty());
    }
}
