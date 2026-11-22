<!--
  Pull request template.
  Rendered for every new pull request in this repository.
  See CONTRIBUTING.md for the full workflow.
-->

## Summary

<!-- What does this change, in one or two sentences? -->

## Motivation

<!-- Why is this needed? Link the issue: Closes #123 -->

Closes #

## Type of change

- [ ] Bug fix (`fix`)
- [ ] New feature (`feat`)
- [ ] Refactor with no behaviour change (`refactor`)
- [ ] Documentation (`docs`)
- [ ] Build, CI or tooling (`build` / `ci`)
- [ ] Other (`chore`, `perf`, `test`, `style`, `revert`)

## Platforms affected

- [ ] Linux
- [ ] macOS
- [ ] Windows
- [ ] All / platform independent

## How was it tested?

<!--
  Be concrete: commands you ran, profiles you tried, what you clicked.
  "Ran the app" is not enough — tell us what you observed.
-->

- [ ] `npm run check` passes locally
- [ ] `cargo test --manifest-path src-tauri/Cargo.toml` passes locally
- [ ] `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` is clean
- [ ] The smoke mode boots and reports `ok: true` (only needed for launcher changes)

**Manual verification:**

<!-- e.g. Started a server on port 4000, created a "Dev" profile pointing at it,
     set the hook to wait-for-port, confirmed the window waited then opened. -->

## Screenshots or recordings

<!-- Required for user-visible changes. Drop images here. -->

## Checklist

- [ ] I have read [CONTRIBUTING.md](../CONTRIBUTING.md).
- [ ] The pull request title follows [Conventional Commits](https://www.conventionalcommits.org/).
- [ ] New user-facing strings are translated in all four locales.
- [ ] I added or updated tests for the behaviour I changed.
- [ ] I updated the documentation (README, comments or CHANGELOG) where relevant.
- [ ] My change does not introduce new dependencies without a short justification below.

## Notes for reviewers

<!-- Anything unusual, intentional trade-offs, follow-up work. -->
