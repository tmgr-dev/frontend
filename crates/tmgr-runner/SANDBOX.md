# Runner sandbox

Every session runs inside an OS sandbox applied before the agent starts. If the sandbox cannot be applied the run
does not start (no `--no-sandbox`). Implementation: `src/sandbox.rs`.

## Environment

Starts empty. Copied from the runner's environment: `PATH USER LOGNAME LANG LC_* SHELL COLORTERM TERM_PROGRAM`.
Names in the profile's `env_pass` are added, except names containing `TOKEN SECRET KEY PASSWORD`, `AWS_* GH_*`,
loader/injection variables (`LD_* DYLD_* NODE_OPTIONS NODE_PATH PYTHON* PERL5* RUBY* BASH_ENV ENV PROMPT_COMMAND`),
git config/exec injection (`GIT_CONFIG* GIT_SSH* GIT_DIR GIT_WORK_TREE GIT_ASKPASS GIT_EXEC_PATH ...`),
`TMGR_*`, `XDG_*`, `SSH_AUTH_SOCK`, `DBUS_SESSION_BUS_ADDRESS`. Then set: `HOME` (per-run home), `TERM=xterm-256color`,
`TMGR_SESSION_ID`, `XDG_CONFIG_HOME XDG_CACHE_HOME XDG_DATA_HOME` (inside the home), `TMPDIR=<home>/tmp`.

## Refused working directories

`/`, the real home, any ancestor of the real home, anything equal to, containing or inside a deny-list path, the
runner directory or the homes directory (the parent of the per-run home). A per-run home inside the runner directory
is refused. Per-run homes must live in a dedicated directory `<homes>/<session id>`.

## Allowed

| | macOS (Seatbelt, `sandbox-exec -p`) | Linux (Landlock, needs ABI >= 6) |
|---|---|---|
| Read | everything except the real home; inside the real home only the working dir, the per-run home and existing toolchain dirs | `/usr /bin /sbin /lib* /etc /opt /nix /snap /proc /sys /run/systemd/resolve`, the working dir, the per-run home, toolchains |
| Toolchains (read-only) | `.cargo .rustup .nvm .volta .bun .deno .local/bin .local/share/mise .local/share/pnpm .sdkman .pyenv .rbenv .npm` under the real home, when they exist | same |
| Write | working dir, per-run home (includes the per-run `tmp`), `/dev/null /dev/tty /dev/ttys* /dev/ptmx /dev/fd` | working dir, per-run home, `/dev/null zero urandom random tty ptmx pts full` |
| Execute | everywhere readable | the read roots above and the working dir / home |
| Signals | own sandbox only (`deny signal` + `allow signal (target same-sandbox)`) | own domain only (Landlock `Scope::Signal`) |
| Abstract unix sockets | not applicable | connecting to sockets created outside the domain is blocked (`Scope::AbstractUnixSocket`) |
| Pathname unix sockets | connect denied under the deny list and the runner directory | blocked only on ABI >= 9 (`ResolveUnix`) |
| Network | allowed | allowed |

`/tmp` and `/var/folders` are not writable on either OS: tools must use `TMPDIR`.

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
- Linux does not restrict `/proc/<pid>` metadata and `/sys` reads beyond ptrace rules; `/etc` is readable.
- macOS: reads outside the real home are not restricted (system files, other volumes); process listing and
  `sysctl` process info of other processes are not blocked; Mach/XPC services are reachable (default allow);
  Seatbelt is deprecated by Apple but still enforced.
- macOS tools that cache under `/var/folders` (some Xcode shims, clang module caches) fail to write there.
- Git worktrees whose `.git` points outside the working dir cannot write the shared object store; the caller must put
  such a repository inside the working dir.
- Toolchain directories are read-only: `cargo fetch`, `npm install -g` and similar that write there fail.
- Windows: not supported; `prepare` returns an error.
