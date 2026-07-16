use arboard::Clipboard;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system};
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
#[cfg(target_os = "windows")]
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use tauri::{AppHandle, Emitter, State};

const MAX_TERMINAL_ID_LEN: usize = 100;
const MAX_INPUT_BYTES: usize = 64 * 1024;
const MAX_OSC52_ENCODED_BYTES: usize = 4 * 1024 * 1024;
const OSC52_PREFIX: &[u8] = b"\x1b]52;";

#[derive(Default)]
struct Osc52ClipboardParser {
    pending: Vec<u8>,
}

impl Osc52ClipboardParser {
    fn consume(&mut self, bytes: &[u8]) -> Vec<String> {
        self.pending.extend_from_slice(bytes);
        let mut clipboard_texts = Vec::new();

        loop {
            let Some(start) = self
                .pending
                .windows(OSC52_PREFIX.len())
                .position(|window| window == OSC52_PREFIX)
            else {
                let retain = (1..OSC52_PREFIX.len())
                    .rev()
                    .find(|&length| self.pending.ends_with(&OSC52_PREFIX[..length]))
                    .unwrap_or(0);
                if retain == 0 {
                    self.pending.clear();
                } else {
                    self.pending = self.pending.split_off(self.pending.len() - retain);
                }
                break;
            };

            if start > 0 {
                self.pending.drain(..start);
            }

            let terminator = (OSC52_PREFIX.len()..self.pending.len()).find_map(|index| {
                if self.pending[index] == 0x07 {
                    Some((index, 1))
                } else if self.pending[index] == 0x1b
                    && self.pending.get(index + 1) == Some(&b'\\')
                {
                    Some((index, 2))
                } else {
                    None
                }
            });
            let Some((end, terminator_len)) = terminator else {
                if self.pending.len() > OSC52_PREFIX.len() + MAX_OSC52_ENCODED_BYTES {
                    self.pending.clear();
                }
                break;
            };

            let payload = &self.pending[OSC52_PREFIX.len()..end];
            if payload.len() <= MAX_OSC52_ENCODED_BYTES
                && let Some(separator) = payload.iter().position(|&byte| byte == b';')
            {
                let encoded = &payload[separator + 1..];
                let encoded = encoded
                    .iter()
                    .copied()
                    .filter(|byte| !byte.is_ascii_whitespace())
                    .collect::<Vec<_>>();
                if !encoded.is_empty() && encoded != b"?" {
                    if let Ok(decoded) = BASE64.decode(encoded) {
                        if let Ok(text) = String::from_utf8(decoded) {
                            clipboard_texts.push(text);
                        }
                    }
                }
            }
            self.pending.drain(..end + terminator_len);
        }

        clipboard_texts
    }
}

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

struct ShellLaunch {
    shell: String,
    args: Vec<String>,
    environment: Vec<(String, String)>,
    init_path: Option<PathBuf>,
}

/// Removes whatever a shell launch wrote to disk: an rcfile or ZDOTDIR on unix,
/// the PSReadLine history file on Windows.
fn remove_shell_init(path: Option<PathBuf>) {
    if let Some(path) = path {
        let _ = if path.is_dir() {
            fs::remove_dir_all(path)
        } else {
            fs::remove_file(path)
        };
    }
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
        let directory = requested_path
            .canonicalize()
            .unwrap_or_else(|_| requested_path.to_path_buf());
        #[cfg(target_os = "windows")]
        return without_windows_verbatim_prefix(directory);
        #[cfg(not(target_os = "windows"))]
        return directory;
    } else {
        home_directory()
    }
}

