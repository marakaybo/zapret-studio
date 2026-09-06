use crate::sysutil::{hhmmss, run_hidden, CREATE_NO_WINDOW};
use serde::Serialize;
use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};

const MAX_LOG_LINES: usize = 600;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LogLine {
    pub time: String,
    pub text: String,
    pub kind: String,
}

#[derive(Default)]
pub struct Runner {
    child: Mutex<Option<Child>>,
    pub current: Mutex<Option<String>>,
    logs: Arc<Mutex<VecDeque<LogLine>>>,
}

impl Runner {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn log(&self, app: Option<&AppHandle>, kind: &str, text: impl Into<String>) {
        let line = LogLine { time: hhmmss(), text: text.into(), kind: kind.to_string() };
        {
            let mut guard = self.logs.lock().unwrap();
            if guard.len() >= MAX_LOG_LINES {
                guard.pop_front();
            }
            guard.push_back(line.clone());
        }
        if let Some(app) = app {
            let _ = app.emit("log", line);
        }
    }

    pub fn logs(&self) -> Vec<LogLine> {
        self.logs.lock().unwrap().iter().cloned().collect()
    }

    pub fn clear_logs(&self) {
        self.logs.lock().unwrap().clear();
    }

    /// Уводит stdout и stderr процесса в общий журнал. Общая на весь
    /// проект: так в журнал попадает и winws.exe, и ciadpi.exe из ByeDPI.
    pub fn attach_output(&self, app: &AppHandle, child: &mut Child) {
        for (stream, kind) in [
            (child.stdout.take().map(|s| Box::new(s) as Box<dyn std::io::Read + Send>), "out"),
            (child.stderr.take().map(|s| Box::new(s) as Box<dyn std::io::Read + Send>), "err"),
        ] {
            let Some(stream) = stream else { continue };
            let logs = self.logs.clone();
            let app = app.clone();
            let kind = kind.to_string();
            std::thread::spawn(move || {
                let reader = BufReader::new(stream);
                for line in reader.split(b'\n') {
                    let Ok(bytes) = line else { break };
                    let text = String::from_utf8_lossy(&bytes).trim_end().to_string();
                    if text.is_empty() {
                        continue;
                    }
                    let entry = LogLine { time: hhmmss(), text, kind: kind.clone() };
                    {
                        let mut guard = logs.lock().unwrap();
                        if guard.len() >= MAX_LOG_LINES {
                            guard.pop_front();
                        }
                        guard.push_back(entry.clone());
                    }
                    let _ = app.emit("log", entry);
                }
            });
        }
    }

    /// zapret рекомендует включённые TCP timestamps — без них часть стратегий не работает.
    pub fn enable_tcp_timestamps(&self) {
        let _ = run_hidden("netsh", &["interface", "tcp", "set", "global", "timestamps=enabled"]);
    }

    pub fn winws_alive() -> bool {
        run_hidden("tasklist", &["/FI", "IMAGENAME eq winws.exe", "/NH"])
            .map(|o| crate::sysutil::out_text(&o).to_lowercase().contains("winws.exe"))
            .unwrap_or(false)
    }

    pub fn is_running(&self) -> bool {
        let mut guard = self.child.lock().unwrap();
        if let Some(child) = guard.as_mut() {
            match child.try_wait() {
                Ok(None) => return true,
                _ => {
                    *guard = None;
                }
            }
        }
        drop(guard);
        Self::winws_alive()
    }

    pub fn start(
        &self,
        app: &AppHandle,
        root: &Path,
        name: &str,
        args: &[String],
        quiet: bool,
    ) -> Result<(), String> {
        self.stop_silent();
        let exe = root.join("bin").join("winws.exe");
        if !exe.is_file() {
            return Err(format!("не найден {}", exe.display()));
        }
        self.enable_tcp_timestamps();
        crate::strategies::ensure_user_lists(root);

        let mut child = Command::new(&exe)
            .args(args)
            .current_dir(root.join("bin"))
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("не удалось запустить winws.exe: {e}"))?;

        self.attach_output(app, &mut child);

        *self.child.lock().unwrap() = Some(child);
        *self.current.lock().unwrap() = Some(name.to_string());
        if !quiet {
            self.log(Some(app), "info", format!("Запущена стратегия «{name}»"));
        }

        // Даём процессу шанс упасть на неверных аргументах и сообщаем об этом сразу.
        std::thread::sleep(std::time::Duration::from_millis(450));
        if !self.is_running() {
            *self.current.lock().unwrap() = None;
            return Err("winws.exe завершился сразу после запуска — смотри журнал".into());
        }
        Ok(())
    }

    fn stop_silent(&self) {
        if let Some(mut child) = self.child.lock().unwrap().take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = run_hidden("taskkill", &["/F", "/IM", "winws.exe"]);
        *self.current.lock().unwrap() = None;
    }

    pub fn stop(&self, app: Option<&AppHandle>) {
        let was = self.current.lock().unwrap().clone();
        self.stop_silent();
        if let Some(name) = was {
            self.log(app, "info", format!("Стратегия «{name}» остановлена"));
        }
    }
}
