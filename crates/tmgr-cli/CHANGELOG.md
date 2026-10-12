# Changelog

## Unreleased

- `tmgr self-update [--check] [--channel stable|beta] [--allow-downgrade]`: updates the binary from a GitHub release whose `manifest.json` carries a valid minisign signature (public key compiled in; a build without a key refuses with exit 5). Checks expiry, the archive sha256 and a rollback floor kept in `~/.local/state/tmgr/update.json`; keeps `tmgr.old`; refuses Homebrew, Nix, cargo and npm installs and prints their upgrade command.
- `https://tmgr.dev/install.sh` installer, `nix/tmgr-cli.nix` (`packages.<system>.tmgr-cli`) and a Homebrew formula template; releases now include `manifest.json`.
- `tmgr today [--date YYYY-MM-DD|--yesterday] [--mine]`: tracked time per task (people), agent time per task (workspace agent work runs clipped to the day in your local time zone), status and project, a time split bar and the routines due that day. `--mine` filters agent runs by your user; tracked time of people is workspace-wide on the server and stays unfiltered (warning `human_time_workspace_wide`).
- `tmgr routine list [--date D]` and `tmgr routine done <id> [--date D]`. `done` looks the routine up first and uses the idempotent instance route, so running it twice never un-completes a routine; a routine without an instance can only be completed for today.
- `tmgr log [--task REF] [--since 2h] [--agent NAME] [--limit N] [--cursor C]`: agent work runs of the workspace, newest first. `--follow` polls every 5 seconds, prints each new or changed run once (NDJSON `{"schema":"tmgr.v1","event":"run.updated","data":{...}}` with `--json`), backs off while offline (exit 7 after about a minute) and exits 130 on Ctrl-C. A workspace with agent work turned off gives an empty result and the warning `agent_work_disabled`.
- `tmgr note [TITLE] [--title T] [--task REF] [--parent PAGE_ID]`: opens `$VISUAL` / `$EDITOR` (fallback `vi`, split into arguments without a shell) on a 0600 file in a private 0700 directory, saves the note as a page (an empty or untouched note is cancelled, exit 130) and links it to the task with a `tmgr://task/<id>` line. When saving fails the note is kept and its path is printed.
- `tmgr page pull <dir> [--tree PAGE_ID|--all] [--force]`, `tmgr page push [<file>...]`, `tmgr page status [<dir>]`: pages as `<slug>.md` files (slug `[a-z0-9-]{1,60}`, `-<id>` added on a name collision) with a `tmgr:` front matter block (`page_id`, `version`, `updated_at`, `workspace`) and a `.tmgr-pages.json` index. Files are written atomically without following symlinks and only inside `<dir>`; pull never overwrites a locally modified file without `--force`; push sends only changed files and refuses files that were not pulled. A version conflict exits 6 and writes `<slug>.remote.md` next to the file.
- JSON schemas `today`, `run_list`, `run` (the `log --follow` event), `routine_list`, `routine_done`, `page_ref`, `page_sync`, `page_status`.
- `tmgr board [-p CODE] [--mine] [--ascii]`: full-screen kanban of the workspace statuses (hidden and archived types excluded). Keys: `h/l` column, `j/k` card, `H/L` move the card to the previous/next status (optimistic, rolled back on error), `s`/`S` start/stop the timer, `c` comment, `/` filter, `Enter` details (description, last 5 comments, agent work runs), `g` refresh, `?` help, `q`/Esc quit; mouse click selects, wheel moves. Refreshes every 30 s and after each action. Below 100 columns it shows one column with a status tab strip, below 60 a plain list grouped by status. Colours follow `COLORTERM`, `NO_COLOR` and `--no-color`. Needs a terminal (exit 2 otherwise). The board always shows statuses: the Workflow module flag is not readable with a CLI token, so there is no Open/Done mode yet.

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
