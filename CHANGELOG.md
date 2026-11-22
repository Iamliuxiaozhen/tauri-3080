# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-11-23

### Added

- Native window that opens `http://127.0.0.1:3080`, built with Tauri 2.
- Launcher profiles: each profile has a name, a URL, an icon and its own
  pre-launch hook.
- Pre-launch hooks running from `~/.config/tauri-3080/scripts/<profile>/`, with
  live output, a hard timeout, cancellation and process-tree termination.
- `run`, `wait-for-port` and `skip` hook modes; `wait-for-port` probes the URL
  with a TCP connection before navigating.
- Multiple launcher windows, one per profile, opened from the window, the
  native menu or the tray.
- Settings window to rename the application, replace its icon (PNG, JPEG,
  WebP, GIF, ICO) and restore the default, in five sections.
- Internationalisation: English, Simplified Chinese, Traditional Chinese and
  Brazilian Portuguese, switchable at runtime, with a completeness test.
- Comment-preserving `config.toml` handling built on `toml_edit`; unknown keys
  survive every save.
- Native menu bar and tray icon whose labels follow the selected language.
- Linux, macOS and Windows support, including the macOS dock icon and the
  Windows taskbar icon refresh.
- CI: PR gate (typecheck, lint, unit tests, Rust tests, clippy, fmt), a build
  matrix for the three platforms, a release workflow that publishes draft
  releases, and an `xvfb` smoke test that boots the real binary.
- Issue and pull request templates, a conventional-commit PR title linter,
  path-based auto-labelling and an issue labeler.

[Unreleased]: https://github.com/Iamliuxiaozhen/tauri-3080/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/Iamliuxiaozhen/tauri-3080/releases/tag/v0.1.0
