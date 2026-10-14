# crates

Rust workspace of the `tmgr` command line tool. It is a separate Cargo workspace from `src-tauri` (the desktop app): both ship an executable named `tmgr`, so they cannot share one workspace. `src-tauri` keeps its own `Cargo.lock` and target directory.

- `tmgr-core` - shared library (envelope `tmgr.v1`, errors and exit codes, config, credential store, API client, task reference grammar).
- `tmgr-cli` - the `tmgr` binary (clap).

## Install

```bash
curl -fsSL https://tmgr.dev/install.sh | sh                 # Linux and macOS, into ~/.local/bin, no sudo
curl -fsSL https://tmgr.dev/install.sh | TMGR_CHANNEL=beta sh   # also prereleases
nix profile install github:tmgr-dev/frontend#tmgr-cli        # Nix (built from source)
cargo install --locked --git https://github.com/tmgr-dev/frontend tmgr-cli
brew install tmgr-dev/tap/tmgr                               # once the tap exists, see below
```

Windows: use WSL2 and the Linux installer inside it. The installer needs `curl`, downloads the signed `manifest.json`, requires `minisign` to verify it, then checks channel, expiry, that the installed tmgr is not newer (`TMGR_ALLOW_DOWNGRADE=1` overrides) and the archive sha256 from the manifest. It refuses an install directory you do not own or that is group- or world-writable. While the release signing key is not set up, or when `minisign` is missing, it refuses unless `TMGR_ALLOW_UNSIGNED=1` (loud warning).

The desktop app also installs a `tmgr` executable, so `tmgr-cli` and the desktop Nix package conflict in one profile.

`tmgr self-update [--check] [--channel stable|beta] [--allow-downgrade]` replaces the binary with the newest release that has a valid minisign signature on `manifest.json` (public key compiled in from `tmgr-core/src/update_key.pub`; without a real key it refuses with exit 5). It checks the manifest expiry and the archive sha256, refuses versions older than the highest one already trusted on this machine (`~/.local/state/tmgr/update.json`, never lowered, updates serialised by `update.lock`; `--allow-downgrade` needs a terminal), searches up to 10 pages of releases and reports an incomplete search instead of guessing, keeps the previous binary as `tmgr.old` and refuses when Homebrew, Nix, cargo or npm manages the binary (it prints their upgrade command instead).

Owner steps before signed releases and Homebrew work: generate a key with `minisign -G`, put the public key into `crates/tmgr-core/src/update_key.pub` and the base64 line into `MINISIGN_PUBKEY` in `public/install.sh`, add the secrets `CLI_MINISIGN_SECRET_KEY` (and `CLI_MINISIGN_PASSWORD` for an encrypted key). For Homebrew create the repository `tmgr-dev/homebrew-tap`, add `Formula/tmgr.rb` from the `homebrew-formula` artifact of a release run (rendered from `packaging/homebrew/tmgr.rb.tmpl`), and later a `HOMEBREW_TAP_TOKEN` secret (contents write on the tap only) so the workflow can push it.

## Build and test

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p tmgr-cli -- version --json
```

## Releases

Tag `cli-vX.Y.Z` (must equal the `tmgr-cli` version and have a `## X.Y.Z` section in `tmgr-cli/CHANGELOG.md`) runs `.github/workflows/cli-release.yml`: musl Linux and macOS archives, `SHA256SUMS`, `manifest.json` (product, version, channel, key epoch, expiry, per-target sha256 and size), minisign signatures of all three, build provenance attestation, macOS codesign and notarization. When a signing secret is missing the release is published as a prerelease and says what was skipped. `workflow_dispatch` is a dry run that publishes nothing.

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

## Board

`tmgr board [-p CODE] [--mine] [--ascii]` is a ratatui kanban (code in `tmgr-cli/src/cmd/board/`: `app.rs` state and update logic, `view.rs` rendering, `data.rs` API calls, `theme.rs` colours and glyphs). It calls only the wave-1 routes plus `GET /api/tasks/{id}/agent-work`; network calls run on a worker thread, the UI never blocks. Snapshot tests render frames with ratatui's `TestBackend` into `src/cmd/board/snapshots/` (`TMGR_UPDATE_SNAPSHOTS=1 cargo test` rewrites them). Timer keys do not touch `active.json`, so `tmgr prompt` reflects only timers started with `tmgr start`.

