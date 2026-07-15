# TermDeck

TermDeck is a cross-platform terminal workspace for Windows and Linux, rebuilt with a Rust/Tauri backend and a Svelte frontend. Its visual system follows the Aevum OS language: restrained dark surfaces, metallic window edging, compact information density, indigo focus states, Segoe UI chrome, and Cascadia Code terminals.

## What it does

- Keeps independent, live terminal groups in named workspaces
- Opens every new terminal at that workspace's default project folder
- Automatically tiles terminal panes to fill the available stage
- Lets you drag pane dividers to resize side-by-side terminals independently within each row
- Moves a running terminal between workspaces by dragging its tab or pane header
- Moves the focused terminal with `Ctrl+Shift+Left/Right`
- Docks folders and files dropped from Explorer or a Linux file manager
- Persists workspace, terminal, and project-folder metadata
- Runs PowerShell on Windows and `$SHELL`/bash on Linux through `portable-pty`

Moving a terminal between TermDeck workspaces does not restart it. The Rust PTY remains alive while only its Svelte layout ownership changes.

## External terminal docking

Dropping a folder or file anywhere in TermDeck opens a new managed shell at that location and makes that folder the workspace default. The same flow is available under **Dock external**.

An already-running terminal process cannot be transferred between unrelated PTY hosts on Windows or Linux. The operating systems do not expose a portable mechanism for moving its process, scrollback, and PTY ownership into TermDeck. External docking therefore recreates a managed shell at the selected location; it does not claim to adopt the original process.

## Keyboard controls

| Action | Shortcut |
| --- | --- |
| New terminal | `Ctrl+Shift+T` |
| Next / previous terminal | `Ctrl+Tab` / `Ctrl+Shift+Tab` |
| Focus terminal 1–9 | `Ctrl+1` through `Ctrl+9` |
| Resize active pane split | `Alt+Shift+Left/Right` |
| Switch workspace 1–9 | `Alt+1` through `Alt+9` |
| Previous / next workspace | `Ctrl+Alt+Left/Right` |
| Move terminal to adjacent workspace | `Ctrl+Shift+Left/Right` |
| Rename focused terminal | `F2` |
| Close focused terminal | `Ctrl+Shift+W` |
| Shortcut reference | `Ctrl+/` |

## Development

Requirements:

- Node.js 22 or newer
- Rust stable
- Tauri 2 system prerequisites for the target OS
- Windows: WebView2 and Visual Studio C++ Build Tools
- Linux: the WebKitGTK development packages listed by Tauri

```powershell
npm install
npm run desktop
```

Frontend-only preview:

```powershell
npm run dev
```

The preview renders the full interface but does not spawn native PTYs.

## Verification

```powershell
npm test
npm run check
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
```

## Packaging

Windows NSIS installer:

```powershell
npm run package:win
```

Linux AppImage and Debian package:

```bash
npm run package:linux
```

Build each native package on its target operating system so the correct WebView and PTY runtime is linked.

## Shell override

Set `TERMDECK_SHELL` before starting TermDeck to use another shell:

```powershell
$env:TERMDECK_SHELL = "C:\\Program Files\\PowerShell\\7\\pwsh.exe"
npm run desktop
```

```bash
TERMDECK_SHELL=/usr/bin/zsh npm run desktop
```
