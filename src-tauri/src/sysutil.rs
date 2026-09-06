use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::{Command, Output, Stdio};

pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Запускает консольную утилиту без всплывающего чёрного окна.
pub fn run_hidden(program: &str, args: &[&str]) -> std::io::Result<Output> {
    Command::new(program)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .output()
}

/// Вывод консольных утилит Windows приходит в OEM-кодировке; для наших
/// проверок достаточно ASCII-совместимого прочтения.
pub fn out_text(out: &Output) -> String {
    let mut s = String::from_utf8_lossy(&out.stdout).to_string();
    s.push_str(&String::from_utf8_lossy(&out.stderr));
    s
}

/// Сортировка «как в проводнике»: ALT2 идёт перед ALT10.
pub fn natural_key(name: &str) -> String {
    let mut out = String::new();
    let mut digits = String::new();
    for ch in name.chars() {
        if ch.is_ascii_digit() {
            digits.push(ch);
        } else {
            if !digits.is_empty() {
                out.push_str(&format!("{:0>8}", digits));
                digits.clear();
            }
            out.push(ch.to_ascii_lowercase());
        }
    }
    if !digits.is_empty() {
        out.push_str(&format!("{:0>8}", digits));
    }
    out
}

/// Папка похожа на распакованный zapret?
pub fn looks_like_zapret(dir: &Path) -> bool {
    dir.join("bin").join("winws.exe").is_file()
}

/// Внутри выбранной папки zapret может лежать на уровень глубже
/// (типичный случай: распаковали архив «как есть»).
pub fn resolve_zapret_root(dir: &Path) -> Option<std::path::PathBuf> {
    if looks_like_zapret(dir) {
        return Some(dir.to_path_buf());
    }
    let entries = std::fs::read_dir(dir).ok()?;
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() && looks_like_zapret(&p) {
            return Some(p);
        }
    }
    None
}

pub fn now_iso() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

pub fn hhmmss() -> String {
    chrono::Local::now().format("%H:%M:%S").to_string()
}
