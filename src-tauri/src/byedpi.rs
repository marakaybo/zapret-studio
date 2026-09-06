//! ByeDPI — второй движок обхода.
//!
//! В отличие от zapret он не ставит драйвер и не требует прав администратора:
//! это обычная программа `ciadpi.exe` (репозиторий hufrea/byedpi), которая
//! поднимает локальный SOCKS5-прокси и ломает исходящие запросы так, чтобы DPI
//! не смог их разобрать. Трафик в неё попадает через системный прокси Windows —
//! его приложение прописывает и снимает само (см. `sysproxy`).
//!
//! Отсюда разница в подходе: стратегии zapret живут в `.bat`-файлах сборки,
//! а здесь набор параметров задаётся прямо в приложении — встроенные пресеты
//! плюс свои, которые пользователь дописывает руками.

use crate::preset::{self, Proc};
use crate::runner::Runner;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use tauri::AppHandle;

pub use crate::preset::{CustomPreset, Preset};

pub const REPO: &str = "hufrea/byedpi";
pub const EXE: &str = "ciadpi.exe";

/// Встроенные наборы параметров. Все взяты из README ByeDPI — в первую очередь
/// из раздела «на практике оптимально использовать» для Windows: там
/// ретрансмиссия работает иначе, чем в Linux, поэтому `disorder` просят
/// сочетать со `split`, а `fake` — с `disorder`.
const BUILTIN: &[preset::Builtin] = &[
    (
        "split-disorder",
        "Разрыв по SNI",
        "Рекомендация ByeDPI для Windows: режем запрос на имени сайта и отправляем куски не по порядку. Начинать стоит с неё",
        "--split 1+s --disorder 3+s",
    ),
    (
        "fake-disorder",
        "Фейк + беспорядок",
        "Перед настоящим запросом уходит поддельный с коротким TTL: DPI его видит, сервер — нет",
        "--disorder 1 --fake -1 --ttl 8",
    ),
    (
        "tlsrec-sni",
        "Разрыв TLS-записи",
        "Заголовок новой TLS-записи вставляется в середину имени сайта — DPI не может его прочитать",
        "--tlsrec 3+s",
    ),
    (
        "auto-tlsrec",
        "Только при блокировке",
        "Ничего не трогает, пока соединение живое; ломает запрос лишь после сброса или таймаута. Самая щадящая",
        "--auto=torst --timeout 3 --tlsrec 3+s",
    ),
    (
        "split-mid-sni",
        "Разрыв в середине SNI",
        "Один разрез ровно посередине имени сайта — минимум вмешательства в трафик",
        "--split 0+sm",
    ),
    (
        "oob-sni",
        "OOB-байт в SNI",
        "В имя сайта подкладывается байт вне основного потока (флаг URG). Помогает там, где split уже не берёт",
        "--auto=torst --timeout 3 --oob 3+s",
    ),
    (
        "disoob",
        "Беспорядок с OOB",
        "Куски идут не по порядку, на месте разрыва — внеканальный байт",
        "--disoob 3+s --disorder 7+s",
    ),
    (
        "fake-ssl-err",
        "Фейк с запасным TTL",
        "Пример из README: по умолчанию TTL 10, а если в ответ не пришёл ServerHello — повтор с TTL 5",
        "--fake -1 --ttl 10 --auto=ssl_err --fake -1 --ttl 5",
    ),
    (
        "proto-only",
        "Только HTTP и TLS",
        "Запутывает лишь веб-трафик, остальные протоколы идут как есть — меньше шансов что-то сломать",
        "--proto=http,tls --split 1+s --disorder 3+s --auto=none",
    ),
    (
        "hard",
        "Всё сразу",
        "Разрыв, беспорядок, фейк и разрез TLS-записи одновременно. Последнее средство, когда мягкие не берут",
        "--split 1+s --disorder 3+s --fake -1 --ttl 8 --tlsrec 3+s",
    ),
];

pub fn builtin() -> Vec<Preset> {
    preset::builtin(BUILTIN)
}

pub fn all(custom: &[CustomPreset]) -> Vec<Preset> {
    preset::all(BUILTIN, custom)
}

pub fn find<'a>(all: &'a [Preset], id: &str) -> Option<&'a Preset> {
    preset::find(all, id)
}

/// К параметрам пресета добавляем свои: слушать только петлевой адрес
/// (по умолчанию ciadpi слушает 0.0.0.0, то есть открыт всей локальной сети)
/// и порт из настроек.
pub fn full_args(preset: &[String], port: u16) -> Vec<String> {
    let mut args = vec![
        "--ip".to_string(),
        "127.0.0.1".to_string(),
        "--port".to_string(),
        port.to_string(),
    ];
    args.extend(preset.iter().cloned());
    args
}

pub fn exe_path(root: &Path) -> PathBuf {
    root.join(EXE)
}

pub fn looks_like_byedpi(dir: &Path) -> bool {
    exe_path(dir).is_file()
}

