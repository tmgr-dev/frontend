# Changelog

## 0.3.0

- `tmgr start <ref>`: creates or switches to a branch from `branch_template` (default `{key_lower}-{slug}`, e.g. `tm-509-repo-prep-cargo-workspace`), moves a backlog task to the first active status, starts the timer and writes `~/.local/state/tmgr/active.json`. Flags: `--branch`, `--no-branch`, `--from`, `--no-timer`, `--status`, `--stash`. A dirty tree is refused with exit 6. A running timer on another task is stopped after confirmation (non-interactive: exit 10 unless `--yes`).
- `tmgr stop`, `tmgr switch <ref>`, `tmgr done [<ref>]` (`--status`, `--no-pr`, `--comment`, `--force`): `done` stops the timer, moves the task to the `review_status` status (default "Review", else the first completed status), looks up the PR with `gh pr view` or builds the GitHub/GitLab compare URL from `origin`, and posts it as a comment.
- `tmgr project map|unmap|list`: maps a project code to a local repository (`[projects]` in `config.toml`).
- `tmgr git install-hooks [--global]`: installs a `commit-msg` hook that chains an existing one (moved to `commit-msg.tmgr-prev`). `tmgr git hook commit-msg <file>` appends `Refs: <KEY>` for branches named after a task and never fails the commit. A global install needs an interactive confirmation.
- `tmgr prompt [--format plain|zsh|bash|fish|starship|tmux|claude]` and `tmgr statusline claude`: print `⏱ TM-509 1h12m` from the local state file only, with no network access.
- `tmgr init zsh|bash|fish|starship|tmux|claude` prints the snippet; `--install` / `--uninstall` edit the config file after showing a diff, asking, and writing a timestamped backup.
- `config` keys `branch_template` and `review_status`. Global `--yes` accepts non-privileged confirmations. New exit code 10 (needs_confirmation).
- `start` finds timers running on other tasks through `GET /api/tasks/runned` (falls back to the local state file when the route is unavailable); `tmgr status` shows the running task (the latest one, with a warning when several run).
- JSON schemas `start`, `stop`, `done`.

## 0.2.0

- `tmgr login` (`auth login`): device-code sign-in with `--scopes`, `--label`, `--no-browser`, `--with-token` (token from stdin). Tokens go to the OS credential store (macOS Keychain, Linux Secret Service) and fall back to a 0600 file when no store exists.
- `tmgr logout`, `tmgr whoami` (`auth status`), `tmgr ws list|use`, `tmgr config list|get|set|path`.
- `tmgr` / `tmgr status`: workspace and today's tracked time.
- `tmgr task show <ref>`, `tmgr task list` (cursor pagination), `tmgr add` (arguments or `-` for stdin).
- Global `--json` (schema `tmgr.v1`, JSON Schemas in `crates/tmgr-cli/schemas/tmgr.v1/`), `--ws`, `--no-color`, `-q`, `-v`, `--server`.
- Stable exit codes: 2 usage, 3 not found, 4 auth, 5 forbidden, 6 conflict, 7 offline, 130 interrupted.
- Automatic token refresh on 401 (one retry); redirects are never followed.
- Raised the minimum Rust version to 1.89.

## 0.1.0

First prerelease: the `version` command and the release pipeline only.
