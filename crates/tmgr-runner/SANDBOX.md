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
env_pass = ["SOME_API_KEY"]                 # values come only from the [secrets] file; a missing value is logged
[env]
DISABLE_AUTOUPDATER = "1"                   # literal, non-secret
[secrets]
file = "~/.config/tmgr/agents/some.env"
[[secrets.files]]                           # owner-only credential file copied into the per-run home
from = "~/.config/tmgr/agents/claude-code.credentials.json"
to = ".claude/.credentials.json"
token_prefix = "sk-ant-oat01-"              # optional: what `tmgr agents login` requires of the token
template = "claude-oauth"                   # optional: "claude-oauth" or "raw" (default raw)
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
- `[[secrets.files]]`: `from` must be inside `~/.config/tmgr/agents/` (plain relative path below it, resolved under the
  real home). The runner, which is not sandboxed, opens it one component at a time with `O_NOFOLLOW`; it must be a regular
  file owned by the user, mode 0600 or stricter, at most 64 KiB, otherwise the session is refused with a `chmod 600`
  hint naming the file. A missing file is not an error: one log line is written (never contents) and the agent starts
  logged out. The copy goes to `to` (plain relative path) in the per-run home: file 0600, parent directories 0700,
  exclusive create, no-follow. The agent still cannot read `~/.config/tmgr`; per-run homes of other sessions are denied
  to each session, so only the session itself sees its copy.
- `tmgr agents login <profile> [--replace]` reads one token (hidden prompt on a terminal, one line from stdin otherwise),
  checks the profile's `token_prefix`, the charset `A-Za-z0-9_-` and a length of 20 to 400, renders the file's `template`
  and writes it atomically (0600 temp file + rename). An existing file is replaced only with `--replace`. The token is
  never printed or logged. `tmgr agents logout <profile>` removes that one declared file.

Claude Code on macOS keeps its login in the Keychain, which the sandbox does not reach; it falls back to
`$CLAUDE_CONFIG_DIR/.credentials.json` (default `~/.claude/.credentials.json` under `HOME`) when the Keychain read fails.
The `claude-code` profile therefore copies a credential file into the per-run home.

### Claude login

1. Run `claude setup-token` in a normal terminal; it prints a long-lived token starting with `sk-ant-oat01-`.
2. Run `tmgr agents login claude-code` and paste the token at the hidden prompt (or pipe it on stdin).
3. This writes `~/.config/tmgr/agents/claude-code.credentials.json` (0600, directory 0700) with the token, no refresh
   token and an expiry one year out. The next `claude-code` session finds it as `~/.claude/.credentials.json` and is
   logged in. Repeat with `--replace` to rotate; `tmgr agents logout claude-code` deletes it.

Who can read it: the owner and the runner; the session that received it (its copy lives in its own per-run home, which
other sessions are denied); not the sandboxed agent's other sessions and not anything under the real `~/.config/tmgr`.
The environment-variable route is intentionally not offered for Claude: on macOS the environment of a process can be read
by other processes of the same user, so a token passed through `env_pass` is visible to other sessions, while a file in the
per-run home is not. The generic `env_pass` and secrets-file engine stays for other profiles, but prefer credential files.

Codex keeps its login in `~/.codex/auth.json`, which its profile seeds; the owner's `config.toml` and
`~/.claude/settings.json` (hooks, MCP servers) are never seeded.

Codex rotates its refresh token when `last_refresh` in `auth.json` is about 8 days old, and a rotation inside a session
would leave the owner's original file with a token the server no longer accepts. A seed can therefore declare
`fresh = { json_key, max_age_days }`: the session is refused (with a hint to run the agent once in a normal terminal)
when that timestamp is missing, unreadable or older than the limit.

The session's copy is agent-controlled, so it is never written back by itself: an agent could keep `tokens.account_id`
and `auth_mode` and plant another account's tokens, and the owner's own Codex would then send future work to that
account. `write_back = true` (or `"confirm"`, the same) therefore only stores a candidate. After the session, if the copy
changed and is a JSON object of at most 1 MiB that keeps every top-level key of the source and has equal values at every
dotted path in `same` (the codex profile pins `tokens.account_id` and `auth_mode`), the runner stores it as
`<runner dir>/candidates/<profile>/<seed index>.json` (0600, directories 0700, with a `<seed index>.meta.json` holding the
source path, the seeded and current source hashes, the session id and the time); a newer candidate replaces an older one.
A copy is not stored if the source changed during the session and the copy is not strictly newer. The original is not touched.

