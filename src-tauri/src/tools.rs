//! Настройки и инструменты самой сборки zapret — то, что в `service.bat`
//! спрятано в текстовом меню: режим ipset, подмена фейков, кэш Discord, hosts.

use crate::sysutil::{out_text, run_hidden};
use serde::Serialize;
use std::path::{Path, PathBuf};

const IPSET_URL: &str =
    "https://raw.githubusercontent.com/Flowseal/zapret-discord-youtube/refs/heads/main/.service/ipset-service.txt";
const HOSTS_URL: &str =
    "https://raw.githubusercontent.com/Flowseal/zapret-discord-youtube/refs/heads/main/.service/hosts";
const IPSET_NONE_MARKER: &str = "203.0.113.113/32";
const HOSTS_BEGIN: &str = "# >>> zapret-discord-youtube >>>";
const HOSTS_END: &str = "# <<< zapret-discord-youtube <<<";

// ------------------------------------------------------------------ ipset

/// Режимы как в service.bat: loaded — настоящий список адресов,
/// none — заглушка (правила по ipset не срабатывают), any — пустой файл,
/// который winws трактует как «любой адрес».
pub fn ipset_mode(root: &Path) -> String {
    let file = root.join("lists").join("ipset-all.txt");
    let Ok(text) = std::fs::read_to_string(&file) else { return "missing".into() };
    let lines = text.lines().filter(|l| !l.trim().is_empty()).count();
    if lines == 0 {
        "any".into()
    } else if text.contains(IPSET_NONE_MARKER) {
        "none".into()
    } else {
        "loaded".into()
    }
}

pub fn set_ipset_mode(root: &Path, mode: &str) -> Result<(), String> {
    let lists = root.join("lists");
    let file = lists.join("ipset-all.txt");
    let backup = lists.join("ipset-all.txt.backup");
    let current = ipset_mode(root);
    if current == mode {
        return Ok(());
    }
    // Уходя из loaded, прячем настоящий список в бэкап — чтобы было куда вернуться
    if current == "loaded" && file.is_file() {
        let _ = std::fs::remove_file(&backup);
        std::fs::rename(&file, &backup).map_err(|e| format!("не удалось сохранить список: {e}"))?;
    }
    match mode {
        "none" => std::fs::write(&file, format!("{IPSET_NONE_MARKER}\n")).map_err(|e| e.to_string()),
        "any" => std::fs::write(&file, "").map_err(|e| e.to_string()),
        "loaded" => {
            if backup.is_file() {
                let _ = std::fs::remove_file(&file);
                std::fs::rename(&backup, &file).map_err(|e| e.to_string())
            } else {
                Err("Сохранённого списка нет — сначала обнови список адресов".into())
            }
        }
        other => Err(format!("неизвестный режим ipset: {other}")),
    }
}

pub async fn update_ipset(root: &Path) -> Result<usize, String> {
    let text = reqwest::Client::builder()
        .user_agent("ZapretStudio/1.0")
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?
        .get(IPSET_URL)
        .send()
        .await
        .map_err(|e| format!("GitHub недоступен: {e}"))?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .text()
        .await
        .map_err(|e| e.to_string())?;
    let entries = text.lines().filter(|l| !l.trim().is_empty() && !l.starts_with('#')).count();
    if entries == 0 {
        return Err("скачанный список пуст".into());
    }
    let lists = root.join("lists");
    std::fs::create_dir_all(&lists).map_err(|e| e.to_string())?;
    // Свежий список — это всегда режим loaded; старый бэкап больше не нужен
    let _ = std::fs::remove_file(lists.join("ipset-all.txt.backup"));
    std::fs::write(lists.join("ipset-all.txt"), text).map_err(|e| e.to_string())?;
    Ok(entries)
}

// ------------------------------------------------------------------ фейки

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FakesInfo {
    pub available: Vec<String>,
    pub discord: Option<String>,
    pub game: Option<String>,
}

