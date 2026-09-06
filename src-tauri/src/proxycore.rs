//! Общее для прокси-ядер: Xray и sing-box.
//!
//! Они устроены иначе, чем остальные движки. zapret и GoodbyeDPI правят пакеты
//! драйвером, ByeDPI ломает запросы своими ключами командной строки, а ядру
//! нужен конфиг — файл JSON, который описывает и то, куда оно слушает, и то,
//! что делает с трафиком. Поэтому «пресет» здесь означает не набор ключей, а
//! способ собрать этот файл: приложение пишет config.json рядом с ядром и
//! запускает его.
//!
//! Второе отличие — ядро умеет не только обходить блокировку на месте
//! (фрагментация TLS), но и увести трафик на чужой сервер по ссылке
//! `vless://` и подобным (см. `link.rs`). Первое работает у всех сразу,
//! второе — только если сервер есть.
//!
//! Наружу оба ядра выглядят как ByeDPI: локальный прокси на 127.0.0.1, в
//! который трафик попадает через системные настройки Windows.

use crate::preset::{Preset, Proc};
use crate::runner::Runner;
use crate::updater::{ReleaseInfo, UpdateCheck};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::AppHandle;

/// Всё, чем одно ядро отличается от другого: где живёт, как называется и как
/// его запускать.
#[derive(Clone, Copy)]
pub struct Spec {
    /// Он же значение `engine` в настройках: xray | singbox
    pub id: &'static str,
    pub name: &'static str,
    pub exe: &'static str,
    pub repo: &'static str,
    pub config_name: &'static str,
    /// Ключи запуска: (конфиг, папка ядра) -> аргументы
    pub args: fn(&Path, &Path) -> Vec<String>,
    /// Какой файл релиза брать с GitHub
    pub asset: fn(&str) -> bool,
}

pub fn exe_path(spec: &Spec, root: &Path) -> PathBuf {
    root.join(spec.exe)
}

pub fn installed(spec: &Spec, dir: &Path) -> bool {
    exe_path(spec, dir).is_file()
}

/// Архивы у ядер устроены по-разному: у Xray файлы лежат в корне, у sing-box —
/// в папке с именем версии. Смотрим на уровень глубже, если в корне пусто.
pub fn resolve_root(spec: &Spec, dir: &Path) -> Option<PathBuf> {
    if installed(spec, dir) {
        return Some(dir.to_path_buf());
    }
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        let p = e.path();
        if p.is_dir() && installed(spec, &p) {
            return Some(p);
        }
    }
    None
}

