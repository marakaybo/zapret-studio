use crate::sysutil::natural_key;
use serde::Serialize;
use std::path::Path;

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Strategy {
    /// Имя файла с расширением, например "general (ALT2).bat"
    pub file: String,
    /// Имя без расширения — то, что видит пользователь
    pub name: String,
    /// Короткая метка: general / ALT2 / FAKE TLS AUTO
    pub label: String,
    pub args: Vec<String>,
    /// Использованные методы обхода — для подсказки в интерфейсе
    pub tags: Vec<String>,
}

pub fn game_ports(mode: &str) -> (&'static str, &'static str) {
    match mode {
        "all" => ("1024-65535", "1024-65535"),
        "tcp" => ("1024-65535", "12"),
        "udp" => ("12", "1024-65535"),
        _ => ("12", "12"),
    }
}

fn tokenize(s: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    for c in s.chars() {
        match c {
            '"' => in_quotes = !in_quotes,
            c if c.is_whitespace() && !in_quotes => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            _ => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out.retain(|t| t != "^" && t != "^^" && !t.is_empty());
    out
}

/// Достаёт из батника аргументы запуска winws.exe и подставляет реальные пути.
pub fn parse_bat(path: &Path, root: &Path, game_filter: &str) -> Result<Vec<String>, String> {
    let raw = std::fs::read(path).map_err(|e| format!("не читается {}: {e}", path.display()))?;
    let text = String::from_utf8_lossy(&raw).to_string();
    let lines: Vec<&str> = text.lines().collect();

    let start = lines
        .iter()
        .position(|l| l.to_lowercase().contains("winws.exe"))
        .ok_or_else(|| format!("в {} нет команды запуска winws.exe", path.display()))?;

    // Склеиваем строки, продолженные символом ^
    let mut buf = String::new();
    let mut idx = start;
    loop {
        let line = lines[idx].trim_end();
        let cont = line.ends_with('^');
        buf.push_str(if cont { &line[..line.len() - 1] } else { line });
        buf.push(' ');
        if !cont || idx + 1 >= lines.len() {
            break;
        }
        idx += 1;
    }

    let lower = buf.to_lowercase();
    let pos = lower.find("winws.exe").unwrap() + "winws.exe".len();
    let mut rest = buf[pos..].trim_start().to_string();
    if rest.starts_with('"') {
        rest.remove(0);
    }

    let (tcp, udp) = game_ports(game_filter);
    let bin = format!("{}\\", root.join("bin").display());
    let lists = format!("{}\\", root.join("lists").display());
    let dp0 = format!("{}\\", root.display());

    let substituted = rest
        .replace("%BIN%", &bin)
        .replace("%LISTS%", &lists)
        .replace("%~dp0", &dp0)
        .replace("%GameFilterTCP%", tcp)
        .replace("%GameFilterUDP%", udp)
        .replace("%GameFilter%", tcp);

    let args = tokenize(&substituted);
    if args.is_empty() {
        return Err(format!("не удалось разобрать аргументы в {}", path.display()));
    }
    Ok(args)
}

fn tags_from_args(args: &[String]) -> Vec<String> {
    let mut tags: Vec<String> = Vec::new();
    for a in args {
        if let Some(v) = a.strip_prefix("--dpi-desync=") {
            for part in v.split(',') {
                let p = part.trim().to_string();
                if !p.is_empty() && !tags.contains(&p) {
                    tags.push(p);
                }
            }
        }
    }
    tags
}

fn label_of(name: &str) -> String {
    // "general (ALT2)" -> "ALT2", "general" -> "базовая"
    match (name.find('('), name.rfind(')')) {
        (Some(a), Some(b)) if b > a + 1 => name[a + 1..b].to_string(),
        _ => {
            if name.eq_ignore_ascii_case("general") {
                "базовая".to_string()
            } else {
                name.to_string()
            }
        }
    }
}

/// Создаёт пользовательские списки, без которых winws.exe не стартует.
pub fn ensure_user_lists(root: &Path) {
    let lists = root.join("lists");
    if !lists.is_dir() {
        return;
    }
    let defaults: [(&str, &str); 3] = [
        ("list-general-user.txt", "# Never leave this file empty\ndomain.example.abc\n"),
        ("list-exclude-user.txt", "domain.example.abc\n"),
        ("ipset-exclude-user.txt", "203.0.113.113/32\n"),
    ];
    for (file, content) in defaults {
        let p = lists.join(file);
        if !p.exists() {
            let _ = std::fs::write(p, content);
        }
    }
}

pub fn list(root: &Path, game_filter: &str) -> Result<Vec<Strategy>, String> {
    ensure_user_lists(root);
    let mut items: Vec<Strategy> = Vec::new();
    let entries = std::fs::read_dir(root).map_err(|e| format!("папка недоступна: {e}"))?;
    for entry in entries.flatten() {
        let path = entry.path();
        let is_bat = path
            .extension()
            .map(|e| e.eq_ignore_ascii_case("bat"))
            .unwrap_or(false);
        if !is_bat {
            continue;
        }
        let file = entry.file_name().to_string_lossy().to_string();
        if file.to_lowercase().starts_with("service") {
            continue;
        }
        let name = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
        match parse_bat(&path, root, game_filter) {
            Ok(args) => {
                let tags = tags_from_args(&args);
                items.push(Strategy { file, label: label_of(&name), name, args, tags });
            }
            Err(_) => continue,
        }
    }
    items.sort_by_key(|s| natural_key(&s.name));
    Ok(items)
}

pub fn find<'a>(all: &'a [Strategy], name: &str) -> Option<&'a Strategy> {
    all.iter().find(|s| s.name == name || s.file == name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tmp_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("zs-test-{name}"));
        std::fs::create_dir_all(dir.join("bin")).unwrap();
        std::fs::create_dir_all(dir.join("lists")).unwrap();
        std::fs::write(dir.join("bin").join("winws.exe"), b"stub").unwrap();
        dir
    }

    #[test]
    fn parses_continuations_quotes_and_vars() {
        let root = tmp_root("parse");
        let bat = root.join("general.bat");
        std::fs::write(
            &bat,
            "@echo off\r\n\
             set \"BIN=%~dp0bin\\\"\r\n\
             cd /d %BIN%\r\n\
             start \"zapret: %~n0\" /min \"%BIN%winws.exe\" --wf-tcp=80,443,%GameFilterTCP% ^\r\n\
             --filter-tcp=443 --hostlist=\"%LISTS%list-general.txt\" --dpi-desync=fake,fakedsplit --new ^\r\n\
             --filter-udp=443 --dpi-desync-fake-quic=\"%BIN%quic_initial_www_google_com.bin\"\r\n",
        )
        .unwrap();

        let args = parse_bat(&bat, &root, "off").unwrap();

        // первый аргумент — параметр, а не остаток команды start
        assert!(args[0].starts_with("--wf-tcp="), "первый аргумент: {}", args[0]);
        // игровой фильтр выключен -> подставляется 12, как в service.bat
        assert!(args[0].ends_with(",12"), "{}", args[0]);
        // кавычки сняты, путь абсолютный
        let hostlist = args.iter().find(|a| a.starts_with("--hostlist=")).unwrap();
        assert!(hostlist.contains("lists"), "{hostlist}");
        assert!(!hostlist.contains('"'), "кавычки должны быть сняты: {hostlist}");
        assert!(!hostlist.contains('%'), "переменные должны быть раскрыты: {hostlist}");
        // символ переноса строки не должен попасть в аргументы
        assert!(!args.iter().any(|a| a == "^"), "остался символ переноса");
        // многострочность склеена целиком
        assert!(args.iter().any(|a| a.starts_with("--dpi-desync-fake-quic=")));
        assert!(args.iter().any(|a| a == "--new"));

        // включённый игровой фильтр меняет диапазон портов
        let args_game = parse_bat(&bat, &root, "all").unwrap();
        assert!(args_game[0].ends_with(",1024-65535"), "{}", args_game[0]);
    }

    #[test]
    fn skips_service_bat_and_sorts_naturally() {
        let root = tmp_root("list");
        for name in ["general.bat", "general (ALT2).bat", "general (ALT10).bat", "service.bat"] {
            std::fs::write(
                root.join(name),
                "start \"zapret\" /min \"%BIN%winws.exe\" --wf-tcp=80 --dpi-desync=multisplit\r\n",
            )
            .unwrap();
        }
        let list = list(&root, "off").unwrap();
        let names: Vec<&str> = list.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["general", "general (ALT2)", "general (ALT10)"]);
        assert_eq!(list[0].tags, vec!["multisplit"]);
        assert!(root.join("lists").join("list-general-user.txt").exists());
    }

    /// Прогон по настоящей сборке: ZAPRET_TEST_DIR=путь cargo test
    #[test]
    fn parses_real_bundle_if_available() {
        let Ok(dir) = std::env::var("ZAPRET_TEST_DIR") else { return };
        let root = PathBuf::from(dir);
        if !root.join("bin").join("winws.exe").is_file() {
            return;
        }
        let list = list(&root, "off").unwrap();
        assert!(list.len() >= 5, "стратегий найдено: {}", list.len());
        for s in &list {
            assert!(s.args[0].starts_with("--"), "{}: {}", s.name, s.args[0]);
            assert!(!s.args.iter().any(|a| a.contains('%')), "{}: осталась переменная", s.name);
            assert!(!s.args.iter().any(|a| a.contains('"')), "{}: остались кавычки", s.name);
            assert!(s.args.iter().any(|a| a.starts_with("--dpi-desync")), "{}", s.name);
        }
    }
}
