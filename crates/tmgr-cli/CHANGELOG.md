# Changelog

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