`tmgr agents` and `tmgr run` print one line per pending candidate (the `candidates.list` method, no contents).
`tmgr agents sync <profile>` needs a terminal on stdin and stdout and refuses inside a session (`TMGR_SESSION_ID`). It shows a
summary without token values (source, age, session, whether each `same` path is equal, the names of the changed top-level keys,
the old and new `fresh` timestamps) and applies only after `apply` is typed. Applying takes the per-source `flock`, opens the
source's parent directory with `O_DIRECTORY | O_NOFOLLOW` one component at a time, re-reads the source, refuses if it changed
since the candidate was made and now holds a newer (or unreadable) `fresh` timestamp, writes `<name>.tmgr-backup` and the new file
(exclusive 0600 temp file, `fsync`, `renameat`) relative to that directory descriptor and checks with `fstatat(AT_SYMLINK_NOFOLLOW)`
before and after that the source name is a regular file. `tmgr agents sync <profile> --discard` drops candidates without applying.

Opt-in auto mode: `tmgr agents sync <profile> --auto on` prints the risk (an agent in a session can replace the login with
another account's tokens, which TMGR cannot verify offline) and needs `I AGREE` typed in a terminal (`--yes` never applies; refused
inside a session). It writes `<agents dir>/<profile>.autosync` (0600; the agents dir is sandbox-denied); while that private regular
file exists the runner applies each new candidate itself after the session with the same function and logs it. `--auto off`
removes the file. A profile TOML cannot enable auto mode.

Residual risk: in auto mode, or if the owner types `apply` without reading the summary, an agent can put another account's
tokens into the login (restore it from `auth.json.tmgr-backup`). Even in the default mode the owner's own `codex` may fail
while a session holds rotated tokens, until the candidate is applied.

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
| Network | allowed, or restricted to the egress proxy (below) | allowed; restriction not supported |

`/tmp` and `/var/folders` are not writable on either OS: tools must use `TMPDIR`. One exception on macOS: files starting with `xcrun_db` in the per-user temp dir (`getconf DARWIN_USER_TEMP_DIR`), the cache the `/usr/bin` Xcode shims (`python3`, `git`, …) write regardless of `TMPDIR`.

## Denied (read and write; plus unix-socket connect on macOS)

