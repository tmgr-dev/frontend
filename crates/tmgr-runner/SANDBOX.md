# Runner sandbox

Every session runs inside an OS sandbox applied before the agent starts. If the sandbox cannot be applied the run
does not start (no `--no-sandbox`). Implementation: `src/sandbox.rs`.

## Environment

Starts empty. Copied from the runner's environment: `PATH USER LOGNAME LANG LC_* SHELL COLORTERM TERM_PROGRAM`.
Names in the caller's `env_pass` are added, except names containing `TOKEN SECRET KEY PASSWORD`, `AWS_* GH_*`,
loader/injection variables (`LD_* DYLD_* NODE_OPTIONS NODE_PATH PYTHON* PERL5* RUBY* BASH_ENV ENV PROMPT_COMMAND`),
git config/exec injection (`GIT_CONFIG* GIT_SSH* GIT_DIR GIT_WORK_TREE GIT_ASKPASS GIT_EXEC_PATH ...`),
`TMGR_*`, `XDG_*`, `SSH_AUTH_SOCK`, `DBUS_SESSION_BUS_ADDRESS`. A profile's own `env_pass` is an explicit opt-in: it may
name `*TOKEN*`/`*SECRET*`/`*KEY*`/`*PASSWORD*`/`AWS_*`/`GH_*` variables, but the loader, git-injection, `TMGR_*`, `XDG_*`
and similar names stay refused (the profile is rejected at load). The profile's `[env]` table sets literal values
(same name rules). Then set: `HOME` (per-run home), `TERM=xterm-256color`,
`TMGR_SESSION_ID`, `XDG_CONFIG_HOME XDG_CACHE_HOME XDG_DATA_HOME` (inside the home), `TMPDIR=<home>/tmp`.

## Profile-declared agent access

An agent profile (trusted local data, see the design doc 6.10) may declare, all relative to the real home as `~/...`:

```toml
env_pass = ["CLAUDE_CODE_OAUTH_TOKEN"]      # values come only from the [secrets] file; a missing value is logged
[env]
DISABLE_AUTOUPDATER = "1"                   # literal, non-secret
[secrets]
file = "~/.config/tmgr/agents/claude-code.env"
[sandbox]
read = ["~/.local/share/claude"]            # extra read-only roots (also executable)
[[sandbox.seed]]
from = "~/.claude.json"                     # copied into the per-run home at session start
to = ".claude.json"
json_keys = ["hasCompletedOnboarding", "theme", "oauthAccount"]   # optional: only these top-level keys
```

Rules, checked when the profile loads and again at session start (a violation drops the profile / refuses the session):
- every path must resolve under the real home and must not equal, contain or sit inside a deny-list path, the runner
  directory or the homes directory. The deny list always wins: nothing under `~/.ssh`, `~/.gnupg`, `~/Library/Keychains`,
  `~/.config/tmgr` and the rest of the list below can be granted or seeded. The Keychain is never opened.
- `read` paths that do not exist are skipped with a log line. They become read-only roots (macOS: `file-read-data`
  allow before the deny rules; Linux: Landlock read+execute rule).
- a seed source must be a regular file of at most 1 MiB. The runner opens it from the real home one component at a time
  (`openat` with `O_NOFOLLOW`), so a symlink in the final component or in any parent is refused, also if it appears after
  the profile was validated; a
  missing source is skipped. `to` is a plain relative path (no `..`). With `json_keys` the source must be a JSON object and
  only those keys are written (compact JSON). Seeds are written 0600 into the per-run home.
- the directory the agent profiles are loaded from (`TMGR_AGENTS_DIR`, `$XDG_CONFIG_HOME/tmgr/agents` or the default) and
  the directory of each profile's secrets file are protected like `~/.config/tmgr`: a session working directory that
  contains or sits inside one is refused, and every session's sandbox denies them. The configured path counts as well
  as its destination: every symlink you own on the way (a `TMGR_AGENTS_DIR` that is a symlink, a symlinked secrets
  directory) is protected under its own name, so a session cannot sit where it could be replaced. The secrets locations
  come from every profile file, also from one that is rejected for an invalid read root or seed, so rejecting a profile
  never lifts the protection of its secrets. If a profile file cannot be parsed the inventory cannot be built and no
  session starts until it is fixed. Read grants must stay under the real home; the Linux helper re-opens each root and
  refuses it if its path changed since validation.
