use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system};
use serde::Serialize;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
#[cfg(target_os = "windows")]
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use tauri::{AppHandle, Emitter, State};

const MAX_TERMINAL_ID_LEN: usize = 100;
const MAX_INPUT_BYTES: usize = 64 * 1024;

#[derive(Clone, Default)]
struct TerminalManager {
    sessions: Arc<Mutex<HashMap<String, Arc<PtySession>>>>,
    next_generation: Arc<AtomicU64>,
}

struct PtySession {
    writer: Mutex<Box<dyn Write + Send>>,
    master: Mutex<Box<dyn MasterPty + Send>>,
    killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
    shell: String,
    cwd: String,
    pid: Option<u32>,
    generation: u64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PtyEvent {
    session_id: String,
    generation: u64,
    kind: &'static str,
    data: Option<String>,
    exit_code: Option<u32>,
    signal: Option<String>,
    message: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TerminalInfo {
    session_id: String,
    generation: u64,
    shell: String,
    cwd: String,
    pid: Option<u32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EnvironmentInfo {
    home: String,
    platform: &'static str,
    shell: String,
    smoke_test: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DockPathInfo {
    directory: String,
    suggested_name: String,
}

fn lock_or_recover<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poison| poison.into_inner())
}

fn validate_session_id(session_id: &str) -> Result<(), String> {
    if session_id.is_empty()
        || session_id.len() > MAX_TERMINAL_ID_LEN
        || !session_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
    {
        return Err("Invalid terminal session ID".to_string());
    }
    Ok(())
}

fn home_directory() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

fn existing_directory(requested: &str) -> PathBuf {
    let requested_path = Path::new(requested);
    if !requested.is_empty() && requested_path.is_dir() {
        requested_path
            .canonicalize()
            .unwrap_or_else(|_| requested_path.to_path_buf())
    } else {
        home_directory()
    }
}

fn resolve_shell() -> (String, Vec<String>) {
    if let Ok(shell) = std::env::var("TERMDECK_SHELL") {
        if !shell.trim().is_empty() {
            #[cfg(target_os = "windows")]
            if !Path::new(&shell).is_absolute() {
                if let Some(resolved) = find_windows_executable(&shell) {
                    return (resolved, Vec::new());
                }
            }
            return (shell, Vec::new());
        }
    }

    #[cfg(target_os = "windows")]
    {
        let shell = find_windows_executable("pwsh.exe").unwrap_or_else(|| {
            let system_root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
            Path::new(&system_root)
                .join("System32")
                .join("WindowsPowerShell")
                .join("v1.0")
                .join("powershell.exe")
                .to_string_lossy()
                .into_owned()
        });
        (shell, vec!["-NoLogo".to_string()])
    }

    #[cfg(not(target_os = "windows"))]
    {
        (
            std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string()),
            Vec::new(),
        )
    }
}

#[cfg(target_os = "windows")]
fn find_windows_executable(name: &str) -> Option<String> {
    let output = Command::new("where.exe").arg(name).output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && Path::new(line).is_file())
        .map(ToOwned::to_owned)
}

impl TerminalManager {
    fn kill(&self, session_id: &str) -> Result<(), String> {
        let session = lock_or_recover(&self.sessions).remove(session_id);
        if let Some(session) = session {
            lock_or_recover(&session.killer)
                .kill()
                .map_err(|error| format!("Unable to stop terminal: {error}"))?;
        }
        Ok(())
    }

    fn kill_all(&self) {
        let sessions = {
            let mut sessions = lock_or_recover(&self.sessions);
            sessions
                .drain()
                .map(|(_, session)| session)
                .collect::<Vec<_>>()
        };
        for session in sessions {
            let _ = lock_or_recover(&session.killer).kill();
        }
    }
}