Under the real home: `.ssh .gnupg .aws .config/tmgr .tmgr.dev .netrc .docker/config.json .kube Library/Keychains
Library/Application Support/dev.tmgr.cli Library/Application Support/Google/Chrome
Library/Application Support/Firefox Library/Safari .mozilla .config/google-chrome .config/chromium
.local/share/keyrings .config/gh .git-credentials .config/git/credentials`; the runner directory; the homes directory (other sessions' homes), with only this session's
home re-allowed. The config directory each CLI reports on connect (`hello.config_dir`, validated: absolute, named
`tmgr`, never the home, runner or homes directory, at most 16) is added to this list for every later session, in
literal and resolved form (every symlink hop and the final target); the set only grows and is kept in `client-config-dirs.json` in the runner directory, so it survives a runner restart. On macOS these are explicit deny rules placed
after the allow rules; on Linux they are simply never granted.

## No-sandbox mode

`runs.create` with `no_sandbox` (manual CLI runs only) starts the agent as if it ran outside tmgr: real `HOME`, the caller's environment, no seeds, secret files or trust writes, no egress proxy. The per-run home stays a plain directory; it is the only directory the runner removes if the start fails.

macOS: the command is still wrapped in `sandbox-exec`, with `(allow default)` and only these denials: file writes, and unix-socket connects, under the runner directory (`r.sock`, session sockets), the homes directory, the tmgr config directory (both `$XDG_CONFIG_HOME/tmgr` and `~/.config/tmgr`, the runner's trust store and profile folder, each in literal and resolved form), `~/.tmgr.dev` (local workspaces and their access socket) and the profile-protected paths and the config directories CLIs reported on connect; writes to the `tmgr` executable; reads under the runner directory and the homes directory, so the agent cannot take the client secret or other runs' logs and logins. Other reads, signals, terminals and the network are not restricted. This is the real fence: without it the agent could talk to the runner or edit the trust store. Because the agent already runs inside this profile, it cannot start a sandbox of its own (`sandbox-exec` refuses to nest), so an agent's built-in macOS sandbox fails in this mode. A fenced folder that does not exist yet (for example `~/.config`) is not protected against being created as a symlink elsewhere; the agent already has full control of the real home, so this is not a new boundary.

Linux: the command runs directly, with no Landlock and no helper. There is no control-plane fence, because Landlock cannot express "write anywhere except this subtree". The `TMGR_RUN_*` environment check and the typed prompts are speed bumps, not boundaries. The agent can read the client secret, talk to the runner as the CLI, and edit its own worklog, so the audit fields are not tamper-proof there. Because the agent can act as the CLI, anything the CLI can answer or type into a session, including permission prompts shown as cards, can be answered by the agent itself; approvals in a no-sandbox Linux session are not proof that a person approved, so use the sandbox when that matters. On both systems the agent has the user's full powers otherwise: ssh keys, keychain, cloud credentials and the network.

## Network restriction

`SandboxSpec.network` is `Open` (default) or `Restricted { proxy_port }`. `sandbox::network_restriction_enforced()` is true
on macOS only.

macOS: after every other rule the profile appends `(deny network-outbound)`, then re-allows unix sockets under the run
home and `(remote tcp "localhost:<proxy_port>")`, then `(deny network-inbound)` and
`(allow network-inbound (local ip "localhost:*"))` (the last matching rule wins). Direct TCP, UDP and DNS lookups from the
agent fail; only the proxy port on localhost is reachable. The environment gets `HTTPS_PROXY HTTP_PROXY https_proxy
http_proxy ALL_PROXY all_proxy = http://127.0.0.1:<port>` and empty `NO_PROXY no_proxy` (a profile or `env_pass` cannot
override them). A restricted profile also denies `appleevent-send`, `lsopen` and `mach-lookup` of
`com.apple.coreservices.launchservicesd` and `com.apple.nsurlsessiond`, so the agent cannot hand a URL to the system
opener (`open`) or to the system URL-session daemon; `tests/sandbox_network.rs` checks that `open` fails.

Linux: Landlock filters TCP ports only (not hosts, not UDP), so the restriction is not enforced and `prepare` refuses a
`Restricted` spec; the mode policy never asks for it there, so unattended bypass runs are not possible on Linux.

Egress proxy (`src/egress.rs`, runs in the runner, outside the sandbox, one per restricted session, `127.0.0.1:0`,
stopped on drop): only `CONNECT <host>:443`; other methods get 405, other ports, IP literals and hosts outside the
allowlist get 403. The allowlist is the base set `github.com codeload.github.com objects.githubusercontent.com
raw.githubusercontent.com` plus the profile's `[network] allow = [...]` (exact names or `*.suffix`, which does not match
the bare suffix). The proxy resolves the name itself, refuses the whole host if any address is loopback, private,
link-local, CGNAT (100.64/10), multicast, unspecified, reserved/documentation, IPv6 ULA or link-local, or an IPv4-mapped,
NAT64 or 6to4 form of those, and connects to the exact address it checked. Requests are limited to 8 KiB of headers,
64 concurrent connections and a 5 minute idle timeout. Refused CONNECTs are logged by host name only.

Residual risks:
- Any allowed host that accepts uploads with credentials the agent holds is an exfiltration channel.
- Mach/XPC services other than the two denied above stay reachable on macOS because the profile starts from
  `allow default`; a service that does network work on the agent's behalf is a theoretical channel.
- CI secrets on same-repo PR branches: a published `tmgr/**` branch is in the owner's repository, so its existing
  workflows run the agent-modified build and test scripts with the repository's secrets. Publishing refuses changes under
  `.github/`, but not changes to the scripts those workflows call. Use environment-scoped secrets, or require approval
  before workflows run on `tmgr/**` branches.

## Linux helper

`prepare` writes the policy to `<runner dir>/sessions/<id>/sandbox.json` (0600, outside everything the sandbox can
reach) and returns `<self exe> __sandbox-exec <policy> -- <argv>`. The binary's `__sandbox-exec` subcommand must call
`tmgr_runner::sandbox::exec_helper(<policy> -- <argv>)`. The helper checks the Landlock ABI (below 6 or disabled: message
and exit 78), builds the ruleset with hard requirements, restricts itself, verifies full enforcement and execs the
command (exec failure: exit 127).

### Read-only mount namespace (Linux)

Landlock cannot restrict `chmod`, `chown`, `utime` or `setxattr`, so on Linux the helper also makes everything outside
the writable roots read-only at the mount level:

- Before Landlock is applied, the helper creates a user and mount namespace (identity uid/gid maps; files owned by
  other users, root included, show as `nobody`). It marks every mount read-only in one recursive `mount_setattr` call,
  which keeps each mount's locked `nosuid`/`nodev`/`noexec`/atime flags. It then bind-mounts each writable root (the
  working directory, the per-run home with its `tmp`, a team member's own folder) onto itself and remounts that bind
  read-write, checking the device and inode before and after, and changes into the working directory again so the
  process works on the new mount. Submounts below a writable root stay read-only.
- A legacy clone that borrows objects through `objects/info/alternates` gets the source repository's object directory as
  a read-only grant; the helper also bind-mounts it onto itself read-only, in case it lies inside a writable root.
- Finally the helper reads `/proc/self/mountinfo` and refuses to continue if any mount other than a writable root is
  still writable. Changing modes, owners, times or extended attributes outside the writable roots then fails with
  `EROFS`. Device nodes (`/dev/null`, the session's terminal, `/dev/ptmx`) and connecting to existing sockets keep
  working, because a read-only mount does not block them. The mounts are private to the session; the host's view is
  unchanged.
- The device and inode of every writable root and object directory are checked before the namespace is tried; a
  mismatch stops the run (exit 78).
- The whole sequence is first tried in a forked probe process. If it cannot work, the helper continues without the
  namespace: unprivileged user namespaces disabled (`kernel.unprivileged_userns_clone=0`, `user.max_user_namespaces=0`),
  restricted by the distribution (Ubuntu 24.04 `kernel.apparmor_restrict_unprivileged_userns=1`), blocked by a container
  seccomp profile (Docker's default), or a multithreaded helper process. Only a refusal to create the namespace or to
  make `/` read-only (`EPERM`, `EACCES`, `EINVAL`, `ENOSYS`, or `ENOSPC` with `user.max_user_namespaces=0`) leads to
  that fallback. Any other failure, in the probe or afterwards, stops the run (exit 78): a probe that cannot fork, a
  namespace quota that is used up, a failure while re-binding the writable roots or in the final check.
- The helper records the outcome in `<runner dir>/sessions/<id>/objects_ro` (`namespace`, `fallback: <reason>` or
  `failed: <reason>`), and the
  daemon logs it when the run ends.
- Independently of the namespace, the daemon records the mode of every directory and file under each granted object
  directory before the run (`objects-modes` in the session directory, outside the sandbox) and, after the run ends
  (also after a daemon restart, or when the session is removed), restores any mode that changed. Directories are
  restored before their contents, symlinks are never followed, and entries that did not exist at the start or whose inode
  changed are left alone. Owner, modification times and extended attributes are not restored, and the restore runs when
  the runner sees the session exit, so a process that outlives the session could change modes again afterwards.

## Known limitations

- Linux metadata changes without a user namespace: Landlock has no right for `chmod`, `chown`, `utime` or `setxattr`, so
  when the read-only mount namespace cannot be created (see above) the agent can change the mode, owner (within what the
  user may do), times and extended attributes of any file the user owns, by absolute path, whether or not the path is
  granted. Granted object directories then get only the mode restore described above (owner, times and extended
  attributes are not restored). macOS Seatbelt refuses these changes outside the writable roots.
- Linux file descriptors the helper inherits from the runner keep pointing at the host's mounts, so metadata changes
  through them (`fchmod`, `fchown`, `futimens`, `fsetxattr`) are not refused by the read-only mounts.
- Network is open unless the run is `Restricted` (macOS only, see above); an open network is not data-exfiltration protection for anything the agent can read.
- The agent can use everything it is allowed to touch: its working dir, per-run home, the network, its own CLI
  credentials copied into the per-run home.
- Linux, kernels with Landlock ABI < 9 (for example 6.17 is ABI 7): pathname unix sockets elsewhere (user ssh-agent,
  D-Bus, gpg-agent under `/run/user/<uid>`) stay reachable if the agent guesses the path; only the environment hints
  are removed. Abstract sockets are blocked entirely, including the runner's own per-session socket if it is abstract.
- Linux does not restrict `/proc/<pid>` metadata and `/sys` reads beyond ptrace rules; `/proc/<pid>/environ` of a process
  in another Landlock domain (another session) or outside any domain is unreadable because Landlock denies ptrace-mode
  access to non-descendant domains; `/etc` is readable.
- Reads outside the real home differ by platform, and this is intended. macOS: everything outside the real home is
  readable, including other repositories, other task copies and team sibling copies that live outside the home, and
  other volumes; inside the real home only the working dir, the per-run home and toolchains are readable. Linux: only
  the system roots listed above, the working dir, the per-run home and toolchains are readable; other repositories and
  sibling copies are not. On macOS, do not keep confidential material outside the home if agents must not see it. The
  test `sibling_copy_outside_home_is_readable_on_macos_only` pins this.
- macOS: process listing and
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
- The Linux sandbox tests skip when Landlock is unavailable, unless `TMGR_TEST_REQUIRE_LANDLOCK=1` is set, in which case they fail. CI sets it on Linux. `TMGR_TEST_REQUIRE_RO_MOUNT=1` likewise makes the tests for the read-only mount namespace fail instead of reporting that only the mode restore was exercised. CI sets it on both Linux jobs (x86_64 and aarch64) and turns off `kernel.apparmor_restrict_unprivileged_userns` there first, because the Ubuntu 24.04 runners restrict unprivileged user namespaces by default.
- Linux helper: every read/write root is re-opened and its kernel-resolved path must equal the policy path, and the working directory (handed over by the shim as a descriptor, not looked up by path) must be the directory with the
  `st_dev`/`st_ino` the runner approved, and the Landlock rule for it is created from that same descriptor; otherwise the run does not start (exit 78). This closes the swap-for-a-symlink race between validation and enforcement.
- macOS: the agent also starts in the approved directory object, but Seatbelt rules are path based, so if that directory was renamed meanwhile the agent keeps no write access there (fail closed).
- Queue clones: the object directory of `clone_of` is re-checked by descriptor identity before the policy is built and again before the spawn; on Linux the helper also verifies the opened root's device and inode. macOS Seatbelt grants are path based, so a swap after the second check is the residual window.
- The `tmgr` binary (`self_exe`) must not live inside the working dir or the per-run home; the run is refused otherwise.
- Windows: not supported; `prepare` returns an error.

## Worktree setup git (not yet wired)

`src/worktree.rs` runs git outside the sandbox with hooks disabled, a cleared environment, system/global config ignored, repository config keys that run programs refused, and every configured filter neutralized. Known gap: a process that can write the repository's `.git/config` between the inspection and the checkout could add a new filter. A session that starts in a session clone can write only the clone's own `.git`, never the owner's repository. Sandboxed sessions started in a git repository use a session clone by default (`attach.worktree`, `--no-worktree`); a session started in place (explicit opt-out, `--no-sandbox`, or a parent folder that is not a repository but contains nested ones) can still write the `.git` of every repository below its folder, and changes it makes there (configuration, hooks) take effect when git is later used on that repository.

The clone is made with a fresh configuration, so the owner's filter settings are not copied: files that a Git LFS attribute names are checked out as the pointer files stored in the repository, and submodules are not populated (the folder is empty). The clone's objects are its own (no alternates), so `tmgr runner import` copies them as data and runs no git inside the clone.
