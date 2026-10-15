# crates

Rust workspace of the `tmgr` command line tool. It is a separate Cargo workspace from `src-tauri` (the desktop app): both ship an executable named `tmgr`, so they cannot share one workspace. `src-tauri` keeps its own `Cargo.lock` and target directory.

- `tmgr-core` - shared library (envelope `tmgr.v1`, errors and exit codes, config, credential store, API client, task reference grammar).
- `tmgr-cli` - the `tmgr` binary (clap).
- `tmgr-usage` - local AI tool usage engine (library, no network): versioned SQLite store, incremental log scanner, Claude Code and Codex collectors, rate limit snapshots, price book (bundled filtered LiteLLM snapshot, MIT, see `pricing/NOTICE`), aggregation and the pure run-linking ladder (`link::attribute`). Parsers declare only whitelisted keys, so prompt, response and tool text is never deserialised or stored. Data dir `$TMGR_USAGE_DIR`, else `$XDG_DATA_HOME/tmgr/usage`, else `~/.local/share/tmgr/usage` (0700); it holds `usage.db`, `snapshots/` (Claude statusLine snapshots) and `accounts.json`.
- `tmgr-queue` - the local auto-run queue of the runner (rules, pickups, approvals, audit); used by the hidden `tmgr queue` commands behind the `runner` feature.
- `tmgr-local` - local (desktop) workspaces: reads `~/.tmgr.dev/workspaces/<code>/workspace.db` (`TMGR_LOCAL_ROOT` overrides the root) and answers the REST-shaped local API with the same JSON as the desktop's TS local API. `tests/parity` holds scenarios and goldens shared with `src/local/__tests__/parity.test.ts`. Writes open the database with the desktop's pragmas and run each request in one `BEGIN IMMEDIATE` transaction. Deliberate differences from the TS handlers, which are only statement-atomic: a request that fails after some writes leaves nothing (a refusal keeps only its `activity_log` row); a persona write whose audit row cannot be written answers 500 with nothing stored; `daily-routines/tasks/:id/complete-on` answers 422 unless the date is a real `YYYY-MM-DD` day. A write also re-checks the schema after taking the lock. Writes from tmgr fire no desktop page or task events, so an open desktop window shows them after a reload (TM-539). The schema is never migrated: anything but schema 12 is refused.

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

## Local workspaces

`--ws local:<code>` (or `tmgr ws use local:<code>`) works on a workspace of the desktop app in `~/.tmgr.dev/workspaces/<code>` (`TMGR_LOCAL_ROOT` overrides the folder) through `tmgr-local`: no login and no network. `tmgr ws list` shows local workspaces next to the cloud ones, also when you are logged out. Login, logout, whoami, notify and alarms always use the cloud. Local tasks have no web link (`url` is null). A local workspace keeps only each task's total time, not when it was tracked, so `today` and `status` report tracked time as unavailable there (`null`, warning `local_time_unavailable`); the local `time-summary` route keeps the cloud's semantics (the whole tracked time of the tasks updated in the range). `stop`, an implicit `done`, `agent update|finish` and the hooks act in the workspace where the timer or run was started, whatever workspace is selected; `page push` only writes to the workspace a directory was pulled from.

Identity: the CLI acts as the desktop's local account, id -1 with the name from `profile.json` next to the workspaces folder (`~/.tmgr.dev/profile.json`). The desktop keeps the id of a signed-in account only in the app's memory and web storage, so when the desktop is signed in, set `tmgr config set local_user_id <id>` to act as that user. Otherwise agent runs started by the CLI cannot be updated in the desktop and the other way round, personas owned by the account cannot be assigned from the CLI, and comments and runs carry author id -1.

Reads open the database read-only; a write takes the desktop's write lock for one request and fails with a conflict (exit 6) when the desktop holds it for more than 5 seconds. A workspace with another schema than 12 is refused (open it in the desktop to upgrade it). A request the local API does not answer fails with "... is not supported for local workspaces yet" (exit 2).

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

## AI usage

`tmgr usage` is a read-only view over the local `tmgr-usage` database (see the crate paragraph above); the CLI owns the network (price update) and the run lookup, the engine never calls a server.

