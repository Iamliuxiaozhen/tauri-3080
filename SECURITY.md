# Security policy

## Supported versions

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |

Only the latest release receives security fixes. Releases are published from
this repository and are code-signed where the platform allows it (see the
release notes).

## Reporting a vulnerability

**Please do not open a public issue for a security problem.**

Report it privately through
[GitHub Security Advisories](https://github.com/Iamliuxiaozhen/tauri-3080/security/advisories/new).

Please include:

- the version and platform,
- a description of the issue and its impact,
- reproduction steps, ideally a minimal `config.toml` and hook script,
- any suggestion for a fix.

You can expect:

| Stage                  | Target         |
| ---------------------- | -------------- |
| Acknowledgement        | within 3 days  |
| Initial assessment     | within 7 days  |
| Fix or mitigation plan | within 30 days |

We will credit you in the advisory unless you prefer to stay anonymous.

## Threat model

`tauri-3080` is a local desktop launcher. It deliberately:

- **runs local scripts.** A pre-launch hook is arbitrary code, executed with
  the user's privileges. The configuration directory is the trust boundary: if
  an attacker can write to `~/.config/tauri-3080/scripts`, they can already run
  code as that user. The app never fetches, generates or downloads scripts.
- **loads one remote URL** into the window (`http://127.0.0.1:3080` by default).
  That page runs with the capabilities listed in
  `src-tauri/capabilities/default.json`; the set is intentionally small (no
  filesystem plugin, no shell plugin, no `asset:` protocol).
- **keeps everything on the machine.** There is no telemetry, no network
  request other than the profile URL and the TCP probe used by
  `wait-for-port`, and no update check.

## Hardening already in place

- Script and icon names are validated; `..`, `/` and `\` are rejected.
- Hook scripts must live inside the configuration directory.
- Script output is truncated before it reaches the UI.
- Every hook run has a hard timeout and is killed as a whole process group.
- `config.toml` is written atomically (write + rename), so a crash never leaves
  a half-written file.
- URLs are restricted to `http://`, `https://` and `file://`.
- Release builds use the Tauri capability model with an explicit allow-list.

## Out of scope

- Vulnerabilities in the local service listening on the profile port.
- Issues that require an attacker to already have write access to the user's
  configuration directory.
- Social engineering.