#[cfg(target_os = "windows")]
fn without_windows_verbatim_prefix(path: PathBuf) -> PathBuf {
    let value = path.to_string_lossy();
    if let Some(unc) = value.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{unc}"));
    }
    PathBuf::from(value.strip_prefix(r"\\?\").unwrap_or(&value))
}

fn selected_shell() -> String {
    if let Ok(shell) = std::env::var("TERMDECK_SHELL") {
        if !shell.trim().is_empty() {
            #[cfg(target_os = "windows")]
            {
                return if !Path::new(&shell).is_absolute() {
                    find_windows_executable(&shell).unwrap_or(shell)
                } else {
                    shell
                };
            }
            #[cfg(not(target_os = "windows"))]
            return shell;
        }
    }

    #[cfg(target_os = "windows")]
    {
        find_windows_executable("pwsh.exe").unwrap_or_else(|| {
            let system_root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
            Path::new(&system_root)
                .join("System32")
                .join("WindowsPowerShell")
                .join("v1.0")
                .join("powershell.exe")
                .to_string_lossy()
                .into_owned()
        })
    }

    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string())
    }
}

fn resolve_shell(session_id: &str, generation: u64, history: Vec<String>) -> Result<ShellLaunch, String> {
    let shell = selected_shell();

    #[cfg(target_os = "windows")]
    {
        windows_shell_launch(shell, session_id, generation, history)
    }

    #[cfg(not(target_os = "windows"))]
    {
        unix_shell_launch(shell, session_id, generation, history)
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

/// PowerShell startup script.
///
/// Restore points PSReadLine at a history file this process writes, rather than
/// calling AddToHistory: PSReadLine loads its history from that file when it
/// first reads a line, which happens after this script and after the first
/// prompt, so anything added before then is discarded. Loading from the file is
/// also what makes native history search and inline suggestions work, and keeps
/// the entries out of the user's global PowerShell history.
///
/// Capture reports each command PowerShell records. PowerShell adds this
/// `-Command` script to its session history once the script finishes, so it is
/// invisible here and first appears at the first prompt; the first prompt
/// therefore adopts whatever entry is present without reporting it, otherwise
/// this whole script would be reported back as the user's first command.
#[cfg(target_os = "windows")]
const POWERSHELL_INIT: &str = "Import-Module PSReadLine -ErrorAction SilentlyContinue; if (Get-Module PSReadLine) { Set-PSReadLineOption -HistorySavePath '__TERMDECK_HISTORY_PATH__' }; $script:termdeckHistoryId = -1; $script:termdeckPrompt = (Get-Command prompt -CommandType Function).ScriptBlock; function global:prompt { $entry = Get-History -Count 1; if ($script:termdeckHistoryId -lt 0) { $script:termdeckHistoryId = $(if ($entry) { $entry.Id } else { 0 }) } elseif ($entry -and $entry.Id -ne $script:termdeckHistoryId) { $script:termdeckHistoryId = $entry.Id; [Console]::Write(\"$([char]27)]6973;$([Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($entry.CommandLine)))$([char]7)\") }; $uri = [System.Uri]::new((Get-Location).Path).AbsoluteUri; [Console]::Write(\"$([char]27)]7;$uri$([char]7)\"); & $script:termdeckPrompt }";

#[cfg(target_os = "windows")]
fn windows_shell_launch(
    shell: String,
    session_id: &str,
    generation: u64,
    history: Vec<String>,
) -> Result<ShellLaunch, String> {
    let executable = Path::new(&shell)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if executable != "pwsh.exe" && executable != "powershell.exe" {
        return Ok(ShellLaunch {
            shell,
            args: Vec::new(),
            environment: Vec::new(),
            init_path: None,
        });
    }

    let history_path =
        std::env::temp_dir().join(format!("termdeck-{session_id}-{generation}.history.txt"));
    fs::write(&history_path, powershell_history_file(&history))
        .map_err(|error| format!("Unable to prepare PowerShell history: {error}"))?;
    let command = POWERSHELL_INIT.replace(
        "__TERMDECK_HISTORY_PATH__",
        &history_path.to_string_lossy().replace('\'', "''"),
    );

    Ok(ShellLaunch {
        shell,
        args: vec![
            "-NoLogo".to_string(),
            "-NoExit".to_string(),
            "-Command".to_string(),
            command,
        ],
        environment: Vec::new(),
        init_path: Some(history_path),
    })
}

/// PSReadLine's history file is one command per line, so entries that span lines
/// cannot be represented and are dropped rather than split into fragments.
#[cfg(target_os = "windows")]
fn powershell_history_file(history: &[String]) -> String {
    history
        .iter()
        .filter(|entry| {
            !entry.trim().is_empty()
                && !entry.contains('\n')
                && !entry.contains('\r')
                && !entry.contains('\0')
        })
        .map(|entry| format!("{entry}\n"))
        .collect()
}

#[cfg(not(target_os = "windows"))]
fn unix_shell_launch(shell: String, session_id: &str, generation: u64, history: Vec<String>) -> Result<ShellLaunch, String> {
    let executable = Path::new(&shell)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let path = std::env::temp_dir().join(format!("termdeck-{session_id}-{generation}"));

    if executable == "bash" {
        let init_path = path.with_extension("bashrc");
        let init = format!(
            "if [ -r \"$HOME/.bashrc\" ]; then . \"$HOME/.bashrc\"; fi\n{}",
            unix_history_setup(&history, "history -s "),
        ) + r#"__termdeck_emit_cwd() { printf '\033]7;file://%s%s\007' "${HOSTNAME:-localhost}" "$(pwd -P)"; }
__termdeck_last_history=
__termdeck_read_history() {
  local line
  line=$(HISTTIMEFORMAT= builtin history 1) || return 1
  [[ $line =~ ^[[:space:]]*([0-9]+)[[:space:]]+(.*)$ ]] || return 1
  __termdeck_history_id=${BASH_REMATCH[1]}
  __termdeck_history_command=${BASH_REMATCH[2]}
}
__termdeck_emit_history() {
  __termdeck_read_history || return
  [ "$__termdeck_history_id" = "$__termdeck_last_history" ] && return
  __termdeck_last_history=$__termdeck_history_id
  printf '\033]6973;%s\007' "$(printf '%s' "$__termdeck_history_command" | base64 | tr -d '\n')"
}
# Seeded entries are already known to the app; start from the current entry so
# the first prompt does not report them back as newly run commands.
__termdeck_read_history && __termdeck_last_history=$__termdeck_history_id
PROMPT_COMMAND="__termdeck_emit_cwd;__termdeck_emit_history${PROMPT_COMMAND:+;$PROMPT_COMMAND}"
"#;
        fs::write(
            &init_path,
            init,
        )
        .map_err(|error| format!("Unable to prepare Bash integration: {error}"))?;
        return Ok(ShellLaunch {
            shell,
            args: vec!["--rcfile".to_string(), init_path.to_string_lossy().into_owned(), "-i".to_string()],
            environment: Vec::new(),
            init_path: Some(init_path),
        });
    }

    if executable == "zsh" {
        fs::create_dir_all(&path).map_err(|error| format!("Unable to prepare Zsh integration: {error}"))?;
        let init_path = path.join(".zshrc");
        let init = format!(
            "if [[ -n \"$TERMDECK_ORIGINAL_ZDOTDIR\" && -r \"$TERMDECK_ORIGINAL_ZDOTDIR/.zshrc\" ]]; then\n  source \"$TERMDECK_ORIGINAL_ZDOTDIR/.zshrc\"\nelif [[ -r \"$HOME/.zshrc\" ]]; then\n  source \"$HOME/.zshrc\"\nfi\n{}",
            unix_history_setup(&history, "print -s -- "),
        ) + r#"function __termdeck_emit_cwd() { print -n -- "\e]7;file://${HOSTNAME:-localhost}${PWD}\a"; }
precmd_functions+=(__termdeck_emit_cwd)
function __termdeck_emit_history() { print -n -- "\e]6973;$(printf '%s' "$1" | base64 | tr -d '\n')\a"; }
preexec_functions+=(__termdeck_emit_history)
"#;
        fs::write(
            &init_path,
            init,
        )
        .map_err(|error| format!("Unable to prepare Zsh integration: {error}"))?;
        return Ok(ShellLaunch {
            shell,
            args: vec!["-i".to_string()],
            environment: vec![
                ("ZDOTDIR".to_string(), path.to_string_lossy().into_owned()),
                (
                    "TERMDECK_ORIGINAL_ZDOTDIR".to_string(),
                    std::env::var("ZDOTDIR").unwrap_or_default(),
                ),
            ],
            init_path: Some(path),
        });
    }

    #[cfg(not(target_os = "windows"))]
    fn unix_history_setup(history: &[String], command: &str) -> String {
        history
            .iter()
            .filter(|entry| !entry.is_empty() && !entry.contains('\0'))
            .map(|entry| format!("{command}'{}'\n", entry.replace('\'', "'\\''")))
            .collect()
    }

    Ok(ShellLaunch {
        shell,
        args: Vec::new(),
        environment: Vec::new(),
        init_path: None,
    })
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
    history: Vec<String>,
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
    let generation = state.next_generation.fetch_add(1, Ordering::Relaxed) + 1;
    let launch = resolve_shell(&session_id, generation, history)?;
    let shell = launch.shell.clone();

    let pair = native_pty_system()
        .openpty(PtySize {
            rows: rows.clamp(2, 500),
            cols: cols.clamp(2, 500),
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|error| format!("Unable to create PTY: {error}"))?;

    let mut command = CommandBuilder::new(&shell);
    command.args(launch.args);
    command.cwd(&working_directory);
    command.env("TERM", "xterm-256color");
    command.env("COLORTERM", "truecolor");
    command.env("TERM_PROGRAM", "TermDeck");
    for (key, value) in launch.environment {
        command.env(key, value);
    }

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
        let mut osc52_parser = Osc52ClipboardParser::default();
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => {
                    for text in osc52_parser.consume(&buffer[..read]) {
                        if let Err(error) = Clipboard::new()
                            .and_then(|mut clipboard| clipboard.set_text(text))
                        {
                            eprintln!("Unable to copy OSC52 terminal content: {error}");
                        }
                    }
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
    let init_path = launch.init_path;
    std::thread::spawn(move || {
        let result = child.wait();
        remove_shell_init(init_path);
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
    EnvironmentInfo {
        home: home_directory().to_string_lossy().into_owned(),
        platform: std::env::consts::OS,
        shell: selected_shell(),
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

    #[cfg(target_os = "windows")]
    #[test]
    fn powershell_history_file_holds_one_command_per_line() {
        let contents = powershell_history_file(&[
            "first".to_string(),
            "  ".to_string(),
            "second".to_string(),
        ]);
        assert_eq!(contents, "first\nsecond\n");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn powershell_history_file_drops_entries_it_cannot_represent() {
        let contents = powershell_history_file(&[
            "keep me".to_string(),
            "spans\nlines".to_string(),
            "has\0nul".to_string(),
        ]);
        assert_eq!(contents, "keep me\n");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn powershell_shell_args_include_pwd_bootstrap() {
        let launch = windows_shell_launch(
            "C:\\Program Files\\PowerShell\\7\\pwsh.exe".to_string(),
            "term_test",
            1,
            vec!["ls ~/Downloads".to_string()],
        )
        .expect("build PowerShell launch");
        assert_eq!(launch.args[0], "-NoLogo");
        assert_eq!(launch.args[1], "-NoExit");
        assert_eq!(launch.args[2], "-Command");
        assert!(launch.args[3].contains("function global:prompt"));
        assert!(launch.args[3].contains("]7;"));
        // Restore must go through PSReadLine's own history file: rebinding the
        // arrow keys shadows the native bindings that accept a suggestion.
        assert!(launch.args[3].contains("Set-PSReadLineOption -HistorySavePath"));
        assert!(!launch.args[3].contains("Set-PSReadLineKeyHandler"));

        let history_path = launch.init_path.expect("history file written");
        assert_eq!(
            fs::read_to_string(&history_path).expect("read history file"),
            "ls ~/Downloads\n"
        );
        let _ = fs::remove_file(history_path);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn powershell_history_handler_restores_the_full_command() {
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: 30,
                cols: 120,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("create PTY");
        let shell = selected_shell();
        let launch = resolve_shell("term_history_test", 1, vec!["ls ~/Downloads".to_string()])
            .expect("build PowerShell launch");
        let mut command = CommandBuilder::new(&shell);
        command.args(launch.args.clone());
        command.env("TERM", "xterm-256color");
        command.cwd(home_directory());

        let mut child = pair.slave.spawn_command(command).expect("start PowerShell");
        let mut reader = pair.master.try_clone_reader().expect("clone PTY reader");
        let mut writer = pair.master.take_writer().expect("take PTY writer");
        drop(pair.slave);

        let (output_sender, output_receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut buffer = [0_u8; 4096];
            while let Ok(read) = reader.read(&mut buffer) {
                if read == 0 {
                    break;
                }
                if output_sender
                    .send(String::from_utf8_lossy(&buffer[..read]).into_owned())
                    .is_err()
                {
                    break;
                }
            }
        });

        let mut output = String::new();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        let mut responded_to_cursor_query = false;
        while std::time::Instant::now() < deadline {
            let Ok(chunk) = output_receiver.recv_timeout(std::time::Duration::from_millis(100)) else {
                continue;
            };
            output.push_str(&chunk);
            if !responded_to_cursor_query && output.contains("\x1b[6n") {
                writer
                    .write_all(b"\x1b[1;1R")
                    .expect("respond to cursor position query");
                writer.flush().expect("flush cursor position response");
                responded_to_cursor_query = true;
            }
            if responded_to_cursor_query && output.contains('>') {
                break;
            }
        }
        writer.write_all(b"\x1b[A").expect("send up arrow");
        writer.flush().expect("flush up arrow");
        std::thread::sleep(std::time::Duration::from_millis(500));
        output.push_str(&output_receiver.try_iter().collect::<String>());
        child.kill().expect("stop PowerShell");
        let _ = child.wait();

        while let Ok(chunk) = output_receiver.recv_timeout(std::time::Duration::from_millis(100)) {
            output.push_str(&chunk);
        }
        remove_shell_init(launch.init_path);
        assert!(
            output.contains("ls ") && output.contains("~/Downloads"),
            "PowerShell did not restore the full command. Output: {output:?}"
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn non_powershell_shell_uses_no_bootstrap_args() {
        let launch = windows_shell_launch("C:\\tools\\bash.exe".to_string(), "term_test", 1, Vec::new())
            .expect("build launch");
        assert!(launch.args.is_empty());
        assert!(launch.init_path.is_none());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn removes_windows_verbatim_path_prefix() {
        assert_eq!(
            without_windows_verbatim_prefix(PathBuf::from(r"\\?\C:\Users\dennis")),
            PathBuf::from(r"C:\Users\dennis")
        );
        assert_eq!(
            without_windows_verbatim_prefix(PathBuf::from(r"\\?\UNC\server\share")),
            PathBuf::from(r"\\server\share")
        );
    }

    #[test]
    fn extracts_osc52_clipboard_text_across_reads() {
        let mut parser = Osc52ClipboardParser::default();
        assert_eq!(parser.consume(b"before\x1b]"), Vec::<String>::new());
        assert_eq!(
            parser.consume(b"52;c;Y29waWVk\x07after"),
            vec!["copied".to_string()]
        );
    }

    #[test]
    fn extracts_osc52_clipboard_text_with_st_terminator() {
        let mut parser = Osc52ClipboardParser::default();
        assert_eq!(
            parser.consume(b"\x1b]52;c;Y29waWVk\x1b\\"),
            vec!["copied".to_string()]
        );
    }

    #[test]
    fn discards_oversized_incomplete_osc52_sequence() {
        let mut parser = Osc52ClipboardParser::default();
        let mut sequence = OSC52_PREFIX.to_vec();
        sequence.resize(OSC52_PREFIX.len() + MAX_OSC52_ENCODED_BYTES + 1, b'a');
        assert_eq!(parser.consume(&sequence), Vec::<String>::new());
        assert_eq!(parser.pending, Vec::<u8>::new());
    }
}
