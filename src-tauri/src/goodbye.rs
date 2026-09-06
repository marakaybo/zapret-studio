//! GoodbyeDPI — третий движок обхода (репозиторий ValdikSS/GoodbyeDPI).
//!
//! По устройству он ближе к zapret, чем к ByeDPI: тоже правит пакеты через
//! драйвер WinDivert и тоже требует прав администратора. Значит, с zapret
//! он одновременно работать не должен — драйвер один на двоих.
//!
//! Отличие от zapret в подходе: у GoodbyeDPI нет папки со стратегиями, весь
//! обход задаётся ключами командной строки, поэтому пресеты живут прямо здесь —
//! как у ByeDPI. Наборы взяты из `.cmd`-файлов самой сборки: они собраны
//! автором под российские блокировки и проверены на практике.

use crate::preset::{self, Proc};
use crate::runner::Runner;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tauri::AppHandle;

pub use crate::preset::{CustomPreset, Preset};

pub const REPO: &str = "ValdikSS/GoodbyeDPI";
pub const EXE: &str = "goodbyedpi.exe";
/// Списки доменов лежат рядом с exe и подставляются через %ROOT%
pub const BLACKLIST: &str = "russia-blacklist.txt";
pub const YOUTUBE_LIST: &str = "russia-youtube.txt";
/// Откуда сам GoodbyeDPI обновляет список заблокированного (0_russia_update_blacklist_file.cmd)
const BLACKLIST_URL: &str = "https://p.thenewone.lol/domains-export.txt";

/// Поддельный ClientHello из комплекта GoodbyeDPI: именно им лечится YouTube.
const FAKE_HEX: &str = "160301FFFF01FFFFFF0303594F5552204144564552544953454D454E542048455245202D202431302F6D6F000000000009000000050003000000";

/// Наборы ключей. Первые четыре — дословно из `.cmd`-файлов сборки, дальше
/// «режимы» -1..-9 из README: они сами разворачиваются в набор опций.
const BUILTIN: &[preset::Builtin] = &[
    (
        "ru-youtube",
        "Россия: список + YouTube",
        "Основной вариант из комплекта GoodbyeDPI: режим 9 плюс поддельный ClientHello, по спискам заблокированного и доменов YouTube. Начинать стоит с него",
        "-9 --fake-gen 5 --fake-from-hex FAKEHEX --blacklist %ROOT%russia-blacklist.txt --blacklist %ROOT%russia-youtube.txt",
    ),
    (
        "ru-youtube-alt",
        "Россия: список + YouTube (ALT)",
        "Запасной вариант из комплекта: режим 5 с автоподбором TTL. Помогает там, где основной не берёт",
        "-5 -e1 -q --fake-gen 5 --fake-from-hex FAKEHEX --blacklist %ROOT%russia-blacklist.txt --blacklist %ROOT%russia-youtube.txt",
    ),
    (
        "ru-blacklist",
        "Россия: только список",
        "Режим 9 по спискам заблокированного, без подмены ClientHello. Мягче к остальному трафику",
        "-9 --blacklist %ROOT%russia-blacklist.txt --blacklist %ROOT%russia-youtube.txt",
    ),
    (
        "ru-dnsredir",
        "Россия: список + свой DNS",
        "То же, что «только список», плюс запросы DNS уводятся на Яндекс на нестандартном порту — на случай, когда провайдер подменяет ответы DNS",
        "-9 --dns-addr 77.88.8.8 --dns-port 1253 --dnsv6-addr 2a02:6b8::feed:0ff --dnsv6-port 1253 --blacklist %ROOT%russia-blacklist.txt --blacklist %ROOT%russia-youtube.txt",
    ),
    (
        "any-country",
        "Любая страна",
        "Режим 9 без списков — обход применяется ко всему трафику подряд. Так GoodbyeDPI работает по умолчанию",
        "-9",
    ),
    (
        "any-country-dnsredir",
        "Любая страна + свой DNS",
        "То же без списков, но с уводом DNS на Яндекс — если провайдер ещё и подменяет DNS",
        "-9 --dns-addr 77.88.8.8 --dns-port 1253 --dnsv6-addr 2a02:6b8::feed:0ff --dnsv6-port 1253",
    ),
    (
        "mode-5",
        "Режим 5: автоподбор TTL",
        "-f 2 -e 2 --auto-ttl --reverse-frag --max-payload. Сам подбирает TTL для поддельных пакетов",
        "-5",
    ),
    (
        "mode-6",
        "Режим 6: неверный номер",
        "-f 2 -e 2 --wrong-seq --reverse-frag --max-payload. Фейк с неправильным номером последовательности",
        "-6",
    ),
    (
        "mode-7",
        "Режим 7: неверная сумма",
        "-f 2 -e 2 --wrong-chksum --reverse-frag --max-payload. Фейк с битой контрольной суммой — до сервера не дойдёт",
        "-7",
    ),
    (
        "mode-1",
        "Режим 1: самый совместимый",
        "-p -r -s -f 2 -k 2 -n -e 2. Старый набор: медленнее, зато работает там, где новые ломают соединение",
        "-1",
    ),
    (
        "mode-4",
        "Режим 4: самый быстрый",
        "-p -r -s. Минимум вмешательства: только пассивный DPI и разрыв заголовка",
        "-4",
    ),
];

