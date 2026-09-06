//! Обновление самой Zapret Studio.
//!
//! Приложение следит за пятью чужими программами и до сих пор не следило за
//! собой: скачавший сборку однажды так и оставался на ней навсегда. Механика
//! та же, что у остальных, — GitHub Releases, — но с двумя отличиями.
//!
//! Первое: в релизе лежит не архив, а установщик NSIS, распаковывать нечего.
//! Его надо скачать, сверить контрольную сумму и запустить, а самому уйти —
//! иначе установщик упрётся в занятые файлы.
//!
//! Второе: обновление приложения никогда не ставится само, даже когда
//! «ставить обновления сразу» включено. Обновление чужого обхода в худшем
//! случае вернёт прежнее поведение, а тут мы запускаем скачанный
//! исполняемый файл — такое делают по нажатию человека, а не по таймеру.

use crate::updater::{self, ReleaseInfo, UpdateCheck};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tauri::AppHandle;

/// Репозиторий с релизами приложения. Своего репозитория у сборки может и не
/// быть — тогда проверка честно скажет, что релизов не нашла, а адрес можно
/// поменять в настройках (`appRepo`).
pub const REPO: &str = "marakaybo/zapret-studio";

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Что искать в релизе: установщик, который собирает Tauri, называется
/// вроде `Zapret Studio_1.1.0_x64-setup.exe`.
pub fn is_installer(name: &str) -> bool {
    let n = name.to_lowercase();
    (n.ends_with(".exe") && n.contains("setup")) || n.ends_with(".msi")
}

pub async fn latest(repo: &str) -> Result<ReleaseInfo, String> {
    updater::latest_release_of(repo, is_installer).await
}

pub async fn check(repo: &str) -> UpdateCheck {
    let mut result = updater::check_repo(Some(version().to_string()), repo, is_installer).await;
    // «Релизов нет» звучит как поломка, хотя обычно это просто не настроенный
    // репозиторий — говорим об этом прямо
    if let Some(e) = &result.error {
        if e.contains("404") {
            result.error = Some(format!(
                "Репозиторий {repo} не отвечает про релизы. Проверь адрес в настройках приложения"
            ));
        }
    }
    result
}

/// Куда класть установщик: рядом с настройками, а не во временную папку —
/// так его видно, если что-то пойдёт не так.
pub fn dir(data_dir: &Path) -> PathBuf {
    data_dir.join("update")
}

/// Скачивает установщик и сверяет контрольную сумму, если она опубликована.
/// Возвращает путь к файлу и, если проверить сумму не вышло, — почему.
pub async fn download(
    app: &AppHandle,
    release: &ReleaseInfo,
    data_dir: &Path,
    cancel: Arc<AtomicBool>,
) -> Result<(PathBuf, Option<String>), String> {
    let dir = dir(data_dir);
    // Прошлые установщики только занимают место и путают
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let name = release
        .zip_url
        .rsplit('/')
        .next()
        .filter(|n| !n.is_empty())
        .unwrap_or("zapret-studio-setup.exe");
    let path = dir.join(name);

    let note = updater::download_release_file(app, release, &path, cancel).await?;
    Ok((path, note))
}

/// Запускает установщик. Приложение после этого обязано выйти: NSIS не сможет
/// заменить файлы, пока они заняты работающим процессом.
pub fn launch(path: &Path) -> Result<(), String> {
    if !path.is_file() {
        return Err("установщик не найден".into());
    }
    std::process::Command::new(path)
        .spawn()
        .map_err(|e| format!("не удалось запустить установщик: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_only_the_installer() {
        assert!(is_installer("Zapret Studio_1.1.0_x64-setup.exe"));
        assert!(is_installer("zapret-studio_1.1.0_x64_en-US.msi"));
        assert!(!is_installer("zapret-studio_1.1.0_x64-setup.exe.sig"));
        assert!(!is_installer("Source code.zip"));
        // Просто exe без «setup» — это, скорее всего, портативная сборка
        assert!(!is_installer("zapret-studio.exe"));
    }

    /// Версия приложения должна быть настоящей: по ней решается, обновляться
    /// ли, и «0.0.0» тихо превратило бы каждый запуск в предложение обновиться.
    #[test]
    fn reports_its_own_version() {
        let v = version();
        assert!(v.split('.').count() >= 2, "странная версия: {v}");
        assert!(v.chars().next().is_some_and(|c| c.is_ascii_digit()), "{v}");
    }
}
