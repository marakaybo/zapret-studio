//! Общее для движков, которые запускаются набором ключей: ByeDPI и GoodbyeDPI.
//!
//! У обоих одна и та же механика — есть встроенные наборы параметров, есть
//! свои, добавленные пользователем, и есть один дочерний процесс, который надо
//! поднять, послушать и погасить. Разница только в самой программе и в том,
//! что подставляется в аргументы.
//!
//! Стратегии zapret живут отдельно (`strategies.rs`): они читаются из
//! `.bat`-файлов сборки, а не задаются в приложении.

use crate::runner::Runner;
use crate::sysutil::{out_text, run_hidden, CREATE_NO_WINDOW};
use serde::{Deserialize, Serialize};
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use tauri::AppHandle;

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    pub id: String,
    pub name: String,
    pub desc: String,
    pub args: Vec<String>,
    /// false — пресет добавлен пользователем, его можно менять и удалять
    pub builtin: bool,
}

/// Пресет, сохранённый пользователем: храним строкой, как он её ввёл.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct CustomPreset {
    pub id: String,
    pub name: String,
    pub args: String,
}

/// Описание встроенного пресета в исходниках: id, название, пояснение, ключи.
pub type Builtin = (&'static str, &'static str, &'static str, &'static str);

pub fn builtin(list: &[Builtin]) -> Vec<Preset> {
    list.iter()
        .map(|(id, name, desc, args)| Preset {
            id: (*id).into(),
            name: (*name).into(),
            desc: (*desc).into(),
            args: parse_args(args),
            builtin: true,
        })
        .collect()
}

pub fn all(list: &[Builtin], custom: &[CustomPreset]) -> Vec<Preset> {
    let mut out = builtin(list);
    for c in custom {
        out.push(Preset {
            id: c.id.clone(),
            name: c.name.clone(),
            desc: "Свой набор параметров".into(),
            args: parse_args(&c.args),
            builtin: false,
        });
    }
    out
}

pub fn find<'a>(all: &'a [Preset], id: &str) -> Option<&'a Preset> {
    all.iter().find(|p| p.id == id)
}

/// Разбор строки параметров с оглядкой на кавычки: --fake-data ":GET / HTTP"
/// должен остаться одним аргументом.
pub fn parse_args(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut started = false;
    for ch in line.chars() {
        match (quote, ch) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => cur.push(c),
            (None, c) if c == '"' || c == '\'' => {
                quote = Some(c);
                started = true;
            }
            (None, c) if c.is_whitespace() => {
                if started {
                    out.push(std::mem::take(&mut cur));
                    started = false;
                }
            }
            (None, c) => {
                cur.push(c);
                started = true;
            }
        }
    }
    if started {
        out.push(cur);
    }
    out
}

/// Подставляет в аргументы папку установки: пресеты ссылаются на списки
/// доменов через %ROOT%, как батники zapret — через %LISTS%.
pub fn expand_root(args: &[String], root: &Path) -> Vec<String> {
    let dir = format!("{}\\", root.display());
    args.iter().map(|a| a.replace("%ROOT%", &dir)).collect()
}

// ------------------------------------------------------------------ процесс

/// Дочерний процесс движка: один на приложение, с выводом в общий журнал.
pub struct Proc {
    /// Имя файла — по нему добиваем осиротевшие копии прошлого запуска
    exe_name: &'static str,
    child: Mutex<Option<Child>>,
    pub current: Mutex<Option<String>>,
}

impl Proc {
    pub fn new(exe_name: &'static str) -> Self {
        Self { exe_name, child: Mutex::new(None), current: Mutex::new(None) }
    }

    pub fn is_running(&self) -> bool {
        let mut guard = self.child.lock().unwrap();
        match guard.as_mut().map(|c| c.try_wait()) {
            Some(Ok(None)) => true,
            None => false,
            _ => {
                *guard = None;
                *self.current.lock().unwrap() = None;
                false
            }
        }
    }

    pub fn process_alive(&self) -> bool {
        run_hidden("tasklist", &["/FI", &format!("IMAGENAME eq {}", self.exe_name), "/NH"])
            .map(|o| out_text(&o).to_lowercase().contains(&self.exe_name.to_lowercase()))
            .unwrap_or(false)
    }

    /// `id` — что положить в `current`, `announce` — строка для журнала.
    pub fn start(
        &self,
        app: &AppHandle,
        runner: &Runner,
        exe: &Path,
        cwd: &Path,
        args: &[String],
        id: &str,
        announce: Option<String>,
    ) -> Result<(), String> {
        // Чужая или осиротевшая копия мешала бы — говорим об этом вслух,
        // чтобы «куда делся мой обход» не осталось загадкой
        if self.current.lock().unwrap().is_none() && self.process_alive() {
            runner.log(Some(app), "info", format!("Останавливаю уже работающий {}", self.exe_name));
        }
        self.stop_silent();

        if !exe.is_file() {
            return Err(format!("не найден {}", exe.display()));
        }
        let mut child = Command::new(exe)
            .args(args)
            .current_dir(cwd)
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("не удалось запустить {}: {e}", self.exe_name))?;

        runner.attach_output(app, &mut child);
        *self.child.lock().unwrap() = Some(child);
        *self.current.lock().unwrap() = Some(id.to_string());
        if let Some(text) = announce {
            runner.log(Some(app), "info", text);
        }

        // Неверные ключи роняют процесс сразу — лучше сказать об этом здесь,
        // чем оставить пользователя с выключенным обходом и зелёной кнопкой.
        std::thread::sleep(std::time::Duration::from_millis(450));
        if !self.is_running() {
            return Err(format!(
                "{} завершился сразу после запуска — смотри журнал",
                self.exe_name
            ));
        }
        Ok(())
    }

    fn stop_silent(&self) {
        if let Some(mut child) = self.child.lock().unwrap().take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if self.process_alive() {
            let _ = run_hidden("taskkill", &["/F", "/IM", self.exe_name]);
        }
        *self.current.lock().unwrap() = None;
    }

    pub fn stop(&self, runner: &Runner, app: Option<&AppHandle>, announce: &str) {
        let was = self.current.lock().unwrap().clone();
        self.stop_silent();
        if was.is_some() {
            runner.log(app, "info", announce);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn splits_args_and_keeps_quoted_values() {
        assert_eq!(parse_args("--split 1+s --disorder 3+s").len(), 4);
        let args = parse_args("--fake-data \":GET / HTTP\" --ttl 8");
        assert_eq!(args[1], ":GET / HTTP");
        assert_eq!(args.len(), 4);
        assert!(parse_args("   ").is_empty());
    }

    #[test]
    fn expands_root_placeholder_into_absolute_paths() {
        let args = parse_args("-9 --blacklist %ROOT%russia-blacklist.txt");
        let out = expand_root(&args, &PathBuf::from(r"C:\tools\gdpi"));
        assert_eq!(out[2], r"C:\tools\gdpi\russia-blacklist.txt");
        assert!(!out.iter().any(|a| a.contains('%')));
    }
}