/// Длинный шестнадцатеричный фейк вынесен в константу, чтобы строки пресетов
/// оставались читаемыми, — здесь он возвращается на место.
pub fn builtin() -> Vec<Preset> {
    preset::builtin(BUILTIN)
        .into_iter()
        .map(|mut p| {
            p.args = p.args.iter().map(|a| a.replace("FAKEHEX", FAKE_HEX)).collect();
            p
        })
        .collect()
}

pub fn all(custom: &[CustomPreset]) -> Vec<Preset> {
    let mut list = builtin();
    list.extend(preset::all(&[], custom));
    list
}

pub fn find<'a>(all: &'a [Preset], id: &str) -> Option<&'a Preset> {
    preset::find(all, id)
}

// ------------------------------------------------------------------- папка

/// Сборка GoodbyeDPI кладёт exe в подпапку по разрядности; нам нужна x86_64.
pub fn exe_path(root: &Path) -> PathBuf {
    root.join("x86_64").join(EXE)
}

pub fn looks_like_goodbye(dir: &Path) -> bool {
    exe_path(dir).is_file()
}

/// Пользователь мог указать и саму папку сборки, и то, что на уровень выше
/// (типичный случай: распаковали архив «как есть»), и даже x86_64 внутри.
pub fn resolve_root(dir: &Path) -> Option<PathBuf> {
    if looks_like_goodbye(dir) {
        return Some(dir.to_path_buf());
    }
    // указали саму x86_64 — корень на уровень выше
    if dir.join(EXE).is_file() {
        if let Some(parent) = dir.parent() {
            if looks_like_goodbye(parent) {
                return Some(parent.to_path_buf());
            }
        }
    }
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        let p = e.path();
        if p.is_dir() && looks_like_goodbye(&p) {
            return Some(p);
        }
    }
    None
}

