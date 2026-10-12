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

- Config: `$XDG_CONFIG_HOME/tmgr/config.toml` (`~/.config/tmgr` when unset, also on macOS). Keys: `server` (default `https://api.tmgr.dev`), `default_workspace`, `web_url` (derived from `server` when unset), `branch_template` (default `{key_lower}-{slug}`; placeholders `key`, `key_lower`, `code_lower`, `seq`, `slug`), `review_status` (default `Review`). `tmgr config list|get|set|path` edits only these keys; `[projects]` (project code to repository path) is written by `tmgr project map`.
- Tokens are stored in the OS credential store under service `dev.tmgr.cli`, account `<server host>:default` (macOS Keychain; Linux Secret Service through a pure Rust client, no libdbus). Without a store they go to `$XDG_CONFIG_HOME/tmgr/credentials.json` (file mode 0600, directory 0700) and a warning is printed. A file readable by other users is refused. `TMGR_CREDENTIAL_STORE=file` forces the file store (used by the tests, so they never touch a real keychain).
- Tokens are never printed, logged or put into `--json` output or error messages. Redirects are not followed, so the bearer token cannot be forwarded to another host. Plain `http` is accepted only for localhost.

## Timer, git and prompt

- `tmgr start|stop|switch|done` drive the timer, the task status, a branch named from `branch_template` and the PR comment. They call only `GET` task/status routes, `PUT /api/tasks/{id}/{statusId}`, `GET /api/tasks/runned`, `POST|DELETE /api/tasks/{id}/countdown` and `POST /api/tasks/{id}/comments`. The branch key is the key in the task title (`TM-509: ...`), never `category_tasks_sequence_id`.
- Local state: `$XDG_STATE_HOME/tmgr/active.json` (`~/.local/state/tmgr`, mode 0600, written atomically; schema `tmgr.active.v1`). `tmgr prompt` and `tmgr statusline claude` read only this file: no network, no config. Garbage, oversized or control-character content prints nothing.
- Git arguments are data: branch names follow `[A-Za-z0-9._/-]` with no leading `-` or `..`, slugs are `[a-z0-9-]{1,40}`, git is always called with argv (never a shell) and `--` before positional arguments.
- `tmgr git install-hooks` writes a `commit-msg` hook that calls the binary by absolute path; an existing hook is moved to `commit-msg.tmgr-prev` and still runs first. The hook never fails a commit. Installs that affect every repository (`--global`, or a `core.hooksPath` from the global config or pointing outside the repository) need an interactive confirmation that `--yes` does not satisfy.
- `tmgr init <shell> --install|--uninstall` edits the rc file between `# >>> tmgr init >>>` markers after printing a diff and asking; it writes `<file>.tmgr-backup-<timestamp>` first and follows symlinks. Without `--install` nothing is written.
- Exit code 10 (needs_confirmation): a confirmation was required but there is no terminal.

## Today, log, notes and pages

- `tmgr today` combines `GET /api/workspaces/{id}/time-summary`, the workspace agent work list (`GET /api/workspaces/{id}/agent-work`, runs clipped to the local day) and `GET /api/daily-routines/expand`. `tmgr routine done` never calls the toggling `POST /api/daily-routines/tasks/{id}/complete` for a routine that is already done.
- `tmgr log --follow` polls every 5 s; `TMGR_FOLLOW_INTERVAL_MS` and `TMGR_FOLLOW_OFFLINE_MS` shorten the interval and the offline give-up time (used by the tests).
- `tmgr note` runs `$VISUAL`, then `$EDITOR`, then `vi`. The value is split with shell quoting rules (single and double quotes, backslash) and never expanded or passed to a shell; the temporary file path is its own argument.
- Pages are read and written with `X-Workspace-Id` set to the workspace stored in the directory index. Page file names come from a strict slug grammar, never from server slugs; everything is opened relative to `<dir>` with `O_NOFOLLOW` (`<dir>` itself may be a symlink you typed). `.tmgr-pages.json` records the version, update time and body hash of each pulled file, so push can tell which files changed and refuses files it did not pull. The `version:` line in the front matter is the base version of a push. To resolve a conflict merge `<slug>.remote.md` into `<slug>.md`, set `version:` to the value in the remote copy, delete the remote copy and push again.

## JSON schemas

`crates/tmgr-cli/schemas/tmgr.v1/*.json` (JSON Schema 2020-12) describe the whole envelope of `error`, `status`, `task`, `task_list`, `task_created`, `whoami` (also the result of `login`), `start` (also `switch`), `stop`, `done`, `today`, `run_list`, `run` (one NDJSON line of `log --follow --json`), `routine_list`, `routine_done`, `page_ref` (`note`), `page_sync` (`page pull|push`) and `page_status`. The integration tests validate real command output against them. They live inside the crate so the CI path filter (`crates/**`) covers them.

## Exit codes

0 ok, 1 error, 2 usage, 3 not found, 4 auth, 5 forbidden (including `insufficient_scope`, with a `tmgr login --scopes` hint), 6 conflict (including a dirty working tree), 7 offline, 10 needs confirmation, 130 interrupted (also a declined confirmation).

## Task references

`TM-512` (key as shown in the title), `#8974` or `8974` (global task id). `tmgr help refs` prints the grammar.