/// В архиве ByeDPI лежит один файл, но пользователь мог распаковать его
/// в подпапку — смотрим на уровень глубже.
pub fn resolve_root(dir: &Path) -> Option<PathBuf> {
    if looks_like_byedpi(dir) {
        return Some(dir.to_path_buf());
    }
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        let p = e.path();
        if p.is_dir() && looks_like_byedpi(&p) {
            return Some(p);
        }
    }
    None
}

/// У ciadpi.exe нет ключа для вывода версии, поэтому при установке кладём
/// рядом метку и читаем её.
pub fn installed_version(root: &Path) -> Option<String> {
    std::fs::read_to_string(root.join("version.txt"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

// ------------------------------------------------------------------ процесс

/// Локальный SOCKS5-прокси: тот же дочерний процесс, что и у GoodbyeDPI,
/// плюс порт — его надо помнить, чтобы вернуть системные настройки.
pub struct Proxy {
    proc: Proc,
    pub port: Mutex<u16>,
}

impl Default for Proxy {
    fn default() -> Self {
        Self::new()
    }
}

impl Proxy {
    pub fn new() -> Self {
        Self { proc: Proc::new(EXE), port: Mutex::new(1080) }
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
        port: u16,
        quiet: bool,
    ) -> Result<(), String> {
        let args = full_args(&preset.args, port);
        let announce = (!quiet).then(|| format!("ByeDPI: «{}» на 127.0.0.1:{port}", preset.name));
        self.proc
            .start(app, runner, &exe_path(root), root, &args, &preset.id, announce)
            .map_err(|e| {
                // Занятый порт — самая частая причина, а из текста ciadpi это не видно
                if e.contains("завершился сразу") {
                    format!("{EXE} не запустился — скорее всего занят порт {port} или в параметрах ошибка, смотри журнал")
                } else {
                    e
                }
            })?;
        *self.port.lock().unwrap() = port;
        Ok(())
    }

    pub fn stop(&self, runner: &Runner, app: Option<&AppHandle>) {
        self.proc.stop(runner, app, "ByeDPI остановлен");
    }
}

// ---------------------------------------------------------------- установка

/// В релизе лежат сборки под все платформы — нам нужна 64-битная под Windows.
pub fn is_windows_asset(name: &str) -> bool {
    let n = name.to_lowercase();
    n.ends_with(".zip") && n.contains("x86_64") && n.contains("w64")
}

pub async fn latest_release() -> Result<crate::updater::ReleaseInfo, String> {
    crate::updater::latest_release_of(REPO, is_windows_asset).await
}

pub async fn check(current: Option<String>) -> crate::updater::UpdateCheck {
    crate::updater::check_repo(current, REPO, is_windows_asset).await
}

/// Скачивает архив с ciadpi.exe и раскладывает его в target.
/// Своих файлов у ByeDPI нет, поэтому переносить между версиями нечего.
pub async fn install(
    app: &AppHandle,
    release: &crate::updater::ReleaseInfo,
    target: &Path,
    cancel: Arc<AtomicBool>,
) -> Result<String, String> {
    let parent = target.parent().ok_or("некорректный путь установки")?.to_path_buf();
    let staging = parent.join("byedpi-new");
    if staging.exists() {
        let _ = std::fs::remove_dir_all(&staging);
    }

    crate::updater::download_and_extract(app, release, &staging, "byedpi-download.zip", cancel).await?;

    let root = resolve_root(&staging).ok_or_else(|| format!("в архиве не найден {EXE}"))?;
    crate::updater::emit(app, "apply", 92.0, "Устанавливаю…");

    std::fs::create_dir_all(target).map_err(|e| e.to_string())?;
    std::fs::copy(exe_path(&root), exe_path(target))
        .map_err(|e| format!("не удалось положить {EXE}: {e}"))?;
    let _ = std::fs::write(target.join("version.txt"), &release.version);
    let _ = std::fs::remove_dir_all(&staging);

    crate::updater::emit(app, "done", 100.0, format!("ByeDPI {} установлен", release.version));
    Ok(release.version.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn always_binds_loopback_and_configured_port() {
        let args = full_args(&preset::parse_args("--disorder 1"), 1081);
        assert_eq!(args[..4], ["--ip", "127.0.0.1", "--port", "1081"]);
        assert_eq!(args.last().unwrap(), "1");
    }

    #[test]
    fn builtin_presets_are_unique_and_parse() {
        let list = builtin();
        for p in &list {
            assert!(!p.args.is_empty(), "{}: пустые параметры", p.id);
            assert!(p.args[0].starts_with('-'), "{}: {}", p.id, p.args[0]);
            assert_eq!(list.iter().filter(|o| o.id == p.id).count(), 1, "дубль id {}", p.id);
        }
    }

    #[test]
    fn picks_only_the_windows_x64_asset() {
        assert!(is_windows_asset("byedpi-17.3-x86_64-w64.zip"));
        assert!(!is_windows_asset("byedpi-17.3-i686-w64.zip"));
        assert!(!is_windows_asset("byedpi-17.3-x86_64.tar.gz"));
    }
}
