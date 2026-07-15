use arboard::Clipboard;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system};
use serde::Serialize;
use std::collections::HashMap;
#[cfg(not(target_os = "windows"))]
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

fn remove_shell_init(path: Option<PathBuf>) {
    #[cfg(target_os = "windows")]
    let _ = path;
    #[cfg(not(target_os = "windows"))]
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

fn resolve_shell(_session_id: &str, _generation: u64) -> Result<ShellLaunch, String> {
    let shell = selected_shell();

    #[cfg(target_os = "windows")]
    {
        Ok(ShellLaunch {
            args: windows_shell_args(&shell),
            shell,
            environment: Vec::new(),
            init_path: None,
        })
    }

    #[cfg(not(target_os = "windows"))]
    {
        unix_shell_launch(shell, _session_id, _generation)
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

#[cfg(target_os = "windows")]
fn windows_shell_args(shell: &str) -> Vec<String> {
    let executable = Path::new(shell)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if executable == "pwsh.exe" || executable == "powershell.exe" {
        return vec![
            "-NoLogo".to_string(),
            "-NoExit".to_string(),
            "-Command".to_string(),
            "$script:termdeckPrompt = (Get-Command prompt -CommandType Function).ScriptBlock; function global:prompt { $uri = [System.Uri]::new((Get-Location).Path).AbsoluteUri; [Console]::Write(\"$([char]27)]7;$uri$([char]7)\"); & $script:termdeckPrompt }".to_string(),
        ];
    }
    Vec::new()
}

#[cfg(not(target_os = "windows"))]
fn unix_shell_launch(shell: String, session_id: &str, generation: u64) -> Result<ShellLaunch, String> {
    let executable = Path::new(&shell)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let path = std::env::temp_dir().join(format!("termdeck-{session_id}-{generation}"));

    if executable == "bash" {
        let init_path = path.with_extension("bashrc");
        fs::write(
            &init_path,
            r#"if [ -r "$HOME/.bashrc" ]; then . "$HOME/.bashrc"; fi
__termdeck_emit_cwd() { printf '\033]7;file://%s%s\007' "${HOSTNAME:-localhost}" "$(pwd -P)"; }
PROMPT_COMMAND="__termdeck_emit_cwd${PROMPT_COMMAND:+;$PROMPT_COMMAND}"
"#,
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
        fs::write(
            &init_path,
            r#"if [[ -n "$TERMDECK_ORIGINAL_ZDOTDIR" && -r "$TERMDECK_ORIGINAL_ZDOTDIR/.zshrc" ]]; then
  source "$TERMDECK_ORIGINAL_ZDOTDIR/.zshrc"
elif [[ -r "$HOME/.zshrc" ]]; then
  source "$HOME/.zshrc"
fi
function __termdeck_emit_cwd() { print -n -- "\e]7;file://${HOSTNAME:-localhost}${PWD}\a"; }
precmd_functions+=(__termdeck_emit_cwd)
"#,
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
    let launch = resolve_shell(&session_id, generation)?;
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
    fn powershell_shell_args_include_pwd_bootstrap() {
        let args = windows_shell_args("C:\\Program Files\\PowerShell\\7\\pwsh.exe");
        assert_eq!(args[0], "-NoLogo");
        assert_eq!(args[1], "-NoExit");
        assert_eq!(args[2], "-Command");
        assert!(args[3].contains("function global:prompt"));
        assert!(args[3].contains("]7;"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn non_powershell_shell_uses_no_bootstrap_args() {
        let args = windows_shell_args("C:\\tools\\bash.exe");
        assert!(args.is_empty());
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