pub fn installed_version(root: &Path) -> Option<String> {
    std::fs::read_to_string(root.join("version.txt"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

// ------------------------------------------------------------------ процесс

pub struct Engine {
    proc: Proc,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine {
    pub fn new() -> Self {
        Self { proc: Proc::new(EXE) }
    }

    pub fn is_running(&self) -> bool {
        self.proc.is_running()
    }

    /// id работающего пресета
    pub fn current(&self) -> Option<String> {
        self.proc.current.lock().unwrap().clone()
    }

    pub fn start(
        &self,
        app: &AppHandle,
        runner: &Runner,
        root: &Path,
        preset: &Preset,
        quiet: bool,
    ) -> Result<(), String> {
        // Списки доменов указаны через %ROOT% — разворачиваем в реальные пути,
        // иначе GoodbyeDPI молча запустится без них
        let args = preset::expand_root(&preset.args, root);
        for missing in args
            .iter()
            .skip(1)
            .filter(|a| a.ends_with(".txt") && !Path::new(a).is_file())
        {
            runner.log(Some(app), "err", format!("не найден список {missing}"));
        }
        // TCP timestamps нужны режимам с --auto-ttl ровно так же, как zapret
        runner.enable_tcp_timestamps();
        let announce = (!quiet).then(|| format!("GoodbyeDPI: «{}»", preset.name));
        self.proc
            .start(app, runner, &exe_path(root), root, &args, &preset.id, announce)
    }

    pub fn stop(&self, runner: &Runner, app: Option<&AppHandle>) {
        self.proc.stop(runner, app, "GoodbyeDPI остановлен");
    }
}

// ---------------------------------------------------------------- установка

pub fn is_windows_asset(name: &str) -> bool {
    name.to_lowercase().ends_with(".zip")
}

/// У GoodbyeDPI стабильный релиз висит с 2022 года, а всё живое —
/// в предрелизах, поэтому берём просто самый свежий.
pub async fn latest_release() -> Result<crate::updater::ReleaseInfo, String> {
    crate::updater::latest_release_any(REPO, is_windows_asset).await
}

pub async fn check(current: Option<String>) -> crate::updater::UpdateCheck {
    crate::updater::check_any(current, REPO, is_windows_asset).await
}

/// Скачивает сборку и раскладывает её в target: exe, драйвер и списки доменов.
/// Пользовательский `russia-blacklist.txt` мог обновляться отдельно, поэтому
/// его переносим, если он новее того, что в архиве.
pub async fn install(
    app: &AppHandle,
    release: &crate::updater::ReleaseInfo,
    target: &Path,
    cancel: Arc<AtomicBool>,
) -> Result<String, String> {
    let parent = target.parent().ok_or("некорректный путь установки")?.to_path_buf();
    let staging = parent.join("goodbyedpi-new");
    let backup = parent.join("goodbyedpi-old");
    for p in [&staging, &backup] {
        if p.exists() {
            let _ = std::fs::remove_dir_all(p);
        }
    }

    crate::updater::download_and_extract(app, release, &staging, "goodbyedpi-download.zip", cancel)
        .await?;

    let root = resolve_root(&staging).ok_or_else(|| format!("в архиве не найден x86_64\\{EXE}"))?;
    crate::updater::emit(app, "apply", 92.0, "Переношу списки…");

    if target.exists() {
        // Список заблокированного обновляется чаще самой программы
        for name in [BLACKLIST, YOUTUBE_LIST] {
            let (old, new) = (target.join(name), root.join(name));
            if let (Ok(a), Ok(b)) = (std::fs::metadata(&old), std::fs::metadata(&new)) {
                if a.len() > b.len() {
                    let _ = std::fs::copy(&old, &new);
                }
            }
        }
        std::fs::rename(target, &backup).map_err(|e| format!("не удалось освободить папку: {e}"))?;
    }
    std::fs::rename(&root, target).map_err(|e| format!("не удалось установить: {e}"))?;
    let _ = std::fs::remove_dir_all(&staging);
    let _ = std::fs::remove_dir_all(&backup);
    let _ = std::fs::write(target.join("version.txt"), &release.version);

    crate::updater::emit(app, "done", 100.0, format!("GoodbyeDPI {} установлен", release.version));
    Ok(release.version.clone())
}

/// Обновляет список заблокированных доменов — то же, что делает
/// `0_russia_update_blacklist_file.cmd` из комплекта.
pub async fn update_blacklist(root: &Path) -> Result<usize, String> {
    let text = reqwest::Client::builder()
        .user_agent("ZapretStudio/1.0")
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())?
        .get(BLACKLIST_URL)
        .send()
        .await
        .map_err(|e| format!("список недоступен: {e}"))?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .text()
        .await
        .map_err(|e| e.to_string())?;

    let count = text.lines().filter(|l| !l.trim().is_empty()).count();
    if count == 0 {
        return Err("скачанный список пуст".into());
    }
    std::fs::write(root.join(BLACKLIST), text).map_err(|e| format!("не записать список: {e}"))?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_presets_expand_fake_and_stay_unique() {
        let list = builtin();
        for p in &list {
            assert!(!p.args.is_empty(), "{}: пустые параметры", p.id);
            assert!(p.args[0].starts_with('-'), "{}: {}", p.id, p.args[0]);
            assert!(!p.args.iter().any(|a| a.contains("FAKEHEX")), "{}: фейк не подставлен", p.id);
            assert_eq!(list.iter().filter(|o| o.id == p.id).count(), 1, "дубль id {}", p.id);
        }
        // Основной пресет должен нести и списки, и поддельный ClientHello
        let main = find(&list, "ru-youtube").unwrap();
        assert!(main.args.iter().any(|a| a.contains(FAKE_HEX)));
        assert_eq!(main.args.iter().filter(|a| a.contains("%ROOT%")).count(), 2);
    }

    #[test]
    fn takes_any_zip_from_release() {
        assert!(is_windows_asset("goodbyedpi-0.2.3rc3-2.zip"));
        assert!(!is_windows_asset("Source code (tar.gz)"));
    }

    #[test]
    fn finds_root_from_bundle_or_arch_folder() {
        let outer = std::env::temp_dir().join("zs-gdpi-test");
        let dir = outer.join("goodbyedpi-0.2.3rc3-2");
        let arch = dir.join("x86_64");
        let _ = std::fs::remove_dir_all(&outer);
        std::fs::create_dir_all(&arch).unwrap();
        std::fs::write(arch.join(EXE), b"stub").unwrap();

        assert_eq!(resolve_root(&dir).unwrap(), dir);
        // указали x86_64 — поднимаемся к настоящему корню
        assert_eq!(resolve_root(&arch).unwrap(), dir);
        // указали папку, куда распаковали архив, — спускаемся на уровень внутрь
        assert_eq!(resolve_root(&outer).unwrap(), dir);
        let _ = std::fs::remove_dir_all(&outer);
    }
}