- implicit read grants (the toolchain directories below and, on Linux, the system roots) that contain or overlap a
  protected path are dropped and logged, because a read grant also covers everything below it; an explicit profile read
  path that overlaps one is refused. The Linux helper gets the protected paths too and refuses to start if a granted
  root contains one.
- the secrets file is read by the runner, never by the sandbox (it lives under the denied `~/.config/tmgr`). It must be a
  regular file owned by the user, mode 0600 or stricter, not a symlink, at most 64 KiB, `NAME=VALUE` lines with `#`
  comments; empty values are ignored. Otherwise the session is refused with a `chmod 600` hint. Values are never logged,
  and `launch.json` (which holds the session environment) is deleted by the shim as soon as it has read it.
- `tmgr agents login-file <profile>` creates that file (dirs 0700, file 0600, `NAME=` lines) if missing and never
  touches an existing one.

Claude Code on macOS keeps its login in the Keychain, which the sandbox does not reach. Log in once with
`claude setup-token`, put the token after `CLAUDE_CODE_OAUTH_TOKEN=` in the file above (`tmgr agents login-file claude-code`
creates it). Codex keeps its login in `~/.codex/auth.json`, which its profile seeds; the owner's `config.toml` and
`~/.claude/settings.json` (hooks, MCP servers) are never seeded.

## Refused working directories

`/`, the real home, any ancestor of the real home, anything equal to, containing or inside a deny-list path, the
runner directory or the homes directory (the parent of the per-run home). A per-run home inside the runner directory
is refused. Per-run homes must live in a dedicated directory `<homes>/<session id>`.

## Allowed

| | macOS (Seatbelt, `sandbox-exec -p`) | Linux (Landlock, needs ABI >= 6) |
|---|---|---|
| Read | everything except the real home; inside the real home only the working dir, the per-run home and existing toolchain dirs | `/usr /bin /sbin /lib* /etc /opt /nix /snap /proc /sys /run/systemd/resolve`, the working dir, the per-run home, toolchains |
| Toolchains (read-only) | `.cargo/bin .cargo/registry .cargo/git .rustup .nvm .volta .bun .deno .local/bin .local/share/mise .local/share/pnpm .sdkman .pyenv .rbenv .npm` under the real home, when they exist | same |
| Write | working dir, per-run home (includes the per-run `tmp`), `/dev/null /dev/tty /dev/ptmx /dev/fd` | working dir, per-run home, `/dev/null zero urandom random tty ptmx full`, plus the session's own `/dev/pts/N` (resolved from fd 0) |
| Execute | everywhere readable | the read roots above and the working dir / home |
| Signals | own sandbox only (`deny signal` + `allow signal (target same-sandbox)`) | own domain only (Landlock `Scope::Signal`) |
| Abstract unix sockets | not applicable | connecting to sockets created outside the domain is blocked (`Scope::AbstractUnixSocket`) |
| Pathname unix sockets | connect denied under the deny list and the runner directory | blocked only on ABI >= 9 (`ResolveUnix`) |
| Network | allowed | allowed |

`/tmp` and `/var/folders` are not writable on either OS: tools must use `TMPDIR`. One exception on macOS: files starting with `xcrun_db` in the per-user temp dir (`getconf DARWIN_USER_TEMP_DIR`), the cache the `/usr/bin` Xcode shims (`python3`, `git`, …) write regardless of `TMPDIR`.

## Denied (read and write; plus unix-socket connect on macOS)