- Sources: the accounts `claude` (`~/.claude`) and `codex` (`~/.codex`), an extra `claude-env` / `codex-env` account when `$CLAUDE_CONFIG_DIR` / `$CODEX_HOME` point somewhere else, plus accounts added with `tmgr usage accounts add`. Account roots are shown as `~/...` or `<external>/name`. Logs are only read, new lines are picked up incrementally on every `tmgr usage`, and the log contents never leave the machine. History older than 90 days is folded into per-day totals; the days are fixed at fold time in the local time zone of that run; rollups folded before this version keep UTC day boundaries and add up with newer rows under the same day label.
- Network and credentials: scanning, the totals, limits and the statusLine hook need no credentials and make no request. The one authenticated request is the task view: `--by task` and `--task` use your own TMGR login to read your Agent work runs (cloud workspaces) and your local workspaces; they read only run metadata (task, session id, branch, times). `usage pricing update` fetches the public LiteLLM list without credentials (only the LiteLLM list on `raw.githubusercontent.com` over https, no credentials, port or odd path, or a loopback address; redirects are re-checked on every hop, at most 3). Vendor tokens, API keys and keychains of Claude Code or Codex are never read.
- Privacy: only token counts, model, timestamps, git branch, session id and the working directory are kept (the directory as a name and a SHA-256 hash in all output). Prompt, response and tool text is not parsed. `tmgr usage reset` deletes everything stored.
- Cost wording: every dollar figure is API-equivalent (tokens times public list prices from the bundled or downloaded LiteLLM list), not what a subscription costs. Models without a price are counted, flagged and left out of the sums.
- Task linking (`--by task`, `--task REF`): runs of your cloud workspaces and of all local workspaces become time windows; each event goes to one run by session id, then branch and window, then repository and window. The repository of a cloud run comes from the local `runs.json` state or the `[projects]` mapping of `tmgr project map` (matched by the exact path the tool logged as its working directory). A branch match without a known repository on both sides is ambiguous when several repositories use that branch name in the run window. Unlinked usage stays visible as "unattributed", usage that fits several runs as "ambiguous". Runs that started more than 3 days before the period are not fetched.
- Rate limits come from Codex session logs and, for Claude Code, from the statusLine snapshot. The statusLine integration is opt-in: `tmgr usage statusline install` only prints the change (the `statusLine` key only, secret-looking commands show as `<redacted>`); `--install` needs a real terminal confirmation (never `--yes`), refuses to write when `settings.json` changed meanwhile (exit 6, re-checked right before the rename) or is a dangling symlink or a symlink to a directory, keeps a uniquely named timestamped backup and chains the command you had before. Snapshots are filed under a hash of the account's config directory and deleted after 30 days. `tmgr usage statusline uninstall` undoes it. The hook stores a whitelisted snapshot (session id, model, rate limits, cost) and never the prompt, transcript path or working directory.
- Tests set `HOME`, `XDG_*`, `TMGR_USAGE_DIR`, `CLAUDE_CONFIG_DIR`, `CODEX_HOME` and `TMGR_LOCAL_ROOT` to temporary directories and use synthetic logs.

## Board

`tmgr board [-p CODE] [--mine] [--ascii]` is a ratatui kanban (code in `tmgr-cli/src/cmd/board/`: `app.rs` state and update logic, `view.rs` rendering, `data.rs` API calls, `theme.rs` colours and glyphs). It calls only the wave-1 routes plus `GET /api/tasks/{id}/agent-work`; network calls run on a worker thread, the UI never blocks. Snapshot tests render frames with ratatui's `TestBackend` into `src/cmd/board/snapshots/` (`TMGR_UPDATE_SNAPSHOTS=1 cargo test` rewrites them). Timer keys do not touch `active.json`, so `tmgr prompt` reflects only timers started with `tmgr start`.

## JSON schemas

`crates/tmgr-cli/schemas/tmgr.v1/*.json` (JSON Schema 2020-12) describe the whole envelope of `error`, `status`, `task`, `task_list`, `task_created`, `whoami` (also the result of `login`), `start` (also `switch`), `stop`, `done`, `today`, `run_list`, `run` (one NDJSON line of `log --follow --json`), `usage_summary`, `usage_task`, `usage_limits`, `usage_accounts`, `usage_statusline`, `routine_list`, `routine_done`, `page_ref` (`note`), `page_sync` (`page pull|push`) and `page_status`. The integration tests validate real command output against them. They live inside the crate so the CI path filter (`crates/**`) covers them.
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

## Queue (`tmgr queue`, hidden, `runner` feature)

`tmgr-queue` is the local auto-run queue. It is off by default. `run-once` starts approved pickups through the runner (`runs.create` with the bundle, `permission_mode = ask`, the start key, `clone_of` and `agent_work`); the CLI implements `RunsClient` over the runner client.

Known limits:

- Authorship comes from the edit journal (schema 13, created by the desktop app). Only the owner's edits in the desktop UI count as `human`; every CLI write is recorded as `unattended`, and a workspace still on schema 12 records nothing, so its tasks are `unrecorded`. Such tasks are skipped unless a rule lists `unrecorded` (manual approval only) or the owner confirms the exact text with `tmgr task confirm`.
- The per-run `git clone --reference` only protects the owner's checkout from incidental git writes. Filesystem isolation of queue runs is the runner sandbox's job (always on).

A pass that may start or check a run (an approved or live pickup, or an `auto` rule) starts the runner when it is down and needs its caps `run_bundle.v1`, `permission_mode.v1`, `start_key.v1`, `run_status.v1`, `clone_dir.v1`, `agent_work.v1` and `exe_pin.v1` (`runs::REQUIRED_CAPS`); every start carries the approved executable pin; otherwise the pass stops before any pickup changes. Only a `not_found` answer from the runner frees a slot: a run the runner no longer knows counts as exited, a start key it does not know as never created, and any transport error leaves the pickup as it is.