/// Ни `xray version`, ни `sing-box version` не спросить, пока файла нет,
/// поэтому версию кладём меткой при установке — как у ByeDPI.
pub fn installed_version(root: &Path) -> Option<String> {
    std::fs::read_to_string(root.join("version.txt"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

// ------------------------------------------------------------------ процесс

/// Запущенное ядро: дочерний процесс плюс порт, на котором оно слушает.
pub struct Core {
    pub spec: Spec,
    proc: Proc,
    pub port: Mutex<u16>,
}

impl Core {
    pub fn new(spec: Spec, default_port: u16) -> Self {
        Self { spec, proc: Proc::new(spec.exe), port: Mutex::new(default_port) }
    }

    pub fn is_running(&self) -> bool {
        self.proc.is_running()
    }

    /// id работающего пресета
    pub fn current(&self) -> Option<String> {
        self.proc.current.lock().unwrap().clone()
    }

    /// Пишет конфиг рядом с ядром и поднимает процесс. Конфиг именно файлом,
    /// а не через стандартный ввод: так его видно глазами, когда что-то не
    /// работает, — и Xray, и sing-box в логах ссылаются на строки файла.
    pub fn start(
        &self,
        app: &AppHandle,
        runner: &Runner,
        root: &Path,
        preset: &Preset,
        config: &serde_json::Value,
        port: u16,
        quiet: bool,
    ) -> Result<(), String> {
        let cfg_path = root.join(self.spec.config_name);
        let text = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
        std::fs::write(&cfg_path, text)
            .map_err(|e| format!("не удалось записать {}: {e}", cfg_path.display()))?;

        let args = (self.spec.args)(&cfg_path, root);
        let announce =
            (!quiet).then(|| format!("{}: «{}» на 127.0.0.1:{port}", self.spec.name, preset.name));
        self.proc
            .start(app, runner, &exe_path(&self.spec, root), root, &args, &preset.id, announce)
            .map_err(|e| {
                if e.contains("завершился сразу") {
                    format!(
                        "{} не запустился — скорее всего занят порт {port} или конфиг ему не понравился, смотри журнал",
                        self.spec.exe
                    )
                } else {
                    e
                }
            })?;

        // Процесс жив — это ещё не значит, что прокси готов принимать
        // соединения: ядро успевает подняться на полсекунды позже, а
        // проверка стратегий стучится сразу и получила бы отказ.
        if !wait_port(port, Duration::from_secs(5)) {
            self.proc.stop(runner, Some(app), "Ядро не открыло порт — останавливаю");
            return Err(format!(
                "{} запустился, но не слушает 127.0.0.1:{port} — смотри журнал",
                self.spec.name
            ));
        }
        *self.port.lock().unwrap() = port;
        Ok(())
    }

    pub fn stop(&self, runner: &Runner, app: Option<&AppHandle>) {
        self.proc.stop(runner, app, &format!("{} остановлен", self.spec.name));
    }
}

/// Ждёт, пока по адресу начнут отвечать. Дешевле и честнее, чем спать
/// фиксированную паузу: обычно порт готов за 200–300 мс.
fn wait_port(port: u16, limit: Duration) -> bool {
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let started = Instant::now();
    while started.elapsed() < limit {
        if TcpStream::connect_timeout(&addr, Duration::from_millis(250)).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(120));
    }
    false
}

// ---------------------------------------------------------------- установка

pub async fn latest_release(spec: &Spec) -> Result<ReleaseInfo, String> {
    crate::updater::latest_release_of(spec.repo, spec.asset).await
}

pub async fn check(spec: &Spec, current: Option<String>) -> UpdateCheck {
    crate::updater::check_repo(current, spec.repo, spec.asset).await
}

/// Копирует всё, что ядро принесло в архиве: сам exe и файлы рядом с ним —
/// у Xray это geoip.dat и geosite.dat, без них не работают правила по стране.
fn copy_tree(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(from).map_err(|e| e.to_string())?.flatten() {
        let src = entry.path();
        let dst = to.join(entry.file_name());
        if src.is_dir() {
            copy_tree(&src, &dst)?;
        } else {
            std::fs::copy(&src, &dst)
                .map_err(|e| format!("не удалось положить {}: {e}", dst.display()))?;
        }
    }
    Ok(())
}

/// Скачивает релиз ядра и раскладывает его в target. Конфиг приложение пишет
/// само при каждом запуске, поэтому переносить между версиями нечего.
pub async fn install(
    spec: &Spec,
    app: &AppHandle,
    release: &ReleaseInfo,
    target: &Path,
    cancel: Arc<AtomicBool>,
) -> Result<String, String> {
    let parent = target.parent().ok_or("некорректный путь установки")?.to_path_buf();
    let staging = parent.join(format!("{}-new", spec.id));
    if staging.exists() {
        let _ = std::fs::remove_dir_all(&staging);
    }

    crate::updater::download_and_extract(
        app,
        release,
        &staging,
        &format!("{}-download.zip", spec.id),
        cancel,
    )
    .await?;

    let root = resolve_root(spec, &staging).ok_or_else(|| format!("в архиве не найден {}", spec.exe))?;
    crate::updater::emit(app, "apply", 92.0, "Устанавливаю…");

    let result = copy_tree(&root, target);
    let _ = std::fs::remove_dir_all(&staging);
    result?;

    let _ = std::fs::write(target.join("version.txt"), &release.version);
    crate::updater::emit(
        app,
        "done",
        100.0,
        format!("{} {} установлен", spec.name, release.version),
    );
    Ok(release.version.clone())
}

// ------------------------------------------------------------------ пресеты

/// Пресет ядра: тот же тип, что у остальных движков, но в `args` лежит не
/// командная строка, а короткое описание того, что окажется в конфиге, —
/// показывать пользователю сырой JSON смысла нет.
pub type Builtin = (&'static str, &'static str, &'static str, &'static str);

pub fn presets(list: &[Builtin]) -> Vec<Preset> {
    list.iter()
        .map(|(id, name, desc, shape)| Preset {
            id: (*id).into(),
            name: (*name).into(),
            desc: (*desc).into(),
            args: vec![(*shape).to_string()],
            builtin: true,
        })
        .collect()
}

pub fn find<'a>(all: &'a [Preset], id: &str) -> Option<&'a Preset> {
    all.iter().find(|p| p.id == id)
}

/// Пресеты, которые без сервера бессмысленны, названы одинаково у обоих ядер.
pub fn needs_server(id: &str) -> bool {
    id.starts_with("server")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tells_server_presets_apart() {
        assert!(needs_server("server"));
        assert!(needs_server("server-frag"));
        assert!(!needs_server("frag-tlshello"));
    }

    #[test]
    fn port_wait_gives_up_instead_of_hanging() {
        let started = Instant::now();
        // 1 — порт, который никто не слушает: ждать дольше срока нельзя
        assert!(!wait_port(1, Duration::from_millis(400)));
        assert!(started.elapsed() < Duration::from_secs(3), "{:?}", started.elapsed());
    }
}