Under the real home: `.ssh .gnupg .aws .config/tmgr .tmgr.dev .netrc .docker/config.json .kube Library/Keychains
Library/Application Support/dev.tmgr.cli Library/Application Support/Google/Chrome
Library/Application Support/Firefox Library/Safari .mozilla .config/google-chrome .config/chromium
.local/share/keyrings`; the runner directory; the homes directory (other sessions' homes), with only this session's
home re-allowed. On macOS these are explicit deny rules placed after the allow rules; on Linux they are simply never
granted.

## Linux helper

`prepare` writes the policy to `<runner dir>/sessions/<id>/sandbox.json` (0600, outside everything the sandbox can
reach) and returns `<self exe> __sandbox-exec <policy> -- <argv>`. The binary's `__sandbox-exec` subcommand must call
`tmgr_runner::sandbox::exec_helper(<policy> -- <argv>)`. The helper checks the Landlock ABI (below 6 or disabled: message
and exit 78), builds the ruleset with hard requirements, restricts itself, verifies full enforcement and execs the
command (exec failure: exit 127).

## Known limitations

- Network is open; this is not data-exfiltration protection for anything the agent can read.
- The agent can use everything it is allowed to touch: its working dir, per-run home, the network, its own CLI
  credentials copied into the per-run home.
- Linux, kernels with Landlock ABI < 9 (for example 6.17 is ABI 7): pathname unix sockets elsewhere (user ssh-agent,
  D-Bus, gpg-agent under `/run/user/<uid>`) stay reachable if the agent guesses the path; only the environment hints
  are removed. Abstract sockets are blocked entirely, including the runner's own per-session socket if it is abstract.
- Linux does not restrict `/proc/<pid>` metadata and `/sys` reads beyond ptrace rules; `/proc/<pid>/environ` of a process
  in another Landlock domain (another session) or outside any domain is unreadable because Landlock denies ptrace-mode
  access to non-descendant domains; `/etc` is readable.
- macOS: reads outside the real home are not restricted (system files, other volumes); process listing and
  `sysctl` process info of other processes are not blocked. In particular `sysctl(KERN_PROCARGS2)` returns the
  environment of any same-user process that lacks the `restrict` code-signing flag, so a session can read the
  environment (including profile-granted tokens) of another running session or runner-launched agent. Seatbelt
  cannot gate this call: tested on macOS 26 with `sysctl-read` name and prefix filters, `process-info*`, `sysctl*`
  and a default-deny profile, none of which stop it, and `syscall-number` filters have no effect. Hardened runtime
  does not help; only a `restrict` signature does. Treat a token passed to one macOS session as readable by every
  other session of the same user. The same call also reaches the user's other non-`restrict`-signed processes (shells,
  editors, `tmgr attach`), so anything exported there is readable from a session. The runner and its shims start
  from an allowlisted environment (`HOME PATH USER LOGNAME SHELL LANG LC_* TMPDIR COLORTERM TERM_PROGRAM XDG_* TMGR_*`)
  so they do not carry the starting shell's secrets; Mach/XPC services are reachable (default allow);
  Seatbelt is deprecated by Apple but still enforced.
- macOS tools that cache under `/var/folders` (clang module caches and similar) fail to write there; only the `xcrun_db` cache is allowed.
- Git worktrees whose `.git` points outside the working dir cannot write the shared object store; the caller must put
  such a repository inside the working dir.
- Toolchain directories are read-only: `cargo fetch`, `npm install -g` and similar that write there fail.
- Terminals: the agent reaches its own PTY through inherited fds 0/1/2 and `/dev/tty`. Opening any other terminal device by path is denied (macOS: explicit deny on `/dev/ttys*`, `/dev/pty[p-w]*` and `/dev/tty[p-w]*`; Linux: no `/dev/pts` grant except the session's own slave). Consequence: PTYs the agent creates itself (`openpty`, `script`, `tmux`, pexpect) cannot open their slave by path and fail.
- Private directories: the runner directory is trusted only if it, its canonical ancestors and every symlink component of the given path live in directories owned by root or the user that are not group/other-writable (unless sticky); there is no group-name exception, so on Ubuntu with umask 002 the fallback `~/.local/state/tmgr/run` is refused until `chmod g-w` or `TMGR_RUNNER_DIR` is set. The homes directory holds no secrets, so it gets only the final-directory checks (owner, 0700, not a symlink).
- Linux helper: every read/write root is re-opened and its kernel-resolved path must equal the policy path, and the working directory (handed over by the shim as a descriptor, not looked up by path) must be the directory with the
  `st_dev`/`st_ino` the runner approved, and the Landlock rule for it is created from that same descriptor; otherwise the run does not start (exit 78). This closes the swap-for-a-symlink race between validation and enforcement.
- macOS: the agent also starts in the approved directory object, but Seatbelt rules are path based, so if that directory was renamed meanwhile the agent keeps no write access there (fail closed).
- The `tmgr` binary (`self_exe`) must not live inside the working dir or the per-run home; the run is refused otherwise.
- Windows: not supported; `prepare` returns an error.
