# crates

Rust workspace of the `tmgr` command line tool. It is a separate Cargo workspace from `src-tauri` (the desktop app): both ship an executable named `tmgr`, so they cannot share one workspace. `src-tauri` keeps its own `Cargo.lock` and target directory.

- `tmgr-core` - shared library (envelope `tmgr.v1`, errors and exit codes, config, credential store, API client, task reference grammar).
- `tmgr-cli` - the `tmgr` binary (clap).

## Build and test

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p tmgr-cli -- version --json
```

## Releases

Tag `cli-vX.Y.Z` (must equal the `tmgr-cli` version and have a `## X.Y.Z` section in `tmgr-cli/CHANGELOG.md`) runs `.github/workflows/cli-release.yml`: musl Linux and macOS archives, `SHA256SUMS`, minisign signatures, build provenance attestation, macOS codesign and notarization. When a signing secret is missing the release is published as a prerelease and says what was skipped. `workflow_dispatch` is a dry run that publishes nothing.

## Configuration and credentials

- Config: `$XDG_CONFIG_HOME/tmgr/config.toml` (`~/.config/tmgr` when unset, also on macOS). Keys: `server` (default `https://api.tmgr.dev`), `default_workspace`, `web_url` (derived from `server` when unset). `tmgr config list|get|set|path` edits only these keys.
- Tokens are stored in the OS credential store under service `dev.tmgr.cli`, account `<server host>:default` (macOS Keychain; Linux Secret Service through a pure Rust client, no libdbus). Without a store they go to `$XDG_CONFIG_HOME/tmgr/credentials.json` (file mode 0600, directory 0700) and a warning is printed. A file readable by other users is refused. `TMGR_CREDENTIAL_STORE=file` forces the file store (used by the tests, so they never touch a real keychain).
- Tokens are never printed, logged or put into `--json` output or error messages. Redirects are not followed, so the bearer token cannot be forwarded to another host. Plain `http` is accepted only for localhost.

## JSON schemas

`crates/tmgr-cli/schemas/tmgr.v1/*.json` (JSON Schema 2020-12) describe the whole envelope of `error`, `status`, `task`, `task_list`, `task_created` and `whoami` (also the result of `login`). The integration tests validate real command output against them. They live inside the crate so the CI path filter (`crates/**`) covers them.

## Exit codes

0 ok, 1 error, 2 usage, 3 not found, 4 auth, 5 forbidden (including `insufficient_scope`, with a `tmgr login --scopes` hint), 6 conflict, 7 offline, 130 interrupted.

## Task references

`TM-512` (key as shown in the title), `#8974` or `8974` (global task id). `tmgr help refs` prints the grammar.
