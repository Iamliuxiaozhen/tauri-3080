# tauri-3080 v0.1.0

**A tiny native window for `127.0.0.1:3080`.** Open the app and your local
service appears in its own window — no browser tab, no address bar, plus a
pre-launch hook that can start whatever needs to be listening.

## Highlights

- **Opens `http://127.0.0.1:3080`** in a native window, sized and remembered.
- **Pre-launch hooks** — a script in the config directory runs *before* the page
  loads, with streamed output, a hard timeout and process-tree termination.
  `run`, `wait-for-port` and `skip` modes.
- **Several launchers, one instance.** Every window has its own profile and its
  own hook run; launching the app again focuses the running instance.
- **Rename it, re-icon it.** Application name and icon (PNG/JPEG/WebP/GIF/ICO)
  apply to window titles, the taskbar, the tray and the macOS dock.
- **Four languages** — English, 简体中文, 繁體中文 and Português (Brasil),
  switchable at runtime, with a test that enforces catalogue completeness.
- **One commented `config.toml`**, safe to edit by hand: comments and unknown
  keys survive every save from the UI.

## Install

| Platform | Artifact |
| --- | --- |
| Linux (x86_64) | `.deb`, `.rpm`, `.AppImage` |
| macOS (Apple silicon) | `tauri-3080_0.1.0_aarch64.dmg` |
| macOS (Intel) | `tauri-3080_0.1.0_x64.dmg` |
| Windows (x86_64) | `tauri-3080_0.1.0_x64_en-US.msi`, `tauri-3080_0.1.0_x64-setup.exe` |

The `.deb` and `.rpm` declare `libwebkit2gtk-4.1` and `libayatana-appindicator3`
as dependencies. The macOS builds are not notarised yet, so the first launch
needs *System Settings → Privacy & Security → Open Anyway*. The Windows builds
are unsigned and will show a SmartScreen prompt.

## Configuration

`config.toml` and the hook scripts live in:

| Platform | Directory |
| --- | --- |
| Linux | `~/.config/tauri-3080` |
| macOS | `~/Library/Application Support/tauri-3080` |
| Windows | `%APPDATA%\tauri-3080` |

```toml
[profile.default]
id = "default"
name = "127.0.0.1:3080"
url = "http://127.0.0.1:3080"
prelaunch = "run"          # "run" | "wait-for-port" | "skip"
timeout_secs = 30
```

The hook lives in `scripts/<profile-id>/pre-launch.sh` (`pre-launch.ps1` on
Windows) and receives `TAURI_3080_PROFILE`, `TAURI_3080_URL` and
`TAURI_3080_CONFIG_DIR`. See the
[README](https://github.com/Iamliuxiaozhen/tauri-3080#readme) for the full
contract.

## What's changed

* Everything — this is the first release. The complete history is in
  [CHANGELOG.md](https://github.com/Iamliuxiaozhen/tauri-3080/blob/main/CHANGELOG.md).

## Known limitations

- No auto-updater yet.
- Pre-launch hooks are only auto-detected by file name inside the profile
  script directory.
- Linux tray support depends on the desktop environment providing an
  AppIndicator host.

**Full changelog**: https://github.com/Iamliuxiaozhen/tauri-3080/commits/v0.1.0