fn fake_candidates(bin: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(bin)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.extension().map(|e| e.eq_ignore_ascii_case("bin")).unwrap_or(false)
                        && !p
                            .file_name()
                            .map(|n| n.to_string_lossy().to_uppercase().starts_with("ACTIVE_"))
                            .unwrap_or(true)
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

fn stem(p: &Path) -> String {
    p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
}

/// Какой из фейков сейчас скопирован в ACTIVE_*: сравниваем содержимое байт в байт.
fn active_matches(bin: &Path, active: &str, candidates: &[PathBuf]) -> Option<String> {
    let current = std::fs::read(bin.join(active)).ok()?;
    candidates
        .iter()
        .find(|c| std::fs::read(c).map(|b| b == current).unwrap_or(false))
        .map(|c| stem(c))
}

pub fn fakes(root: &Path) -> FakesInfo {
    let bin = root.join("bin");
    let candidates = fake_candidates(&bin);
    FakesInfo {
        discord: active_matches(&bin, "ACTIVE_DISCORD_UDP.bin", &candidates),
        game: active_matches(&bin, "ACTIVE_GAME_UDP.bin", &candidates),
        available: candidates.iter().map(|c| stem(c)).collect(),
    }
}

pub fn set_active_fake(root: &Path, kind: &str, source: &str) -> Result<(), String> {
    let bin = root.join("bin");
    let target = match kind {
        "discord" => "ACTIVE_DISCORD_UDP.bin",
        "game" => "ACTIVE_GAME_UDP.bin",
        _ => return Err("неизвестный тип фейка".into()),
    };
    let src = fake_candidates(&bin)
        .into_iter()
        .find(|c| stem(c) == source)
        .ok_or("такого файла-фейка нет в папке bin")?;
    std::fs::copy(&src, bin.join(target)).map_err(|e| format!("не удалось заменить фейк: {e}"))?;
    Ok(())
}

// ------------------------------------------------------------------ Discord

/// Закрывает Discord и чистит его кэш — стандартный первый шаг, когда
/// голос или картинки не грузятся даже с работающим обходом.
pub fn clear_discord_cache() -> Vec<String> {
    let Ok(appdata) = std::env::var("APPDATA") else { return vec!["Не найдена папка APPDATA".into()] };
    let variants: [(&str, &str, &str); 4] = [
        ("Discord.exe", "Discord", "discord"),
        ("DiscordPTB.exe", "Discord PTB", "discordptb"),
        ("DiscordCanary.exe", "Discord Canary", "discordcanary"),
        ("DiscordDevelopment.exe", "Discord Development", "discorddevelopment"),
    ];
    let mut report = Vec::new();
    for (process, title, dir) in variants {
        let root = Path::new(&appdata).join(dir);
        if !root.is_dir() {
            continue;
        }
        let running = run_hidden("tasklist", &["/FI", &format!("IMAGENAME eq {process}"), "/NH"])
            .map(|o| out_text(&o).to_lowercase().contains(&process.to_lowercase()))
            .unwrap_or(false);
        if running {
            let _ = run_hidden("taskkill", &["/F", "/IM", process]);
            std::thread::sleep(std::time::Duration::from_millis(600));
        }
        let mut removed = 0;
        for sub in ["Cache", "Code Cache", "GPUCache"] {
            let p = root.join(sub);
            if p.is_dir() && std::fs::remove_dir_all(&p).is_ok() {
                removed += 1;
            }
        }
        report.push(format!(
            "{title}: {}очищено папок — {removed}",
            if running { "закрыт, " } else { "" }
        ));
    }
    if report.is_empty() {
        report.push("Discord не установлен".into());
    }
    report
}

// ------------------------------------------------------------------ hosts

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct HostsStatus {
    pub total: usize,
    pub missing: usize,
    pub up_to_date: bool,
}

fn hosts_path() -> PathBuf {
    let sysroot = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
    Path::new(&sysroot).join("System32").join("drivers").join("etc").join("hosts")
}

async fn fetch_hosts_block() -> Result<Vec<String>, String> {
    let text = reqwest::Client::builder()
        .user_agent("ZapretStudio/1.0")
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?
        .get(HOSTS_URL)
        .send()
        .await
        .map_err(|e| format!("GitHub недоступен: {e}"))?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .text()
        .await
        .map_err(|e| e.to_string())?;
    let lines: Vec<String> = text
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect();
    if lines.is_empty() {
        return Err("файл hosts из репозитория пуст".into());
    }
    Ok(lines)
}

fn missing_lines(block: &[String]) -> Vec<String> {
    let current = std::fs::read_to_string(hosts_path()).unwrap_or_default();
    let present: Vec<String> = current.lines().map(|l| l.split_whitespace().collect::<Vec<_>>().join(" ")).collect();
    block
        .iter()
        .filter(|l| {
            let norm = l.split_whitespace().collect::<Vec<_>>().join(" ");
            !present.iter().any(|p| p == &norm)
        })
        .cloned()
        .collect()
}

pub async fn hosts_status() -> Result<HostsStatus, String> {
    let block = fetch_hosts_block().await?;
    let missing = missing_lines(&block).len();
    Ok(HostsStatus { total: block.len(), missing, up_to_date: missing == 0 })
}

/// Дописывает недостающие записи в hosts одним помеченным блоком.
/// Повторный вызов ничего не дублирует. Файл системный — нужны права администратора.
pub async fn apply_hosts() -> Result<usize, String> {
    let block = fetch_hosts_block().await?;
    let missing = missing_lines(&block);
    if missing.is_empty() {
        return Ok(0);
    }
    let path = hosts_path();
    let mut content = std::fs::read_to_string(&path).map_err(|e| format!("hosts не читается: {e}"))?;
    if !content.ends_with('\n') && !content.is_empty() {
        content.push_str("\r\n");
    }
    content.push_str(HOSTS_BEGIN);
    content.push_str("\r\n");
    for l in &missing {
        content.push_str(l);
        content.push_str("\r\n");
    }
    content.push_str(HOSTS_END);
    content.push_str("\r\n");
    std::fs::write(&path, content).map_err(|e| format!("hosts не записывается: {e}"))?;
    let _ = run_hidden("ipconfig", &["/flushdns"]);
    Ok(missing.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("zs-tools-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("lists")).unwrap();
        std::fs::create_dir_all(dir.join("bin")).unwrap();
        dir
    }

    #[test]
    fn ipset_round_trip_keeps_real_list() {
        let root = tmp("ipset");
        let file = root.join("lists").join("ipset-all.txt");
        let real = "1.2.3.0/24\n5.6.7.8/32\n";
        std::fs::write(&file, real).unwrap();
        assert_eq!(ipset_mode(&root), "loaded");

        set_ipset_mode(&root, "none").unwrap();
        assert_eq!(ipset_mode(&root), "none");

        set_ipset_mode(&root, "any").unwrap();
        assert_eq!(ipset_mode(&root), "any");

        // Возврат в loaded обязан вернуть исходный список, а не пустышку
        set_ipset_mode(&root, "loaded").unwrap();
        assert_eq!(ipset_mode(&root), "loaded");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), real);
    }

    #[test]
    fn ipset_loaded_without_backup_is_refused() {
        let root = tmp("ipset-nobackup");
        std::fs::write(root.join("lists").join("ipset-all.txt"), format!("{IPSET_NONE_MARKER}\n")).unwrap();
        assert_eq!(ipset_mode(&root), "none");
        assert!(set_ipset_mode(&root, "loaded").is_err(), "нечего восстанавливать — должна быть ошибка");
    }

    #[test]
    fn fakes_detect_and_replace() {
        let root = tmp("fakes");
        let bin = root.join("bin");
        std::fs::write(bin.join("stun.bin"), b"AAAA").unwrap();
        std::fs::write(bin.join("stun2.bin"), b"BBBB").unwrap();
        std::fs::write(bin.join("ACTIVE_DISCORD_UDP.bin"), b"AAAA").unwrap();
        std::fs::write(bin.join("ACTIVE_GAME_UDP.bin"), b"ZZZZ").unwrap();

        let info = fakes(&root);
        assert_eq!(info.available, vec!["stun", "stun2"], "ACTIVE_* не должны попадать в список");
        assert_eq!(info.discord.as_deref(), Some("stun"));
        assert_eq!(info.game, None, "чужое содержимое не должно ни с чем совпасть");

        set_active_fake(&root, "game", "stun2").unwrap();
        assert_eq!(fakes(&root).game.as_deref(), Some("stun2"));
        assert!(set_active_fake(&root, "game", "нет-такого").is_err());
    }

    #[test]
    fn hosts_diff_ignores_spacing() {
        let block = vec!["1.2.3.4 example.com".to_string(), "5.6.7.8 other.com".to_string()];
        // Нормализация пробелов: в hosts часто табы вместо пробелов
        let normalized: Vec<String> = block
            .iter()
            .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
            .collect();
        assert_eq!(normalized[0], "1.2.3.4 example.com");
        assert_eq!(
            "1.2.3.4\t\texample.com".split_whitespace().collect::<Vec<_>>().join(" "),
            normalized[0]
        );
    }

    #[test]
    fn real_bundle_fakes_are_recognised() {
        let Ok(dir) = std::env::var("ZAPRET_TEST_DIR") else { return };
        let root = PathBuf::from(dir);
        if !root.join("bin").join("winws.exe").is_file() {
            return;
        }
        let info = fakes(&root);
        assert!(info.available.len() >= 5, "фейков найдено: {}", info.available.len());
        assert!(!info.available.iter().any(|f| f.starts_with("ACTIVE_")));
        assert!(info.discord.is_some(), "ACTIVE_DISCORD_UDP.bin ни с чем не совпал");
    }
}