#[tauri::command]
fn spawn_terminal(
    session_id: String,
    cwd: String,
    rows: u16,
    cols: u16,
    app: AppHandle,
    state: State<'_, TerminalManager>,
) -> Result<TerminalInfo, String> {
    validate_session_id(&session_id)?;

    if let Some(session) = lock_or_recover(&state.sessions).get(&session_id).cloned() {
        return Ok(TerminalInfo {
            session_id,
            generation: session.generation,
            shell: session.shell.clone(),
            cwd: session.cwd.clone(),
            pid: session.pid,
        });
    }

    let working_directory = existing_directory(&cwd);
    let working_directory_string = working_directory.to_string_lossy().into_owned();
    let (shell, args) = resolve_shell();
    let generation = state.next_generation.fetch_add(1, Ordering::Relaxed) + 1;

    let pair = native_pty_system()
        .openpty(PtySize {
            rows: rows.clamp(2, 500),
            cols: cols.clamp(2, 500),
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|error| format!("Unable to create PTY: {error}"))?;

    let mut command = CommandBuilder::new(&shell);
    command.args(args);
    command.cwd(&working_directory);
    command.env("TERM", "xterm-256color");
    command.env("COLORTERM", "truecolor");
    command.env("TERM_PROGRAM", "TermDeck");

    let mut child = pair
        .slave
        .spawn_command(command)
        .map_err(|error| format!("Unable to start {shell}: {error}"))?;
    let pid = child.process_id();
    let killer = child.clone_killer();
    drop(pair.slave);

    let mut reader = pair
        .master
        .try_clone_reader()
        .map_err(|error| format!("Unable to read PTY: {error}"))?;
    let writer = pair
        .master
        .take_writer()
        .map_err(|error| format!("Unable to write PTY: {error}"))?;

    let session = Arc::new(PtySession {
        writer: Mutex::new(writer),
        master: Mutex::new(pair.master),
        killer: Mutex::new(killer),
        shell: shell.clone(),
        cwd: working_directory_string.clone(),
        pid,
        generation,
    });
    lock_or_recover(&state.sessions).insert(session_id.clone(), session.clone());

    let output_app = app.clone();
    let output_session_id = session_id.clone();
    std::thread::spawn(move || {
        let mut buffer = [0_u8; 8192];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => {
                    let _ = output_app.emit(
                        "pty-event",
                        PtyEvent {
                            session_id: output_session_id.clone(),
                            generation,
                            kind: "output",
                            data: Some(BASE64.encode(&buffer[..read])),
                            exit_code: None,
                            signal: None,
                            message: None,
                        },
                    );
                }
                Err(error) => {
                    let _ = output_app.emit(
                        "pty-event",
                        PtyEvent {
                            session_id: output_session_id.clone(),
                            generation,
                            kind: "error",
                            data: None,
                            exit_code: None,
                            signal: None,
                            message: Some(error.to_string()),
                        },
                    );
                    break;
                }
            }
        }
    });

    let wait_sessions = state.sessions.clone();
    let wait_session = session.clone();
    let wait_session_id = session_id.clone();
    std::thread::spawn(move || {
        let result = child.wait();
        let (exit_code, signal, message) = match result {
            Ok(status) => (
                Some(status.exit_code()),
                status.signal().map(ToOwned::to_owned),
                None,
            ),
            Err(error) => (None, None, Some(error.to_string())),
        };

        let should_remove = {
            let sessions = lock_or_recover(&wait_sessions);
            sessions
                .get(&wait_session_id)
                .map(|current| Arc::ptr_eq(current, &wait_session))
                .unwrap_or(false)
        };
        if should_remove {
            lock_or_recover(&wait_sessions).remove(&wait_session_id);
        }

        let _ = app.emit(
            "pty-event",
            PtyEvent {
                session_id: wait_session_id,
                generation,
                kind: "exit",
                data: None,
                exit_code,
                signal,
                message,
            },
        );
    });

    Ok(TerminalInfo {
        session_id,
        generation,
        shell,
        cwd: working_directory_string,
        pid,
    })
}

#[tauri::command]
fn write_terminal(
    session_id: String,
    data: String,
    state: State<'_, TerminalManager>,
) -> Result<(), String> {
    validate_session_id(&session_id)?;
    if data.len() > MAX_INPUT_BYTES {
        return Err("Terminal input was too large".to_string());
    }
    let session = lock_or_recover(&state.sessions)
        .get(&session_id)
        .cloned()
        .ok_or_else(|| "Terminal session is not running".to_string())?;
    lock_or_recover(&session.writer)
        .write_all(data.as_bytes())
        .map_err(|error| format!("Unable to write terminal input: {error}"))
}

