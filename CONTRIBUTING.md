# Contributing to tauri-3080

Thanks for taking the time to contribute! This document explains how to get a
change merged with as little friction as possible.

## Code of conduct

By participating you agree to follow the [Code of Conduct](CODE_OF_CONDUCT.md).

## Ways to help

- **Report a bug** — open an issue using the bug template and include the
  version, OS and the relevant part of `config.toml` (redact secrets).
- **Request a feature** — describe the problem you are trying to solve, not only
  the solution you have in mind.
- **Translate** — the UI ships with English, Simplified Chinese, Traditional
  Chinese and Brazilian Portuguese. Adding a language is one file plus one test.
- **Improve the docs** — README, templates and comments are all fair game.

## Development setup

Prerequisites:

| Tool              | Version                                                                  |
| ----------------- | ------------------------------------------------------------------------ |
| Node.js           | 22.12+                                                                   |
| Rust              | 1.77.2+                                                                  |
| Tauri system deps | see the [prerequisites guide](https://v2.tauri.app/start/prerequisites/) |

```bash
npm ci
npm run tauri:dev     # builds the Rust side and serves the UI with hot reload
```

Useful commands:

| Command                                                                          | What it does                                           |
| -------------------------------------------------------------------------------- | ------------------------------------------------------ |
| `npm run dev`                                                                    | Frontend only, in a browser (`http://localhost:1420`). |
| `npm run typecheck`                                                              | `tsc --noEmit`.                                        |
| `npm run lint`                                                                   | ESLint, zero warnings allowed in CI.                   |
| `npm run format`                                                                 | Prettier.                                              |
| `npm test`                                                                       | Vitest unit tests.                                     |
| `npm run check`                                                                  | Everything above, in the order CI runs it.             |
| `cargo test --manifest-path src-tauri/Cargo.toml`                                | Rust unit tests.                                       |
| `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` | Lints.                                                 |
| `cargo fmt --manifest-path src-tauri/Cargo.toml`                                 | Format Rust.                                           |

## Project layout

```text
src/                frontend (TypeScript, no framework)
src/i18n/           one catalogue per language
src/lib/            IPC bridge and the launch state machine
src-tauri/src/      Rust backend, one module per concern
.github/            workflows, templates, labeler rules
```

Two rules keep the codebase readable:

1. **The backend owns the filesystem.** The frontend never reads or writes
   files directly; it calls a command in `src-tauri/src/commands.rs`.
2. **Logic that can be tested without a DOM lives in `src/lib/`.** The launcher
   state machine (`launch-flow.ts`) and the i18n runtime are plain modules with
   unit tests.

## Commit messages

We use [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/):

```text
<type>(<scope>): <summary>

[optional body]

[optional footer]
```

Allowed types: `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`,
`build`, `ci`, `chore`, `revert`.

Examples:

```text
feat(launcher): wait for the port before navigating
fix(config): keep comments when deleting a profile
docs(readme): document the smoke test
```

Pull request titles are linted against the same rules, because the squash merge
uses them as the final commit message.

## Pull requests

1. Fork the repository and create a topic branch:
   `feat/short-description` or `fix/short-description`.
2. Keep the change focused. Unrelated refactors make review harder — split them.
3. Add or update tests. Bug fixes should come with a test that fails before the
   fix.
4. Run `npm run check` and the Rust commands above before pushing.
5. Fill in the pull request template: what changed, why, and how you tested it.
6. Wait for the **PR gate** workflow. It runs typecheck, lint, unit tests on
   Linux/macOS/Windows, Rust tests, clippy and a smoke build. All checks must be
   green.

Draft pull requests are welcome for work in progress — CI still runs, but
reviewers will wait until you mark it ready.

## Adding a language

1. Copy `src/i18n/en.ts` to `src/i18n/<tag>.ts` and translate the values.
2. Register the tag in `SUPPORTED_LOCALES` and `LOCALE_LABELS` in
   `src/i18n/index.ts`.
3. Add the catalogue to `CATALOGUES`.
4. Add the locale to `SUPPORTED_LANGUAGES` in `src-tauri/src/config.rs` so it
   survives validation, and to the translation table in `src-tauri/src/i18n.rs`
   for the native menu labels.
5. Run `npm test`. The completeness test lists every missing or extra key and
   checks that placeholders (`{name}`) are preserved.

## Coding style

- TypeScript: strict mode, no `any`, no non-null assertions without a comment.
- Rust: `cargo fmt`, clippy clean at `-D warnings`.
- Comments explain _why_, not _what_; the code already says what it does.
- User-facing strings go through the i18n catalogue — never hard-code English in
  the UI.

## Release process (maintainers)

1. Update the version in `package.json`, `src-tauri/Cargo.toml` and
   `src-tauri/tauri.conf.json` (they must match).
2. Add an entry to `CHANGELOG.md`.
3. Merge to `main`.
4. Push the tag (`git tag -a vX.Y.Z && git push origin vX.Y.Z`). The **Release**
   workflow builds installers for all three platforms and creates a **draft**
   release.
5. Rewrite the notes for a first release — GitHub's generated notes only cover
   merged pull requests, so `docs/release-notes-v0.1.0.md` is the model to copy.
6. Review the notes and artifacts, then publish:

   ```bash
   gh release edit vX.Y.Z --draft=false
   ```