## JSON schemas

`crates/tmgr-cli/schemas/tmgr.v1/*.json` (JSON Schema 2020-12) describe the whole envelope of `error`, `status`, `task`, `task_list`, `task_created`, `whoami` (also the result of `login`), `start` (also `switch`), `stop`, `done`, `today`, `run_list`, `run` (one NDJSON line of `log --follow --json`), `routine_list`, `routine_done`, `page_ref` (`note`), `page_sync` (`page pull|push`) and `page_status`. The integration tests validate real command output against them. They live inside the crate so the CI path filter (`crates/**`) covers them.
## Agent mode, hooks and notifications

- `tmgr agent start|update|finish` drive an Agent work run (`POST /api/tasks/{id}/agent-work`, `PATCH /api/agent-work/{run}`, `POST /api/agent-work/{run}/finish`, scope `agent-work:write`). Runs are remembered in `$XDG_STATE_HOME/tmgr/runs.json` (`~/.local/state/tmgr`, mode 0600, schema `tmgr.runs.v1`) keyed by task, branch and repository.
- `tmgr agent status set <session-id> --state S [--task REF] [--profile ID] [--since T]`, `tmgr agent status clear <session-id>` and `tmgr agent status list` are the hooks the runner calls to publish session state (`PUT` / `DELETE /api/agent-status/{session}`, scope `agent-status:write`, not in the default login; `GET /api/workspaces/{id}/agent-status`, scope `tasks:read`). `set` and `clear` are refused with exit 5 until `tmgr config set upload_status true` (default off); the upload carries only the task, state, agent profile id, timestamps and the device's host label, never terminal content. A 429 exits 6 and names the `Retry-After` wait; a workspace with Agent work off gives the warning `agent_work_disabled` and exit 0. JSON schemas: `agent_status_session` (set, clear) and `agent_status_list`.
- `tmgr hook claude <event>` / `tmgr hook codex notify` never fail the agent: exit 0 always, no stdout, no shell, payload size bounded, 1.5 s watchdog. They use an existing notify token and login only; they never mint a token or refresh a session (a kill in the middle would lose a rotated refresh token), so an expired login silently skips the run update until the next `tmgr` command refreshes it.
- `tmgr hook install claude|codex` edits `~/.claude/settings.json` (`CLAUDE_CONFIG_DIR` is honoured), `.claude/settings.json` with `--project`, or `~/.codex/config.toml` (`CODEX_HOME` is honoured). Hook commands call this binary by absolute path; `--project` writes that path into a file you may commit, so it warns.
- `tmgr notify`, `tmgr alarm`, `tmgr mcp` use a notify token (`tmgrn_...`), never the login token. Order: `TMGR_NOTIFY_TOKEN` (server `TMGR_URL`, else the configured one), the legacy `~/.config/tmgr-notify/env` (ignored unless mode 0600 and owned by you), the credential store entry `<server>:default:notify`, else mint through `POST /api/notify-tokens` (scope `notify:send`) and store it. `tmgr logout` calls `DELETE /api/notify-tokens/{id}`.
- Alarms exit 0 only when acknowledged, 1 for any other final status (the status is still printed).

## JSON schemas

`crates/tmgr-cli/schemas/tmgr.v1/*.json` (JSON Schema 2020-12) describe the whole envelope of `error`, `status`, `task`, `task_list`, `task_created`, `whoami` (also the result of `login`), `start` (also `switch`), `stop`, `done`, `agent_run`, `agent_status_session`, `agent_status_list`, `notify_result` and `alarm`. The integration tests validate real command output against them. They live inside the crate so the CI path filter (`crates/**`) covers them.

## Exit codes

0 ok, 1 error, 2 usage, 3 not found, 4 auth, 5 forbidden (including `insufficient_scope`, with a `tmgr login --scopes` hint), 6 conflict (including a dirty working tree), 7 offline, 10 needs confirmation, 130 interrupted (also a declined confirmation).

## Task references

`TM-512` (key as shown in the title), `#8974` or `8974` (global task id). `tmgr help refs` prints the grammar.