#[tauri::command]
fn resize_terminal(
    session_id: String,
    rows: u16,
    cols: u16,
    state: State<'_, TerminalManager>,
) -> Result<(), String> {
    validate_session_id(&session_id)?;
    let session = lock_or_recover(&state.sessions)
        .get(&session_id)
        .cloned()
        .ok_or_else(|| "Terminal session is not running".to_string())?;
    lock_or_recover(&session.master)
        .resize(PtySize {
            rows: rows.clamp(2, 500),
            cols: cols.clamp(2, 500),
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|error| format!("Unable to resize terminal: {error}"))
}

#[tauri::command]
fn kill_terminal(session_id: String, state: State<'_, TerminalManager>) -> Result<(), String> {
    validate_session_id(&session_id)?;
    state.kill(&session_id)
}

#[tauri::command]
fn get_environment() -> EnvironmentInfo {
    let (shell, _) = resolve_shell();
    EnvironmentInfo {
        home: home_directory().to_string_lossy().into_owned(),
        platform: std::env::consts::OS,
        shell,
        smoke_test: std::env::var("TERMDECK_SMOKE_TEST").as_deref() == Ok("1"),
    }
}

#[tauri::command]
fn running_terminal_count(state: State<'_, TerminalManager>) -> usize {
    lock_or_recover(&state.sessions).len()
}

#[tauri::command]
fn complete_smoke_test(success: bool, message: String, app: AppHandle) {
    if success {
        println!("TERMDECK_SMOKE_OK {message}");
        app.exit(0);
    } else {
        eprintln!("TERMDECK_SMOKE_FAILED {message}");
        app.exit(1);
    }
}

#[tauri::command]
fn normalize_dock_path(path: String) -> Result<DockPathInfo, String> {
    let trimmed = path.trim().trim_matches(['"', '\'']);
    if trimmed.is_empty() {
        return Err("No path was provided".to_string());
    }
    let source = PathBuf::from(trimmed);
    let canonical = source
        .canonicalize()
        .map_err(|error| format!("Unable to open dropped path: {error}"))?;
    let directory = if canonical.is_dir() {
        canonical
    } else {
        canonical
            .parent()
            .ok_or_else(|| "Dropped file has no parent directory".to_string())?
            .to_path_buf()
    };
    let suggested_name = directory
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("Docked terminal")
        .to_string();
    Ok(DockPathInfo {
        directory: directory.to_string_lossy().into_owned(),
        suggested_name,
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let manager = TerminalManager::default();
    let shutdown_manager = manager.clone();
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(manager)
        .invoke_handler(tauri::generate_handler![
            spawn_terminal,
            write_terminal,
            resize_terminal,
            kill_terminal,
            get_environment,
            running_terminal_count,
            complete_smoke_test,
            normalize_dock_path,
        ])
        .build(tauri::generate_context!())
        .expect("error while building TermDeck");

    app.run(move |_app_handle, event| {
        if matches!(event, tauri::RunEvent::Exit) {
            shutdown_manager.kill_all();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_safe_session_ids() {
        assert!(validate_session_id("term_abc-123").is_ok());
    }

    #[test]
    fn rejects_unsafe_session_ids() {
        assert!(validate_session_id("../terminal").is_err());
        assert!(validate_session_id("").is_err());
    }

    #[test]
    fn invalid_working_directory_falls_back_to_home() {
        assert_eq!(
            existing_directory("this-path-does-not-exist"),
            home_directory()
        );
    }

    #[test]
    fn dock_path_normalization_uses_a_directory() {
        let current = std::env::current_dir().expect("current directory");
        let canonical = current.canonicalize().expect("canonical directory");
        let info = normalize_dock_path(current.to_string_lossy().into_owned())
            .expect("normalize current directory");
        assert_eq!(PathBuf::from(info.directory), canonical);
        assert!(!info.suggested_name.is_empty());
    }
}
