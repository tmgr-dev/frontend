# TMGR runner protocol 1

Protocol 1 — stable from 2026-10-13; changes are additive and announced.

This is the local IPC between the TMGR runner (the process that owns agent
terminal sessions) and its clients: the `tmgr` CLI, the desktop app and later
the leader-to-follower link. The Rust types live in `tmgr_ipc::api`; the golden
JSON files in `tests/fixtures/` are the executable form of this document.

## 1. Transport

One Unix stream socket per user, no TCP.

- macOS: pathname socket `r.sock` in the runner directory
  (`~/Library/Application Support/dev.tmgr.cli/run`, override `TMGR_RUNNER_DIR`).
- Linux: abstract socket. The runner directory (`$XDG_RUNTIME_DIR/tmgr`,
  override `TMGR_RUNNER_DIR`) holds `runner.addr`, a text file with the abstract
  name (`tmgr-<uid>-<hex>`). Connect to `"\0" + name`.
- The runner directory is mode 0700 and owned by the user. Do not follow
  symlinks when you open files in it.
- The runner checks the peer uid on every accept (`SO_PEERCRED` / `LOCAL_PEERCRED`)
  and drops other users. A client should check the server's uid too.
- The shared secret is the file `client.secret` in the runner directory: 64 hex
  characters, mode 0600, owned by the user. Trim whitespace; the key is the
  resulting text as bytes (not the decoded hex).

## 2. Framing

Every message is `u32 length (big endian) | u8 kind | payload`. `length` counts
the kind byte and the payload.

| kind | payload | limit |
|---|---|---|
| 0 | UTF-8 JSON object | 1 MiB |
| 1 | binary: `u8 tag`, `u8 sid_len`, `sid` (ASCII session id, may be empty), data | 4 MiB |

Clients use one binary tag: `3` = terminal input for session `sid` (data is raw
bytes for the agent's stdin, at most 64 KiB per frame; split longer input).
Tags 1 and 2 are runner-to-shim only. The runner never sends binary frames to
clients. Anything else, an unknown kind or an oversized frame closes the
connection.

Session ids look like `s_` plus 10 characters from `a-z2-7`.

## 3. Handshake

All handshake messages are JSON frames with a `type` field. Nonces are 32
random bytes as 64 lowercase hex characters.

1. C to S `hello`: `{"type":"hello","proto":1,"client":"desktop","version":"0.3.0","pid":4242,"nonce_c":"<hex>"}`.
   `client` is `cli`, `desktop`, `worker` or `hook` (self-declared, see 6).
   `hello` may carry an optional `"config_dir"`: the absolute path of the client's tmgr config directory. The runner
   validates it (absolute, no `.`/`..`, named `tmgr`, not the home, runner or homes directory, at most 16 distinct)
   and adds it to the sandbox deny list of every later session; the set is add-only and an invalid value is ignored,
   never a handshake error. Older runners ignore the field.
2. S checks `proto`. Different major: S sends `error` with `proto_mismatch` and
   closes. S replies `challenge`:
   `{"type":"challenge","nonce_s":"<hex>","proof_s":"<hex>"}` where
   `proof_s = HMAC(nonce_c, nonce_s, label "tmgr-runner-server")`.
3. C verifies `proof_s` (constant time). Failure means a stale or foreign socket:
   disconnect and do not send anything else. C replies
   `{"type":"auth","proof_c":"<hex>"}` with
   `proof_c = HMAC(nonce_s, nonce_c, label "tmgr-runner-client")`.
4. S verifies `proof_c`. Failure: `error` with `unauthorized`, close. Success:
   `{"type":"welcome","proto":1,"runner_version":"0.3.0","caps":[...]}`.

`HMAC(a, b, label)` = lowercase hex of HMAC-SHA256 with the secret text as key
over `label || 0x00 || a || b` (the two nonces as their 64-character hex text,
concatenated without separator).

Test vector: key text is the 16 characters `0123456789abcdef` four times; the
nonces are 32 bytes of `0xaa` (client) and `0xbb` (server), written as hex:

- `proof_s` = `a490348a2f8a10158c9911d731984007d11ac37d6c5f900e3d6de242b09156fb`
- `proof_c` = `9067d3e30032ebc623682f4022e80aee006ecc41efae880905b3e3e34eac0bbb`

Both sides must finish within 5 seconds. A refusal is
`{"type":"error","code":"proto_mismatch","message":"..."}`.

```json
{"type":"hello","proto":1,"client":"desktop","version":"0.3.0","pid":4242,"nonce_c":"0011...eeff"}
{"type":"welcome","proto":1,"runner_version":"0.3.0","caps":["sessions.v1","attach.v1","runs.create.v1","task_ref.v1","layout.v1","run_bundle.v1","cwd_pin.v1","link_origin.v1","candidates.v1","worklog.v1","runs.restore.v1","permission_mode.v1","start_key.v1","run_status.v1","clone_dir.v1","session_text.v1","trust_cwd.v1","agent_work.v1","exe_pin.v1","argv_pin.v1"]}
```

## 4. Versions and capabilities

- `proto` is the major version, currently `1`. A mismatch is always an error and a close.
- Additive changes keep the major: new optional fields, new methods, new push
  `type`s. Each comes with a capability string in `welcome.caps`; check it before
  you use the feature.
- Clients must ignore unknown JSON fields and unknown push `type`s (the Rust
  type decodes them as `ServerMsg::Unknown`). Do not use `deny_unknown_fields`.
- Servers answer an unknown `method` with `unknown_method`.
- An unknown error `code` is an unknown failure (`ErrorCode::Other`).

| capability | meaning |
|---|---|
| `sessions.v1` | `runner.*`, `sessions.*` |
| `attach.v1` | `attach.open`, `attach.set`, `Screen` and `Event` pushes, stream messages |
| `runs.create.v1` | `runs.create` |
| `task_ref.v1` | `task` on create params, `SessionInfo` and `Event` |
| `layout.v1` | `layout.get`, `layout.set` |
| `run_bundle.v1` | `bundle`, `bundle_sha256` on `runs.create`; `bundle_sha256` on `SessionInfo` (section 5, "Run bundles") |
| `cwd_pin.v1` | `cwd_pin` on `runs.create` (section 5, "Pinning the folder") |
| `link_origin.v1` | `origin` on `runs.create`, `SessionInfo` and `Event`; `origin.revoke` (section 5, "Link-originated runs") |
| `candidates.v1` | `candidates.list` (section 5, "Refreshed logins") |
| `worklog.v1` | `sessions.worklog` (section 5, "Work log") |
| `runs.restore.v1` | `runs.restore` (section 5, "Restoring interrupted runs") |
| `sessions_resume.v1` | `sessions.resume`; `resumed_from` on `SessionInfo`; `via` in the result (section 5, "Resuming an exited session") |
| `sessions_fork.v1` | `sessions.fork` (section 5, "Forking a Claude Code session") |
| `manual_bypass.v1` | `permission_mode: "bypass"` without a cloud approval for a manual CLI run (section 5, "Queue capabilities") |
| `no_sandbox.v1` | `no_sandbox` on `runs.create` and `SessionInfo` (section 5, "Runs without the sandbox") |
| `external_resume.v1` | `external_resume` on `runs.create`, `permission_mode` and `no_sandbox` on `sessions.resume`, `agent_session_id` on `SessionInfo` (section 5, "External resume") |
| `checkpoints.v1` | `checkpoints.*`; `checkpoints` on `SessionInfo` (section 5, "Checkpoints") |
| `permission_cards.v1` | `permission_cards` on `runs.create` and `SessionInfo` (section 5, "Permission cards") |
| `permission_mode.v1` | `permission_mode` on `runs.create`, `SessionInfo` and `launch` of a bundle (section 5, "Queue capabilities") |
| `start_key.v1` | `start_key` on `runs.create`; `runs.lookup` (section 5, "Queue capabilities") |
| `run_status.v1` | `runs.status` (section 5, "Queue capabilities") |
| `clone_dir.v1` | `clone_of` on `runs.create` (section 5, "Queue capabilities") |
| `multi_clone.v1` | `clones` on `runs.create`; `repos_sha256` in `cloud` (section 5, "Queue capabilities") |
| `signed_context.v1` | `persona.id`, `persona.instructions` and `context` in a bundle; `persona_sha256`, `context_sha256` in `cloud` (section 5, "Run bundles") |
| `team.v1` | `team` in a run bundle (section 5, "Run bundles") |
| `team.workers.v1` | `team` on `runs.create` and `SessionInfo`; `assignment` in a run bundle (section 5, "Run bundles") |
| `team.multi.v1` | `team` together with `clones` on `runs.create`: the run folder is `<primary>-worktrees/queue/<pickup id>/lead` or `.../wK` and holds one clone per repository |
| `profiles_login_check.v1` | `profiles.login_check` |
| `chat.v1` | `chat.rooms`, `chat.history`, `chat.send`, `chat.search`, `chat.subscribe`, `chat.relay_self` |
| `chat.mention.v1` | `chat.mention_inject`; `SendParams.mention`, `RoomInfo.mention`, `ChatMessage.mention` (`cli` and `desktop` only) |
| `manager.v1` | `manager` on `runs.create` and `SessionInfo` (section 5, "Manager session") |
| `run_prompt.v1` | `prompt` on `runs.create` |
| `spawnauth.v1` | method `spawn.challenge` (CLI and desktop only) |
| `spawn_decide.v1` | method `spawn.decide` (desktop only) |
| `manager.unrestricted.v1` | method `manager.unrestricted` (CLI only, follower PIN; section 5, "Unrestricted manager") |
| `manager.remote_policy.v1` | methods `manager.remote_policy` (read) and `manager.remote_policy_set {leader, ask, pin?}`; `cli` and `desktop` only, `_set` is refused unless the caller's parent chain reaches init without passing a live session, and `ask: true` needs `cli` plus the follower PIN (same throttle and tries as `manager.unrestricted`) |
| `chat.blob.v1` | method `chat.blob.put` (`cli` and `desktop` only; section 5, "Chat") |
| `chat.activity.v1` | `{"activity": true}` on `chat.subscribe`; push `chat_activity` (section 5, "Chat activity") |
| `orchestrator.v1` | `orchestrator.create` (section 5, "Orchestrator session") |
| `session_text.v1` | `sessions.text` (section 5, "Session text") |
| `trust_cwd.v1` | `trust_cwd` on `runs.create`; `trust` in `WorkSummary` (section 5, "Pre-accepted folder trust") |
| `agent_work.v1` | `runs.create` `agent_work` (section 5, "Queue capabilities") |
| `exe_pin.v1` | `exe_pin` on `runs.create` (section 5, "Pinned executable") |
| `argv_pin.v1` | `argv_sha256` on `runs.create` (section 5, "Pinned executable") |
| `trusted_folders.v1` | `trust.list`, `trust.add`, `trust.remove` (section 5, "Trusted folders") |
| `object_exempt.v1` | `objects.list`, `objects.allow`, `objects.deny`: folders exempt from the shared git object check at start (`.object-exempt.json` in the homes directory; the runner skips only that check for a run whose opened `cwd` has the same canonical path, `st_dev`/`st_ino`, filesystem id and creation time (`birth_ns`, 0 when the filesystem reports none)) |
| `trust_via_repo.v1` | `trust_via_repo` on `runs.create` (section 5, "Trusted folders") |
| `human_time.v1` | `attention` stream message; `human_s` on `SessionInfo` (section 7, "Human time") |
| `autopilot.v1` | `autopilot.start`, `autopilot.status`, `autopilot.control`, `autopilot.log`; `reason` on `sessions.kill` (section 5, "Autopilot") |

## 5. Messages after the handshake

Client to server (JSON, `type` tag): `request`, `resize`, `scroll`, `focus`,
`attention`, `detach`. Server to client: `response`, `screen`, `event`, `closed`.

Example: `{"type":"response","id":8,"err":{"code":"unknown_profile","message":"..."}}`.

`id` is chosen by the client (u64) and echoed. Exactly one of `ok` / `err` is
present; `ok` may be `null`. A client may keep several requests in flight; pushes
can arrive between a request and its response. After `attach.open` the same
connection keeps accepting requests.

### Methods

All methods are "user tier". Errors are in addition to `invalid_params` (bad or
missing params) and `internal`.

| method | params | result | errors | caller |
|---|---|---|---|---|
| `runner.status` | none | `StatusInfo` | | any |
| `runner.stop` | none | `null`, then the runner exits (sessions keep running) | | any |
| `sessions.list` | none | `[SessionInfo]`, oldest first | | any |
| `sessions.create` | `CreateReq` | `SessionInfo` | `forbidden`, `sandbox`, `limit`, `unknown_profile` | `client: "cli"` only |
| `runs.create` | `RunCreateReq` (optional `bundle` + `bundle_sha256`, `cwd_pin`, `origin`, `trust_cwd`, `trust_via_repo`, `exe_pin`, `argv_sha256`) | `SessionInfo` | `unknown_profile`, `sandbox`, `limit`, `invalid_params`, `busy`, `forbidden` | any; `client: "worker"` must send `origin`, every other client must not |
| `origin.revoke` | `{link, generation}` | `{killed, unconfirmed}` | `forbidden` | `client: "cli"` only |
| `candidates.list` | `{}` | `[{profile, source, created_ms, session}]` | | any client; cap `candidates.v1` |
| `profiles.login_check` | `{profile}` | `{problem?}` (the reason a session of the profile cannot start for lack of a login, absent when there is none) | `unknown_profile`, `invalid_params`, `forbidden` | any client except `worker`; cap `profiles_login_check.v1` |
| `runs.restore` | `{id}` (the interrupted session) | `{session: SessionInfo, resumed}` | `not_found`, `busy`, `forbidden`, `unknown_profile`, `sandbox`, `limit` | `client: "cli"` only |
| `sessions.resume` | `{id, cols?, rows?}` (an exited session) | `{session: SessionInfo, resumed, via}` | `not_found`, `busy`, `forbidden`, `invalid_params`, `unknown_profile`, `sandbox`, `limit` | `client: "cli"` only; cap `sessions_resume.v1` |
| `sessions.fork` | `{id, agent_id, name?, cols?, rows?}` (any state) | `{session: SessionInfo, resumed, via}` (`via` is `fork`) | `not_found`, `busy`, `forbidden`, `invalid_params`, `unknown_profile`, `sandbox`, `limit`, `internal` | `client: "cli"` only; cap `sessions_fork.v1` |
| `runs.lookup` | `{start_key}` | `SessionInfo` | `not_found` | any client; cap `start_key.v1` |
| `runs.status` | `{id}` | `{id, alive, state, exit_code, signal, started_ms, ended_ms}` | `not_found` | any client; cap `run_status.v1` |
| `sessions.text` | `{id, lines?}` (`lines` 1 to 10000, default the screen height) | `{id, rows, cols, lines, scroll_max}` | `not_found`, `invalid_params`, `forbidden` | any client except `worker`; cap `session_text.v1` |
| `trust.list` | none | `[TrustedFolder]` (`{path, dev, ino, added_ms, how}`) | | any client except `worker`; cap `trusted_folders.v1` |
| `trust.add` | `{path, dev?, ino?}` (absolute) | `TrustedFolder` | `invalid_params` | `client: "cli"` only; cap `trusted_folders.v1` |
| `trust.remove` | `{path}` | `{removed}` | | `client: "cli"` only; cap `trusted_folders.v1` |
| `objects.list` | none | `[ExemptFolder]` (`{path, dev, ino, birth_ns, fsid, added_ms, measured_ms, stale}`) | `forbidden` | `client: "cli"` only; cap `object_exempt.v1` |
| `objects.allow` | `{path, dev, ino, birth_ns, fsid, measured_ms}` (absolute; all four identity fields must equal what the runner reads now) | `ExemptFolder` | `invalid_params`, `forbidden` | `client: "cli"` only; cap `object_exempt.v1` |
| `objects.deny` | `{path, dev?, ino?}` (removes the entries with this path or this `dev`/`ino`) | `{removed}` | `forbidden` | `client: "cli"` only; cap `object_exempt.v1` |
| `sessions.kill` | `{id, signal, reason?}` (`term`, `kill`, `int`, `hup`; `reason` at most 200 characters, control characters become `?`) | `null` | `not_found`, `not_running`, `invalid_params` | any |
| `orchestrator.create` | `{profile, cwd, start_key, name?}` (no other field) | `SessionInfo` | `forbidden`, `invalid_params`, `unknown_profile`, `sandbox`, `limit`, `busy` | `client: "cli"` only; cap `orchestrator.v1` |
| `autopilot.start` | `{settings_id}` (`[a-z0-9-]`, 1 to 64) | `{pid}` | `forbidden`, `busy`, `invalid_params` | `client: "cli"` only; cap `autopilot.v1` |
| `autopilot.status` | none | `{running, pid?, status}` (`status` is `status.json` or `null`) | `forbidden` | `cli` and `desktop`; cap `autopilot.v1` |
| `autopilot.control` | `{command}` (`pause`, `resume`, `stop`, `suspend`) | `{seq}` | `forbidden`, `not_running` | `cli` and `desktop`; `suspend` from `cli` only; cap `autopilot.v1` |
| `autopilot.log` | `{tail}` (1 to 1000) | `{lines}` (the last `tail` JSON lines of `log.jsonl`) | `forbidden`, `invalid_params` | `cli` and `desktop`; cap `autopilot.v1` |
| `sessions.send` | `{id, text, at_prompt?}` (text at most 64 KiB, typed as input; `at_prompt` true refuses with `busy` unless the agent sits at its idle prompt) | `null` | `not_found`, `not_running`, `busy` | any |
| `permissions.list` | none (`client: "cli"` only) | array of `PermissionCard` | `forbidden` |
| `permissions.answer` | `{session, card, choice: "yes"\|"always"\|"no", confirmed?}` (`client: "cli"` only) | `null` | `forbidden`, `not_found`, `busy` |
| `sessions.remove` | `{id}` (exited sessions only) | `null` | `not_found`, `busy` | any |
| `sessions.rename` | `{id, name}` (1 to 64 printable characters, unique among sessions) | `null` | `not_found`, `invalid_params`, `busy` (name taken) | any |
| `sessions.worklog` | `{id}` | `WorkSummary` | `not_found` | any |
| `checkpoints.enable` | `{id, keep?}` | `CheckpointList` | `not_found`, `invalid_params` | `cli` |
| `checkpoints.list` | `{id}` | `CheckpointList` | `not_found`, `invalid_params` when off | `cli` |
| `checkpoints.diff` | `{id, n, against?}` | `CheckpointDiff` | `not_found`, `invalid_params` | `cli` |
| `checkpoints.restore` | `{id, n}` | `CheckpointRestored` | `not_found`, `busy`, `forbidden` | `cli` |
| `layout.get` | none | the stored JSON value or `null` | | any |
| `layout.set` | `{layout}` (any JSON, at most 64 KiB serialized) | `null` | | any |
| `attach.open` | `AttachParams` | `null`, then pushes start | `not_found` | any |
| `attach.set` | `AttachParams` | `null` | `not_found` | any |

`CreateReq`: `{argv:[str], cwd, name?, profile?, cols=80, rows=24, env_pass?:[str], task?}`.
argv has 1 to 64 entries, each at most 4096 bytes; argv[0] is an absolute path.
`sessions.send` answers `busy` when the input queue is full or the shim dropped earlier input; retry later.

`RunCreateReq`: `{profile, task?, cwd, name?, model?, cols=80, rows=24, bundle?, bundle_sha256?, cwd_pin?, origin?}`. The
runner loads the agent profile (built-in or the user's `agents` directory),
builds argv, resolves argv[0] through its own `PATH` and then follows the exact
same validation, sandbox and creation path as `sessions.create`. A missing
executable is `invalid_params`. A desktop webview never sends argv.

Both: `cwd` is an existing absolute directory (not `/`, the home directory or a
protected path); `name` is 1 to 64 printable characters; `cols`/`rows` are
clamped to 1..500 and 1..200.

`SessionInfo`: `{id, name, argv, cwd, profile: str|null, state, created_ms,
cols, rows, shim_pid, child_pid, task?, bundle_sha256?, origin?, resumed_from?, human_s?, no_sandbox?, permission_cards?, team?, agent_session_id?}`. `task`, `bundle_sha256`, `origin`, `resumed_from`, `human_s`, `no_sandbox`, `permission_cards`, `team` and `agent_session_id` are absent when not set. `agent_session_id` (cap `external_resume.v1`) is the id of the agent conversation an external resume continues. `team` (cap `team.workers.v1`) is the label the run was created with. `no_sandbox` (cap `no_sandbox.v1`) is `true` for a run started with `no_sandbox`; absent otherwise, and a client treats absent as sandboxed. `human_s` (cap `human_time.v1`, `sessions.list` only) is the human seconds counted for the session, closed chunks plus the open one; absent when 0. `resumed_from` is the id of the exited session this one replaced through `sessions.resume`; a client that tracks sessions by id maps the old id to this one.

`TaskRef`: `{workspace_id: i64, task_id: i64}`. Local workspaces have negative
ids. The runner treats it as opaque, stores it in the session's `meta.json` (it
survives runner restarts) and echoes it in `SessionInfo` and `Event`.

`state`: `{kind, since_ms, detail: str|null, exit_code: int|null}`, `kind` is one
of `starting`, `working`, `waiting`, `blocked`, `exited`.

`StatusInfo`: `{pid, version, proto, uptime_s, sessions}`.

`AttachParams`: `{sessions:[id] (at most 64), sizes?: {id: [cols, rows]}}`. It
replaces the watched list; ids must exist. `attach.open` also turns the
connection into a push stream (once). An empty list gives state events only.

### Restoring interrupted runs

Every `runs.create` that succeeds leaves a restore record in `<homes>/.intents/<id>.json`
(mode 0600, inside the per-run homes directory that agents cannot reach; the runner
directory is not used because on Linux it lives in `$XDG_RUNTIME_DIR` and is gone after a
reboot). The record holds the profile id, model, name, canonical `cwd` with its
`(dev, ino)` pin, task, `bundle_sha256` (the canonical bundle is stored next to it as
`<id>.bundle.json`), `origin`, terminal size, creation time, a boot marker and the shim and
agent pids. While the boot marker matches and one of those pids is alive, the record counts as
still running and is not restored; the same holds while anything is left in the agent's process group. It never holds argv or
environment: a restore rebuilds argv from the current profile.

The record is dropped when the agent exits with code 0 and the runner is still up 10 s later
(agents often exit 0 on the SIGTERM of a shutdown, which kills the runner too), when `sessions.kill` is sent to
it and when the session is removed. Exits by signal, non-zero exits and sessions the
runner lost (reboot, logout) keep it.

`runs.restore {id}` (cap `runs.restore.v1`, `cli` only) claims the record by renaming it,
so two restores of the same session cannot both start. It refuses records older than
7 days, records with an `origin` (a link run is re-requested by its leader), bypass
runs and runs started with `no_sandbox`; then it sends the recorded launch through the same path as `runs.create`, with
the stored bundle and `cwd_pin`, so the bundle hash and the folder identity are checked
again (a replaced folder answers `invalid_params` "approved folder changed"). When the
old run's home still exists it is moved to the new session and, if the profile has
`[resume] args`, the agent starts with them (`resumed: true`); otherwise it starts fresh
with the same bundle. On success the old record and the old exited session are removed;
on failure the record is put back. The runner never restores on its own: each restore
is one explicit local request (`tmgr agents restore`).

### Forking a Claude Code session

`sessions.fork {id, agent_id, name?, cols?, rows?}` (cap `sessions_fork.v1`, `cli` only) starts a new
session from the conversation of a `claude-code` session in any state, with `claude --resume <agent_id>
--fork-session`. The new session has its own run home, a new session id and the optional `name`; the old
session is not touched. `agent_id` must be a lowercase UUID and the runner finds the conversation file
itself, only in the old session's run home (`.claude/projects/*/<agent_id>.jsonl`, opened without following
links, at most 512 MiB) and copies that one file into the new run home before the agent starts. The launch
is rebuilt exactly like a resume of that session (launch record or `meta.json`, same folder identity check,
same refusals for link, bypass, no-sandbox, expired and unpinned queue runs); the profile must declare `[resume] by_id`.
A `codex` session is forked the same way with `codex fork <agent_id>`: the rollout
(`.codex/sessions/YYYY/MM/DD/rollout-*-<agent_id>.jsonl`, whose first line must be the session metadata of
`agent_id`) is copied to the same relative path, and the profile must declare `[resume] fork_by_id`.
Any other profile is `forbidden`. The runner also answers `forbidden` unless reading the agent's logs (Claude Code or Codex) is allowed on this device.

### Resuming an exited session

`sessions.resume {id, cols?, rows?}` (cap `sessions_resume.v1`, `cli` only) starts an
exited session again in place: same name, folder and profile, a new session id, and
`SessionInfo.resumed_from` set to the old id. The old session must be `exited`
(`busy` otherwise, also while another resume or a `runs.restore` of it is running, and
`busy` when processes of the old session's process group are still alive); `sessions.remove`
of it is `busy` while a resume is running. The old session is removed once the new one has
started, and a failed resume leaves it as it was. The new session keeps the old session's
current name (a rename before the resume is kept).
`cols` and `rows` default to 80 and 24 and are clamped like create.

An agent run (a profile) is relaunched from a copy of its launch record that the runner
keeps in the session directory (`relaunch.json`, plus the canonical bundle when there is one),
so it survives the clean exit and the kill that drop the `runs.restore` record. The same
rules as `runs.restore` apply and nothing is weakened: records with an `origin` (link), bypass
and no-sandbox runs, records older than 7 days and queue runs without an `exe_pin` are `forbidden`; the
executable pin, the argv digest, the folder identity (`cwd_pin`) and the stored bundle hash
are checked again. A session without a launch record (started before `relaunch.json` existed) is rebuilt
from its own `meta.json`: the profile is looked up in the runner's current profile table (the stored
`argv` is never run), and the recorded folder identity, permission mode, name and task binding are
reused; nothing comes from the client. The 7-day limit counts from the original start. When that is impossible (no recorded folder, or a
bundled, multi-repository or keyed run) the answer is `not_found` saying to start it again with
`tmgr run`; a profile that is no longer configured is `unknown_profile`. On both paths (launch record
or `meta.json`) the folder path is resolved and its identity (dev, inode) compared with the recorded one
before anything starts: a folder that is gone is `invalid_params`, a folder replaced by another
directory or a symlink to one is `forbidden` "the folder of this session changed since it started".
While an exited session's shim is still alive, the runner takes the folder identity the shim reports on
adoption, not the stored one. A shim that dies at start returns `internal` with the last
lines of its own log. Any other session
(a shell or `--agent-cmd`) is rebuilt from its own metadata and run again; its folder must
still be the same directory (identity compared), otherwise `forbidden`.

The result is `{session, resumed, via}`. `via` says how the command line was built:
`id` (the agent resumes the conversation named in its own last output), `continue` (the
profile's `[resume] args`, for example `--continue` or `resume --last`, used when no id
was found and the run home exists), `rerun` (a session without a profile was run again),
`fresh` (no conversation to continue; the agent starts new). `resumed` is true for `id` and `continue`.

The id comes from the old session's own `output.log` only: the last 64 KiB, opened without
following links, cut after the last `ESC [ ? 1049 l`, with escape sequences and control
bytes removed. Only the session's own agent counts (`claude-code`: `claude --resume <id>`, `codex`: `codex resume <id>`; any other profile gets no id). The last match wins; `<id>` must be a
lowercase UUID (`8-4-4-4-12` hex digits) and is passed as its own argument, never through a shell.
A profile opts in with `[resume] by_id = ["--resume", "{resume_id}"]`: exactly one whole
argument `{resume_id}`, placed like `[resume] args` (`at`). A run that was approved with an argv
digest also stores the declaration (including `by_id`); a record without `by_id` never gets one, and every later resume keeps the declaration that was approved.

The new session starts with the old screen text in its scrollback, followed by a dim
`-- resumed --` line. The text is written into the screen model directly (it does not
pass the status matcher, so an old prompt cannot mark the new session `blocked`), stored as
`prior.txt` (at most 1 MiB, mode 0600) in the session directory and loaded first after a
runner restart. `max_sessions` does not count the exited session being replaced.

### Pinning the folder

`RunCreateReq.cwd_pin` is optional: `{"dev":16777234,"ino":8812345}`, the
`st_dev` and `st_ino` that the client read with `stat` on the folder the human
approved (cap `cwd_pin.v1`). The runner opens the canonical `cwd` once with
`O_DIRECTORY | O_NOFOLLOW`, compares the opened directory's identity with the
pin and refuses with `invalid_params` ("approved folder changed") when they
differ. It checks the same identity again right before it starts the agent
(`invalid_params`, nothing is written to the folder or launched). Without a pin
the identity is still fixed at the first open and the agent starts in that
opened directory (see the bundle section), so the folder cannot be swapped
between the check and the start. A pin works with and without a bundle.

### Link-originated runs

A run that comes over a leader to follower link names its origin (cap
`link_origin.v1`): `"origin":{"link":"laptop","generation":7}` on `runs.create`.
The runner records it in the session's `meta.json` (it survives restarts) and
echoes it as an optional `origin` in `SessionInfo` and `Event`; local runs have none.

Who may send it: only a connection whose hello `client` is `"worker"` may send
`origin`, and a `worker` must send it on every `runs.create`; any other
combination is `forbidden`. `sessions.create` stays `cli` only. The client kind
is self-declared by whoever holds `client.secret` (the same user), so this is a
guardrail against mistakes, not a boundary. The authority for a link's scope is
the follower's own trust store (`links.toml` in the config directory, which must
be a private regular file), which the runner reads itself on every request; the
worker is not trusted to have checked it, and there is no bypass field.

Checks, all under the runner's create lock before anything is written. Each
failure is `forbidden` unless noted:

- the link exists in the trust store (a revoked or removed link does not) and
  `generation` equals the link's current generation, else "link changed;
  re-request". The generation is bumped on every trust or scope change;
- `generation` is above the highest generation passed to `origin.revoke` for the link;
- `profile` is in the scope's agent profiles and, when the scope lists models,
  `model` is one of them;
- `cwd` is opened once (`O_DIRECTORY | O_NOFOLLOW` after canonicalisation) and
  its directory identity walked upwards with `..` (bounded depth) until it
  equals one of the scope's repository roots, each opened as a descriptor. No
  string prefix is compared, so symlinks and `..` cannot leave a root. The same
  descriptor is re-checked right before the agent starts, as for `cwd_pin`;
- fewer than the scope's `max` sessions that are not exited carry the same
  `origin.link`, else `limit`.

Trust fence: for a link-originated run the runner holds the follower's trust
fence `link/links.lock` (next to `links.toml`) with a shared `flock` from before
the checks above until the agent has been spawned. `tmgr link` takes the same
file exclusively for trust, scope and revoke changes, so such a change lands
before the generation check or after the spawn, never in between. The runner
waits at most 5 seconds for the fence, then answers `busy` (retry). Lock order:
the runner's create lock, then the fence; a client must not hold the fence
exclusively while calling `origin.revoke` (that call takes the create lock).

`origin.revoke` `{link, generation}` (caller `client: "cli"` only; the follower's
owner calls it from `tmgr link revoke`). Under the create lock it records
`link -> generation` in `revoked.json` (mode 0600, in the runner directory, so it
survives restarts), then sends SIGKILL to the process group of every session
that is not exited and whose origin is that link with a generation up to the
given one. A session counts as `killed` only when its shim reports the exit
within a per-session wait; otherwise it is unconfirmed: it stays not exited (state
detail `revoking`), keeps its directory reserved, and the runner retries the
kill every second (re-adopting the shim if needed) until the exit is reported.
The revocation is incomplete while `unconfirmed` is not empty. The result is
`{"killed":n,"unconfirmed":["s_…"]}` (the session ids still being retried). It is idempotent; a later call keeps the
highest generation. After it, `runs.create` for that link and a generation up to
the recorded one is `forbidden`; a newly trusted link has a higher generation.

### Refreshed logins

A profile seed with `write_back` never changes the owner's file by itself. When a session ends with a changed copy that
passes the runner's checks, the copy is stored as a candidate (`<runner dir>/candidates/<profile>/<seed index>.json`)
and `candidates.list` (cap `candidates.v1`) reports it: `profile`, `source` (absolute path of the file the candidate
would replace), `created_ms` and the `session` that produced it. The result never carries file contents; an empty list
means nothing is pending. The owner applies a candidate with `tmgr agents sync <profile>`, not over this protocol.

### Queue capabilities

All four are additive: new optional fields and methods, no change for clients that do not send them.

**`permission_mode.v1`.** `runs.create` takes an optional `permission_mode`: `ask`, `accept-edits`, `plan` or `bypass`. Bypass is accepted in two cases only: a cloud task with a verified approval, or a manual run from the CLI. A manual run is `client: "cli"` with no `origin`, no `start_key`, no `unattended` and no `clones`; it needs no cloud approval (a cloud task, a local-workspace task or no task at all). Every other request for bypass (desktop and worker clients, queue and autopilot runs, link runs, clones) is `forbidden`, and so is a forged or unbound cloud approval. The profile must have a `bypass` entry in `permission_modes`. Only bypass with a verified cloud approval gets the network allowlist; a manual bypass run without one keeps the open network. The profile maps each mode it supports to arguments (`permission_modes` table) and the runner appends them after the profile's own arguments. A request names a mode the profile has no entry for, or a profile that has a `permission_modes` table but no entry for `ask` gets no mode: `invalid_params` ("profile X cannot run in mode Y"). A profile without any `permission_modes` table keeps its old behavior when no mode is requested (no extra arguments, no recorded mode). The effective mode (request value, else `ask`) is stored and returned as `SessionInfo.permission_mode`. `launch.permission_mode` of a bundle is optional and hashed only when present; when the request carries a bundle, the bundle's value must equal the effective mode, and an absent value (old bundles) is accepted only when the effective mode is `ask` or none. `sessions.create` is not affected.

**`start_key.v1`.** `runs.create` takes an optional `start_key` (`[A-Za-z0-9._:-]`, 1 to 64 characters). The runner stores it with a hash of the other parameters (everything except `cols`, `rows` and `start_key`). A second `runs.create` with the same key and the same hash returns the existing `SessionInfo` in whatever state it is, and starts nothing; the same key with other parameters is `busy` ("start key already used for a different request"). Keys survive a runner restart and are forgotten when the session is removed. `runs.lookup {start_key}` returns the `SessionInfo` or `not_found`.

**`run_status.v1`.** `runs.status {id}` returns `{id, alive, state, exit_code, signal, started_ms, ended_ms}`: `state` is the state kind, `alive` is true while the shim is reachable and the child has not exited, `exit_code`, `signal` and `ended_ms` are `null` until the child exits, `started_ms` is `created_ms`. It carries no screen.

**`clone_dir.v1`.** `runs.create` takes an optional `clone_of`, the absolute path of a repository. Before the sandbox is prepared the runner checks, walking descriptors without following any symlink, that the repository is a work tree root with a real `.git` directory, that the canonical `cwd` lies strictly inside `<repo parent>/<repo name>-worktrees/` (any depth), that `cwd/.git` is a real directory (not a worktree file) and that `cwd/.git/objects/info/alternates`, if present (at most 4 KiB), lists only `<repo>/.git/objects`. Any violation is `invalid_params`. A clone whose alternates list the repository (made by an older version) gets read-only access to `<repo>/.git/objects` and nothing else of the repository; a clone without alternates gets none. A clone the host recorded when it made it is refused if it has alternates. Writes stay limited to `cwd` and the per-run home.

**Runs without the sandbox (`no_sandbox.v1`).** `runs.create` takes an optional `no_sandbox: {"env": {...}}`. It is accepted only for a manual CLI run (see `permission_mode.v1`); any other request is `forbidden`. The agent then runs as it would outside tmgr: its `HOME` is the real home, its environment is `env` (every `TMGR_*` name is dropped; the runner sets `TMGR_SESSION_ID`, `TMGR_RUN_ID` when it applies, `TERM` and `HOME`), and the runner writes no seeds, credential files, secrets or folder trust, applies no Landlock policy and starts no egress proxy. The per-run home still exists as a plain directory and is the only one the runner ever removes. Hooks, agent work, the work log, the live view and attach keep working. On macOS the command still runs under a minimal Seatbelt profile that only denies writes to, and unix-socket connects under, the runner directory, the homes directory, the tmgr config directory, `~/.tmgr.dev` and the profile-protected paths, and writes to the runner executable; reads are not restricted. On Linux there is no such fence. The work log records `mode` and `sandbox` ("on" or "off") in its `created` event, and the agent work summary on finish names `mode: bypass, sandbox: off`. `SessionInfo.no_sandbox` is `true` for such a session. Such a session is never restored or forked; `sessions.resume` takes it back only with an explicit `no_sandbox` (see "External resume").

**External resume (`external_resume.v1`).** `runs.create` takes an optional `external_resume: {"id": "<agent session id>", "fork": false}` (`fork` absent means false): continue, or fork, a conversation the agent recorded outside the runner. It is accepted only for a manual CLI run that also carries `no_sandbox`, with no `origin`, `start_key`, `clones`, `team`, `agent_work`, `cloud`, `clone_of`, trust flags, bundle or `task`; any other request is `forbidden`. `id` must be a lowercase hyphenated UUID, else `invalid_params`. The runner builds the command only from the profile's `resume.by_id` (or `resume.fork_by_id`) template, never reads a transcript, and starts the agent in `cwd` exactly as sent: `cwd` must already be its resolved (canonical) absolute path, else `invalid_params`. A profile without the template answers `forbidden`. The new session reports `agent_session_id` (not for a fork).

`sessions.resume` takes optional `permission_mode` (`bypass`, or another mode the profile defines) and `no_sandbox`. Without them nothing changes, except that an expired session may be resumed while its run home exists. With them: a session started with `no_sandbox` resumes only with `no_sandbox` (through an external resume of the conversation id found in its log); a sandboxed session cannot be resumed with `no_sandbox`; a session that ran in bypass resumes only with `permission_mode: "bypass"`. Sessions started over a link, by the queue, by a cloud approval, for agent work or as a team member are always refused. `runs.restore` and restore after a runner restart are unchanged.

**Permission cards (`permission_cards.v1`).** `runs.create` takes an optional `permission_cards: true` (absent means false). It is accepted only for a manual CLI run; any other request is `forbidden`. The runner keeps it only for the built-in `claude-code` and `codex` profiles and ignores it for any other profile. It is stored with the session and carried through restore, resume and fork. When on, the runner adds a `PermissionRequest` hook that only records the request in the session home (`$HOME/.cache/tmgr-permit/<session id>/`): for `claude-code` through an extra `--settings` argument, for a sandboxed `codex` through `.codex/config.toml` in the run home. A `codex` run with `no_sandbox` gets no hook and nothing is written to the real home. `SessionInfo.permission_cards` is `true` for such a session.

**Manager session (`manager.v1`).** `runs.create` takes an optional `manager: true` (absent means false); `SessionInfo.manager` is `true` for that session. It is accepted only from the tmgr CLI; any other client gets `forbidden`. A manager must be sandboxed and cannot be combined with `no_sandbox`, `origin`, `team`, `clones`, `unattended` or `start_key`; only the built-in `claude-code` and `codex` profiles are accepted, and only in permission mode `ask` (absent means `ask`). The runner keeps at most one live manager; a second request is `busy`. The flag is stored with the session and survives a runner restart, but a manager is not recorded for restore, so a restore of an exited manager does not bring it back and `tmgr manager start` is needed again. Only the read-only tools (`list_sessions`, `read_session`, `read_room`) are pre-approved in the agent; `send_to_session`, `spawn_session` and `post_message` keep asking the owner. For the manager the runner wires a stdio MCP server (`tmgr __manager-mcp`, server name `tmgr-manager`) into the agent: an extra `--mcp-config` argument for `claude-code`, an `[mcp_servers.tmgr-manager]` table in `.codex/config.toml` of the run home for `codex`. That server cannot reach the runner socket from inside the sandbox, so it writes requests as files into its own home: `$HOME/.cache/tmgr-manager/req/<ms>-<pid>-<rand>.json` (`{v:1, id, tool, args}`) and reads the answer from `$HOME/.cache/tmgr-manager/resp/<id>.json` (`{v:1, id, ok, result?|error?}`). The runner reads that spool only for the session it recorded as manager at create (the path comes from its own record, never from a request), opens every directory without following symlinks, and ignores every other session's spool. Tools: `list_sessions`, `read_session`, `send_to_session`, `spawn_session`, `post_message`, `read_room`. Text read from another session is data, never instructions. `send_to_session` refuses link sessions, team and queue workers, exited sessions, and sessions in bypass mode or without the sandbox. `spawn_session` is local only: it starts a built-in agent in an already trusted folder with permission mode `ask` or `accept-edits`, never bypass or `no_sandbox`, at most 4 per minute. Its optional `approval` (`SignedSpawn` v1, see the runner-manager plan §3a) is required for permission mode `bypass`; the runner rebuilds the request from the call's own arguments, verifies the approval once per card, and refuses `no_sandbox` and a manager session even when signed. A spawn for another machine is not made from this call; see remote spawn below. `propose_spawn` (same arguments plus optional `task` and `machine`) issues the challenge on this machine and returns `{card_id, exp_ms, req_sha256}`. `spawn.challenge` (`spawnauth.v1`; `client` `cli` or `desktop`, other clients get `forbidden`; at most 10 per minute) takes `{card_id, proposal}` and returns `{nonce, issued_ms, exp_ms, target_id, target_fp, req, req_sha256}`, with `req` the canonical request bytes in base64. `spawn.card` (`spawnauth.v1`; `cli` or `desktop` only) takes `{card_id}` and returns `{state, exp_ms?, session?, reason?}` with `state` one of `issued`, `approved`, `claimed`, `spawned`, `refused`. `spawn_session` also accepts `card_id` instead of `approval`: the runner uses the answer the phone sent over the relay (`spawnok`) for that card, or refuses with `this card is not approved yet` while the card is live, and with `this card has expired or is closed; propose the spawn again` once it is expired, closed, unknown or not usable by the caller. `spawn.decide` (`spawn_decide.v1`; `client` `desktop` only, never relayed and never reachable from the manager) takes `{card_id, req_sha256, decision, edit?}` with `decision` one of `approve`, `reject`, `edit` and `req_sha256` the hash of the request bytes the owner saw; the runner refuses unless it equals the stored hash, the card is a live unapproved manager card and no answer (phone or desktop) is stored yet. `approve` stores a local approval that `spawn_session` with that `card_id` and the exact arguments can use once; `reject` closes the card with reason `rejected`; `edit` (`{cwd, mode, name?, task?}`, cards of a local manager only, `cwd` must be a trusted folder, bypass is not added to a card that did not ask for it) closes the card with reason `edited` and issues a new, not yet approved card. The result is `{state, card_id?, exp_ms?}` with `state` `approved`, `rejected` or `edited` (`card_id` and `exp_ms` of the new card). An older runner answers `unknown_method`. `spawn.card` reports a claimed card whose spawn failed as `refused` with reason `spawn_failed`, one left over from before this runner started as `refused` with reason `interrupted`, and one whose spawn is in flight as `claimed`. `runs.create` refuses a client-supplied `spawn_card`; the runner sets that tag itself on a spawn that came from an approval.

**Chat activity (`chat.activity.v1`).** A client that sends `{"activity": true}` with its first `chat.subscribe` on a connection also receives `chat_activity` pushes: `{type, v: 1, room, state: "working"|"idle", current: string|null, used: [string]}`, the labels of the tools of the current turn (at most 50). A label is the tool name with at most a file base name (`Read`, `Edit`, `Write`, `NotebookEdit`) or the program name of a Bash command, plus one plain subcommand for a fixed list of well-known programs; never arguments, patterns, URLs, file contents or tool output. Events are produced only while the owner has granted reading of agent transcripts for that agent; without consent nothing is read or sent. When consent is withdrawn the runner drops everything queued and sends one `idle` event with `current: null` and `used: []` per room that had shown activity, then nothing more. When a turn ends, one `idle` event carries the finished turn's labels in `used`. Activity is not a chat message: it is not stored and not in history. It reaches the chat relay only as sealed `relay.activity.v1` envelopes to phones that hold a lease (see "Relay activity" below). A second `chat.subscribe` on the same connection is a no-op that returns `null`: it starts no second push stream and does not change the first one's options. Buffering for activity is bounded: the runner keeps only the latest event per room, at most 256 rooms, and drops the oldest room beyond that. Activity does not make a slow client disconnect by itself, but a write that stays blocked until the socket write timeout closes the connection, as for any other frame. Codex sessions carry `state` only. Clients that do not opt in, and old clients, get nothing or ignore the unknown type.

**Activity call details.** A `chat_activity` push may also carry `calls`, an optional list that lines up with `used` and is omitted when empty: `{n, name, summary, result?, running?, error?}`. `n` counts calls in the session, `summary` is what the call does (Bash command, file path, Grep/Glob pattern, WebFetch URL, MCP tool arguments), `result` is the first 8 lines of the output, and `running`/`error` are set only when true. Every `summary` and `result` line is masked for secrets and cut to 200 characters inside the runner before it is queued. `result` is set only for Glob, Grep (not `content` mode) and Bash commands from a short allowlist of listing and build commands (no `$`, no input redirection, no files named like secrets), and only for the last 5 calls. These fields reach local-socket clients and, like the labels, reach phones only through the E2E relay while a phone holds a lease, re-masked and capped again before sealing. Clients that do not know `calls` ignore it.

**Relay activity (`relay.activity.v1`).** Two sealed relay kinds carry tool activity to phones; the runner advertises support with `"activity": true` in the `rooms` index it already sends (old readers ignore the field). `activity_on` (phone to runner, message-id prefix `u_`) is `{v: 1, on: bool}`. The runner accepts it only from a device that is trusted, joined, not revoked, fingerprint-matched and pinned as `mobile`; a desktop, a runner, an unpinned device or an untrusted one is ignored, and a stale message (older than 5 minutes) is dropped. It opens or removes a lease for that device: the lease expires 150 seconds after the runner received the message, by the runner's own clock (the payload carries no time), so the phone renews every 60 seconds and sends `on: false` when it leaves the Manager tab, goes to the background or the owner turns the setting off. At most 20 leases exist at once; a new device is refused while the table is full. `activity` (runner to phones, message-id prefix `t_`) is `{v: 1, machine, snapshot, rooms: [...]}` with `machine` the machine id of the `rooms` index and each room entry the `chat_activity` fields without `v` (`room`, `state`, `current`, `used`, optional `calls`). Only rooms of the runner's own machine are sent, and only to leased devices; a device without a live lease gets nothing, so an owner who never opts in produces no envelope at all. Before sealing the runner masks every label, `summary` and `result` line again (a no-op on already masked text), keeps the last 10 calls and 50 labels per room, and caps the payload at 48 KiB: over it, it drops every `result`, then every `calls`, then splits the rooms across several envelopes (the first keeps `snapshot`). The runner flushes every 3 seconds with at most one envelope per leased device; it sends only rooms that changed since the previous flush, a full `snapshot: true` to a device whose lease is new (with an empty `rooms` list when nothing is active, so the phone drops what it showed before), and a snapshot keepalive at most every 30 seconds while some room is `working`. When consent is withdrawn the blank `idle` events are forwarded once and then forgotten. Sends are not retried; after a failed send the device's next flush is a full snapshot, and a failure is logged once per error class. Like any relay message the envelope waits on the server until the phone fetches it, but it raises no push notification, and the phone drops envelopes that are too old or arrive while it holds no lease. The server sees only the message-id prefix, the size and the timing; never the content. Other versions of `v` are ignored by receivers.

**Unrestricted manager (`manager.unrestricted.v1`).** `manager.unrestricted` takes `{run, pin}` with `run` a `RunCreateReq` and `pin` the follower PIN. Only the tmgr CLI may call it; other clients get `forbidden`. It is macOS only: on other systems the runner answers `forbidden` (`an unrestricted manager is available only on macOS`) before any check, because there an unsandboxed session has no control-plane fence. The runner checks the PIN itself (the CLI gives it a checker at start): at most one check per 2 seconds, sharing the lockout of `tmgr link pin`. The runner refuses before it touches the PIN, so a doomed request costs no try: `run.manager` must be `true`; `origin`, `team`, `clones`, `unattended`, `start_key`, `clone_of`, `agent_work`, `spawn_card`, `parent_session`, `external_resume`, `trust_cwd` and `trust_via_repo` must be absent; the client sets neither `no_sandbox` nor a permission mode other than `ask`; the profile is `claude-code` only (`codex` is refused); no bundle; no other live manager (`busy`). A runner without a checker answers `forbidden` (`this runner cannot check the follower PIN`); a wrong PIN, a locked PIN and an unset PIN each have their own message. On success the runner itself sets `no_sandbox` (the runner's own base environment) and permission mode `bypass`, and starts the manager without the sandbox, in the real home, with `--strict-mcp-config` so the owner's other MCP servers are not loaded. Since the sandbox no longer gives the manager a private home, the runner creates `homes/<id>/.cache/tmgr-manager/{req,resp}` before the start and passes its canonical path to the MCP server as `TMGR_MANAGER_SPOOL` (with `TMGR_SESSION_ID`); on macOS the control-plane profile allows that one subtree after its deny rules. The server opens the spool root directly and descends only into `req` and `resp` without following symlinks, and refuses a spool whose `homes/<id>` name differs from its session id. What changes: the manager is unsandboxed and in bypass mode, and `send_to_session` may write to sessions in bypass mode or without the sandbox. What stays: link, team and queue workers, exited sessions and the manager itself are still refused; `read_session`, `spawn_session`, `propose_spawn` and `switch_agent` are unchanged (spawns still never bypass or `no_sandbox` without a signed approval); relay chat reaches the manager's terminal as before. The permission is derived from the live manager's own record (`manager`, `unrestricted`, `no_sandbox` and mode `bypass`), never from a request, and the manager RPC from other machines (`mgr_rpc`), `link` runs and the MCP server never get it. The mode is not restored: a restore of an exited manager does not bring it back. `RoomInfo.unrestricted` is `true` (absent means false) for the manager room while the live manager is unrestricted; `SessionInfo` has no new field, an unrestricted manager is `manager` with `no_sandbox`. `runs.create` still refuses a manager with `no_sandbox` or mode `bypass`.

`permissions.list` returns one `PermissionCard` for each live session with the flag on whose screen shows a permission prompt of the agent. A card is answerable (`can_answer`) only when all of these hold: the prompt is the last block on the screen with only the agent's footer below it; the newest unconsumed hook record of the session matches the prompt text exactly (the card `text` is then the full text from the hook, never cut); the foreground process group of the terminal, from a status report no older than 1 s, is the agent process; and the agent supports the tool (`Bash`, or `Write` with the full content visible). Anything else is a view-only card with `can_answer: false` and `why_not`. `id` is a digest of the session, the hook record and the prompt, so a new prompt or a new hook record is a new card. `armed_in_ms` is the time left of a 300 ms window that starts when the runner first lists a card; an answer inside it is refused. `always_label` is set only when the agent offers a rule for exactly this command and the card has no `danger` entries.

`permissions.answer` repeats every check against the live screen, in this order, and refuses with `forbidden` (the message says why) on the first failure: the session is running and has the flag; the prompt is still on screen; the recomputed `id` equals `card` (which also fails when a newer hook record exists); the card is answerable; the foreground check; the arming window; the card was not answered before; a card with `danger` needs `confirmed: true` for `yes`, and `always` is never sent for it; the agent shows the chosen option. Then the card is marked answered, its hook record is consumed (deleted), and exactly one key of the parsed option is queued as terminal input. Enter is never sent. A full input queue returns `busy` and the card stays answered. Only the `cli` client may call either method; `worker` and `desktop` get `forbidden`. Known limit: the agent can change state between the last status report and the key; the worst case is one key without Enter.

**`agent_work.v1`.** `runs.create` takes an optional `agent_work: true` (absent means false). The runner then records the run as Agent work of its task in the local workspace: it needs a `task` of a local workspace (negative `workspace_id`; a cloud task is `invalid_params`, owner decision 16a.11), a profile with a `permission_modes` table (otherwise `invalid_params` naming the profile), and is `forbidden` for link runs. The runner calls its own executable as `<self> __agent-work` with one JSON request on stdin (`op` `start`, `update` or `finish`) and keeps the run id with the session: `start` when the session is created (agent = profile id, model, branch, session id), `update` with the branch and commits from the work log at most every 10 minutes, and `finish` once when the child has exited: `succeeded` for exit code 0, `cancelled` after `sessions.kill`, a revocation or a session lost while the runner was down, otherwise `failed`, with a one-line summary and the commits. Failed calls are retried and never block the session; a session that ended while the runner was down is finished after the restart. A `start_key` replay returns the existing session and opens no second run. The flag is part of the start-key hash.

### Autopilot

**`autopilot.v1`.** The runner hosts at most one background loop, `<runner executable> queue autopilot serve --settings <settings_id>`. The client names only the saved settings; the runner builds the command line itself, so no argv or path ever comes from a client. It runs with the runner's cleaned environment (`TMGR_RUNNER_ENV_CLEAN=1`), in its own session, with stdin closed and stdout and stderr appended to `serve.log` (capped, rotated once at 5 MiB when a start finds it larger). The runner keeps the child and reaps it. It starts a loop on its own only at its own start, see "Suspend and resume" below.

State directory: `autopilot/` inside the tmgr config directory (`$XDG_CONFIG_HOME/tmgr` or `~/.config/tmgr`, the directory of `runner-allow.json`). Files, all private:

- `autopilot.lock`: the loop takes an exclusive `flock` on it at startup and holds it until it exits. Probing the lock takes a shared lock for a moment, so the loop retries a refused lock for a few seconds before giving up. A runner that restarts finds a held lock, reads `pid` from `status.json` and reports the loop as running ("adopts" it).
- `status.json`: written atomically by the loop; a JSON object that must carry `version`, `state` (`running`, `paused`, `stopping`, `stopped`, `suspended`), `pid` (the loop's pid) and `control_seq` (the last handled `control.json` `seq`). `autopilot.status` returns it as is, or `null`. A start moves a previous one to `status.prev.json`, unless the previous one is `suspended` with the same `settings_id`: then it stays in place and the new loop continues from it.
- `control.json`: written atomically by the runner on `autopilot.control`, `{seq, command, at}`; `seq` counts up from 1 for each start (a start deletes the file). The loop handles a command when `seq` is above the last it handled (0 at startup) and echoes it in `control_seq`. `stop` and `suspend` also tell the runner not to treat the following exit as a crash.
- `log.jsonl`: appended by the loop, one JSON object per line. `autopilot.log` returns the last `tail` lines that parse, looking at most 1 MiB back.

`autopilot.start` is `busy` while a loop runs (own child, or a held lock). On an exit with a non-zero code or a signal, not after a stop was requested, the runner merges `state: "stopped"`, `stop_reason: "crashed"`, `exit` and `stopped_at` into `status.json` (creating it when missing) and disables the queue by running `<runner executable> queue disable --reason "autopilot crashed"`, the audited path of `tmgr queue disable`. When the runner stops it first sends `suspend`, waits 20 seconds, then sends SIGTERM.

Settings have a kind: `drain` (works through chosen tasks, then stops) or `queue` (the continuous loop, settings id `queue`: every rule, no caps, no usage guard, never stops for "nothing left"; it ends when the queue is turned off, on `stop` or on an error).

#### Suspend and resume

`suspend` (`autopilot.control`, from `cli` only; the desktop gets `forbidden`) makes the loop exit with code 0 without turning the queue off and without interrupting running agents. It writes `state: "suspended"` first. SIGTERM, SIGINT and SIGHUP sent to the loop mean the same. A loop started again with the same `settings_id` continues from that `status.json`: it keeps `started_at`, `events`, `prs`, `failures_in_row` and the started and result counters, and logs a `resumed` event instead of `started`.

The runner sends `suspend` (not `stop`) when it shuts down. When it starts and no loop holds the lock, it starts a loop in this order: the `settings_id` of a `status.json` in state `suspended`; otherwise `queue` when `queue.toml` in the tmgr config directory has `enabled = true` and `autopilot/settings/queue.json` exists. A start that fails is only logged.

`sessions.kill` takes an optional `reason`. When the session is an Agent work run, the runner uses it as the `summary` of the `cancelled` finish instead of the default text.

### Session text

`sessions.text {id, lines?}` (cap `session_text.v1`) returns the session's terminal contents as plain text from the runner's live screen model: `{id, rows, cols, lines, scroll_max}`. `lines` are strings without attributes, trailing spaces trimmed, oldest first; the last entry is the bottom row of the screen, so blank bottom rows are kept as empty strings. `lines` defaults to the screen height (`rows`); a larger value adds scrollback above the screen and is capped by what exists and by about 512 KiB of text (the oldest lines are dropped first), so fewer lines than asked may come back. `lines` outside 1 to 10000 is `invalid_params`, an unknown session `not_found`. Exited sessions answer too, with the screen as it was at exit. The runner never stores the text, and this method is refused for a `worker` client (link-originated runs): `forbidden`. A leader that attaches to a run it started on a follower still sees that run's screen through `attach.open` (section 5, "Link-originated runs").

### Pinned executable

**`exe_pin.v1`.** `runs.create` takes an optional `exe_pin {path, sha256}`: the agent executable a human approved, as the approving side resolved it. `path` must be absolute and `sha256` exactly 64 lowercase hex characters (the whole file), otherwise `invalid_params`. After the profile command line is built, the runner resolves `argv[0]` with its own `PATH` as usual, then, with `exe_pin` present:

1. canonicalizes (realpath) the resolved `argv[0]` and `path`; if they differ, or `argv[0]` cannot be resolved (not found, not executable), the run is `forbidden`;
2. opens the canonical file once (read-only, close-on-exec, without following a final symlink), requires a regular file (checked on the opened descriptor) of at most 512 MiB and streams it through SHA-256; anything else, or a different digest, is `forbidden`. It then compares the descriptor with the path on disk (device, inode, size, modification time, also against the state before hashing); any difference is `forbidden`;
3. replaces `argv[0]` with the canonical path, so the exec uses exactly the verified path and not a `PATH` lookup (`SessionInfo.argv[0]` shows it).

The hashing happens before the start-key lock is taken; a `start_key` replay still returns the existing session first.

Error messages never contain file contents.

What the pin does not cover:

- There is no `fexecve` on every platform (macOS has none), so the check and the exec are bound by the path only. Someone who can write to the approved file as the same user can still swap it between the check and the exec; the comparison above only narrows that window.
- The pin covers the file at `argv[0]` only. The interpreter named by the `#!` line of a script and the target of a wrapper are not pinned and are not hashed. An approval that shows the pin should say so.

`exe_pin` is allowed together with `start_key`, `clone_of` and `bundle`, from any client, and is part of the start-key hash. The runner stores the verified pin with the restore record: `runs.restore` re-resolves `argv[0]`, runs the same check against the stored pin (`forbidden` on a mismatch or an unresolvable command, the record is kept) and executes the canonical path. A run started without a pin restores without one, except a queue run (started with a `start_key`; a record written before the runner stored that flag counts as one when it has `agent_work`): its restore is `forbidden` ("queue the task again") when it was recorded without an `exe_pin`, and the record is kept.

**`argv_pin.v1`.** `runs.create` takes an optional `argv_sha256`: the SHA-256 of the whole command line a human approved. It needs `exe_pin` in the same request (otherwise `invalid_params`) and must be exactly 64 lowercase hex characters (otherwise `invalid_params`). The digest is defined as follows, so that the approving side can reproduce it:

- take the final argv exactly as the runner executes it for a new start: the profile `command` and `args`, then the model arguments (when `model` is set), then the permission-mode arguments (when the profile has `permission_modes`), then the profile's `[prompt]` arguments when its mode is `arg`, with the literal string `{prompt}` left in place of the task prompt, in that order, without the `resume` arguments;
- replace `argv[0]` with the canonical executable path (the `exe_pin.path` that the pin check verifies);
- feed SHA-256 with each element's UTF-8 bytes followed by a single `0x00` byte, nothing else;
- write the digest as lowercase hex.

`tmgr_ipc::api::argv_sha256` implements this. Known vector: `["a", "b"]` gives `8fb20ef63ced4145fc2e983ffe597d1dcff39154c3bf21f0fa9dde6a0c50fdc9`. After the `exe_pin` check, the runner computes the digest of the argv it built and compares; a difference is `forbidden` ("the agent command line differs from the approved one"). A `start_key` replay still returns the existing session first, and `argv_sha256` is part of the start-key hash. The digest is stored with the restore record; `runs.restore` rebuilds the start-form argv from the current profile, checks it the same way (`forbidden` on a mismatch, the record is kept) and only then adds the `resume` arguments for the exec. Because the digest does not cover the `resume` arguments, a run created with `argv_sha256` also stores the profile's `[resume]` declaration (the `args` list and the `at` position, or its absence); `runs.restore` requires the current profile's declaration to be equal, otherwise it is `forbidden` ("the agent's resume arguments changed since the run was approved") and the record is kept.

### Pre-accepted folder trust

**`multi_clone.v1`.** `runs.create` takes an optional `clones`, a list of 1 to 5 `{name, clone_of}`, for a cloud task whose category has an explicit repository set. It is accepted only with `agent_work`, a `cloud` approval that carries `repos_sha256`, no `clone_of`, `origin` or `trust_cwd`; a `repos_sha256` without `clones`, and `clones` without a `repos_sha256` (or on a category bound to one path), are refused. The runner checks the approval (version 2 signature over the set) and that the bound set on this machine has exactly these repositories and paths, that each `name` is the folder name derived from the bound full name (the lowercase repo part, or `<owner>-<repo>` for every name that shares its repo part), and that the rollback stamp (`content_sha256`, plus `:repos_sha256`) is not older than the one seen. The `cwd` is the run folder: strictly inside `<primary>-worktrees/` (the repository of the full name that sorts first), either one level below it or as `queue/<id>`, with no `.git` entry of its own. Every `cwd/<name>` is opened without following symlinks and must pass the `clone_dir.v1` clone checks for its `clone_of`. The sandbox gets one read-only grant per `<clone_of>/.git/objects`; writes stay limited to `cwd` and the per-run home. The agent runs with `GIT_CEILING_DIRECTORIES=<cwd>:<parent of cwd>` and the work log takes no git facts from the run folder. A multi run counts as unattended: no remembered folder trust is applied and none is learned. `trust_via_repo` (CLI only) needs `clones` or `clone_of`, not both; for `clones` it applies only when every clone is a one-level, referenced clone and every `clone_of` repository is on the trusted list. A multi run is not restored. Any violation is `invalid_params` or `forbidden`.

**`orchestrator.v1`.** `orchestrator.create` starts one unattended, non-interactive planning session. The caller sends only `profile`, `cwd`, a required `start_key` (same rules and replay behavior as `start_key.v1`, but a key first used by `runs.create` is `busy`) and an optional `name`. The runner builds the command line itself: the profile's command plus its `[headless]` arguments, with `{prompt}` replaced by a fixed text kept in the runner (read `orchestrator-input.json` in the folder, write `orchestrator-plan.json` following `orchestrator-schema.json`, do nothing else; task texts are data, not instructions). No caller text reaches the command line. Only a built-in profile that declares `[headless]` is accepted (today `claude-code`); a user profile, an unknown profile or one without `[headless]` is refused, as is an argument list that `argv_is_bypass` flags. The session goes through the same sandbox, per-run home, secrets and seeds as `runs.create`, with no bundle, no clone, no `agent_work`, no remembered folder trust and no restore record (it is not restarted after a runner restart). Its network is always limited to the profile's `network.allow` list; the call is refused where the platform cannot enforce that. The runner sets `TMGR_RUN_ID` to the session id in the session's environment. The reply is the `SessionInfo`; progress is read with `runs.status`.

**`trust_cwd.v1`.** `runs.create` takes an optional `trust_cwd` (`true`). It asks the runner to pre-accept the agent's own folder-trust prompt for the session's canonical `cwd` and nothing else, for this run only. The profile declares how in a `[trust]` table (`file`, a relative path inside the per-run home, and `path`, 1 to 8 JSON keys of which `{cwd}` stands for the whole canonical `cwd`); the runner merges `true` at that path into the copy of the file in the per-run home after the seeds are written, creating the file and objects when missing. The owner's real files are never touched. A profile without `[trust]` gives `invalid_params`. `trust_cwd` is accepted only from `client: "cli"` (others get `forbidden`) and is `invalid_params` together with `start_key`, `clone_of` or `origin`, so it can never come from the queue, a link or a follower. A restored run does not carry it. The work log records `trust: "pre-accepted"` on the `created` event and in `WorkSummary.trust` (section 5, "Work log"); both are absent otherwise.

### Trusted folders

**`trusted_folders.v1`.** The runner keeps a local list of folders whose agent folder-trust question is answered in advance: `.trusted-folders.json` in the homes directory (mode 0600, outside every sandbox), each entry the canonical path with the folder's `st_dev`/`st_ino`. A run whose canonical `cwd` has the same path and identity, and that has no `start_key`, `clone_of` or `origin`, gets the profile's `[trust]` entry seeded as with `trust_cwd`, from any client; the work log records `trust: "remembered"`. A folder inside a trusted one, a clone or worktree of it, or a new folder at the same path is not trusted. Entries come only from `trust.add` (`client: "cli"`; `tmgr runner trust` asks to confirm and refuses inside an agent session) or from the owner's own answer: when a run of the built-in `claude-code` profile that the runner did not pre-trust (no `start_key`, `clone_of`, `origin`, not restored) exits, and its per-run `.claude.json` (a plain file, not a link) records `projects.<cwd>.hasTrustDialogAccepted: true`, the folder is added with `how: "answered"`, unless any input to that session came from a client other than `cli` or `desktop` (for example a link worker). Claude Code runs nothing before that question is answered, so the record comes from a person at this machine. The answer is read once per session (the session's record drops it when read or when foreign input arrives), so a runner restart never adds a folder again after `trust.remove`. `trust.add` may carry the `dev` and `ino` the person confirmed; a different folder at the path is then `invalid_params`. `trust.remove` matches the stored path or the canonical form of the given one, also for a folder that no longer exists.

**`trust_via_repo.v1`.** `runs.create` takes an optional `trust_via_repo` (`true`) for the per-task clones `tmgr run` makes. It is accepted only from `client: "cli"` (others get `forbidden`), needs `clone_of` and is `invalid_params` together with `start_key`, `origin` or `trust_cwd`, so the queue (which always sends a `start_key`), a link or a follower can never use it; `tmgr run` does not send it inside an agent session, and a restored run does not carry it. The profile's `[trust]` entry is seeded for the clone's canonical `cwd`, as with `trust_cwd`, only when every `clone_dir.v1` check passes and in addition: the `clone_of` repository's path and `st_dev`/`st_ino` match an entry of the trusted list; `cwd` is exactly one level below `<repo>-worktrees/` and that name is not `queue`; the clone's `objects/info/alternates` names the repository's object directory, or the runner's homes folder holds a `.task-clones/<dev>-<ino>` record for that clone written when it was made, with the same clone path and repository identity; and no session that has not exited has a `cwd` equal to or above the clone (such an agent could have replaced the folder). The runner cannot prove the folder is the clone `tmgr run` just made, only these properties. Otherwise the run starts without it and the agent asks as usual. A folder inside such a clone, a clone elsewhere, a clone with neither alternates nor a record and a `-worktrees` folder reached through a symlink are never covered. The work log records `trust: "repo"`. Answering the question inside a clone never adds the repository (or the clone) to the list.

### Run bundles

A run bundle is the immutable input of one agent run: the task text, its
comments, the context Pages with their revisions, the persona instructions
pinned by hash and the launch parameters. A client that wants an agent to work
on exactly what a human approved sends that bundle with `runs.create` (cap
`run_bundle.v1`). `bundle` and `bundle_sha256` are both present or both absent.

`RunBundle`:

```json
{"v":1,"task":{"workspace_id":-3,"task_id":412},"title":"...","body":"...",
 "author":{"kind":"human","id":"u_7"},
 "comments":[{"author":{"kind":"agent"},"at":"2026-10-13T09:30:00Z","body":"..."}],
 "pages":[{"id":9,"revision":3,"title":"...","body":"..."}],
 "persona":{"name":"reviewer","instructions_sha256":"<64 hex>"},
 "launch":{"profile":"codex","model":"gpt-5","cwd":"/work/project"}}
```

Cloud runs (cap `signed_context.v1`) may add, all omitted when absent so other bundles and their hashes are unchanged: `persona.id` (lowercase UUID) with `persona.instructions` (at most 32768 bytes after `\r\n` became `\n`, and `persona.instructions_sha256` must be the SHA-256 of its UTF-8 bytes), and `context`, a list of at most 5 pages `{id, revision, title, body}` (`revision` is the page version, display only) with titles and bodies totalling at most 65536 UTF-8 bytes after `\r\n` became `\n`. These texts are the owner's: the runner refuses a bundle that carries them unless the request has a `cloud` approval that signs them (`cloud.persona_sha256` = `persona_sha256(id, name, instructions)` and `cloud.context_sha256` = `context_sha256(page hashes in order)`, formats in `tmgr-approval`; a bundle without the hash for a text it carries, or with a text whose hash differs, is `forbidden`). Only then does `task.md` start with the sections "Instructions from the owner (persona <name>)" and "Rules from the owner (workspace context)", labelled as signed by the person who approved the run; the persona name and page titles are collapsed to one cleaned line and `<<<` is escaped in all of them. The prompt then also says to read and follow them first.

`launch.permission_mode` (`ask`, `accept-edits` or `plan`) is optional, omitted when absent, and part of the hash when present.

A bundle may carry `team` `{size, mode}` (cap `team.v1`; `size` 1 to 5, `mode` `eco` or `heavy`, no other fields), omitted when absent so other bundles and their hashes are unchanged. It is a note from the runner side, not owner text: it needs no signature, and `task.md` shows it in a separate block labelled as set by the runner on this machine and not signed.

`runs.create` takes an optional `team` `{role, of}` (cap `team.workers.v1`, no other fields): `role` is `"lead"` or `{"worker": N}` with `N` from 1, `of` is the pickup id (1 to 64 characters of `A-Za-z0-9._-`); the run is created with it and `SessionInfo.team` returns it. It is a label for grouping and display and grants nothing: the sandbox, grants, home and egress of a member are built exactly as for a single run, and an invalid link is `invalid_params`. A bundle may carry `assignment`, a string of at most 16384 bytes, omitted when absent so other bundles and their hashes are unchanged and part of the hash when present. It is unsigned text written by the lead agent: it is outside the cloud signature, the runner refuses it (`invalid_params`) unless the request carries a `team` with role `worker`, and `task.md` shows it after the team block under "Assignment from the lead", inside the same untrusted data markers as other task text (`<<<` is escaped), never among the owner sections.

`runs.create` takes an optional `finish_marker` (boolean, omitted when false): the queue sets it for a single-agent run that ends when the agent writes an empty `.tmgr/done` in its folder, and the runner adds a "When you are done" block to `task.md` and a one-line note to the prompt. It is ignored for a run with a `team`.

`runs.create` takes an optional `prompt` (text, at most 100000 bytes, control characters other than newline and tab removed): the first prompt of a plain run, passed to the agent the way a task prompt is. A prompt that starts with `-` is refused, so it is never read as a flag. It needs `client: "cli"` or `"desktop"`, is ignored when the session is resumed, and is refused together with `task`, `bundle`, `origin`, `team`, `cloud`, `external_resume`, `clones`, `start_key`, `agent_work` or `unattended`. `parent_session` (CLI and desktop only) names an exited session of the same task, or of no task when the new run has none.

`author.kind` is `human`, `agent` or `other`; `author.id`, `persona` and
`launch.model` are optional and are omitted, never `null`. `v` is `1`. Unknown
fields are ignored by the runner and are not part of the hash, so send only
the fields above.

Canonical form and hash. `bundle_sha256` is the lowercase hex SHA-256 of the
canonical JSON of the bundle, which is RFC 8785 (JCS) for the value kinds used
here (there are no floats):

- UTF-8, no whitespace between tokens.
- Object keys sorted at every level by their UTF-16 code units (this is what
  the default `Array.prototype.sort` does on JavaScript strings); `null` and
  unknown fields are not present.
- Integers as plain decimals (`-3`, `412`), no exponent, no `+`, no leading zeros.
- Strings: `"` is `\"`, `\` is `\\`, backspace, form feed, line feed, carriage
  return and tab are `\b \f \n \r \t`, every other character below U+0020 is
  `\u00xx` with lowercase hex. Everything else (including `/`, U+007F,
  U+2028 and non-BMP characters) is written as the literal UTF-8 character.

In TypeScript, `JSON.stringify` of an object whose keys were inserted in sorted
order gives exactly this string.

Test vector (also `tests/fixtures/run_bundle.json` and
`run_bundle.canonical.json`). The input bundle is the one above with these
values: title `Fix "login" é中`; body is `line1`, LF, `line2`, TAB, `tab`, a
backslash, a space, U+0001, a space, U+1F600, a space and `</x>`; persona
`instructions_sha256` is 64 zeros; the comment has no `author.id`. Canonical
string (one line):

```
{"author":{"id":"u_7","kind":"human"},"body":"line1\nline2\ttab\\ \u0001 😀 </x>","comments":[{"at":"2026-10-13T09:30:00Z","author":{"kind":"agent"},"body":"ok"}],"launch":{"cwd":"/work/project","model":"gpt-5","profile":"codex"},"pages":[{"body":"# Spec","id":9,"revision":3,"title":"Spec"}],"persona":{"instructions_sha256":"0000000000000000000000000000000000000000000000000000000000000000","name":"reviewer"},"task":{"task_id":412,"workspace_id":-3},"title":"Fix \"login\" é中","v":1}
```

`bundle_sha256` = `e60cccd72a81bef205b9173f8fcbc6a8a7ef1608122a3662495f4b775c1bbf76`.

The runner refuses the request with `invalid_params` when:

- the recomputed hash differs from `bundle_sha256` ("bundle hash mismatch"), or
  `v` is not 1, or only one of the two fields is present;
- `launch.profile`, `launch.model` or `task` differ from the request's
  `profile`, `model` or `task`, or `launch.cwd` is not the canonical (symlinks
  resolved) path of the request's `cwd`. An approved hash therefore cannot be
  replayed with other parameters;
- a metadata field is malformed. These fields are rendered outside the data
  blocks or identify who wrote what, so they are checked and the message names
  the field: `persona.instructions_sha256` is exactly 64 lowercase hex
  characters; `persona.name` is 1 to 64 characters without control characters;
  every `comments[].at` is `YYYY-MM-DDTHH:MM:SS`, an optional `.` and 1 to 9
  digits, then `Z` or `+HH:MM` / `-HH:MM` (upper-case `T` and `Z`, no other
  text); `author.id` and `comments[].author.id`, when present, are at most 64
  characters of `A-Za-z0-9._:@-`;
- `<cwd>/.tmgr` is a symlink or not a directory, or `<cwd>/.tmgr/task.md` is a
  symlink;
- `cwd` is not the directory that was approved by the time the agent starts
  (for example a symlink was swapped after the check).

The task reaches the agent as its first message. When the profile's `[prompt]`
mode is `arg`, the runner renders the bundle into a prompt and puts it in place
of `{prompt}` in the profile's prompt arguments; `argv_sha256` covers those
arguments with `{prompt}` itself in that place. A resumed run gets no prompt. The prompt starts with
`Task: <title>`, then the description and the comments when there are any (a
title alone is a complete task). Comments not written by a confirmed human are
fenced between `<<<` `>>>` markers and labelled untrusted. Control characters
and bidirectional overrides are dropped. Above 4000 bytes the prompt is cut to
the title and a pointer to `.tmgr/task.md`, which always holds the full task.
`SessionInfo.argv` shows the prompt argument as `<task prompt, N bytes>`. The
prompt is a fixed function of the bundle, so `bundle_sha256` binds it: what was
approved is what the agent receives. It holds nothing but the bundle's own text.
The agent's process arguments still carry it, so other local accounts that can
list processes can read it.

The runner opens the canonical `cwd` once, writes `.tmgr` only relative to the
opened directory and hands that open directory to the process that starts the
agent, which changes into it by descriptor. The agent therefore starts in the
exact directory object that was opened, also when the path is renamed or
replaced after the check; the path is not looked up again. A path that no
longer names the same directory before the hand-over is refused with
`invalid_params`, before anything starts.

`busy` ("another session is running in this directory") is decided by the
identity (`st_dev`, `st_ino`) of the opened directory, not by its path, and
holds until the session exits or is removed, on `runs.create` and on
`sessions.create`: a bundled run is refused while any session that has not
exited uses that directory, and a session without a bundle is refused while a
bundled one does. A renamed directory is still the same directory; a new
directory at an old path is not. One run can never replace the context file of
another. All other validation (profile, secrets, sandbox) happens before
`task.md` is written. If the launch still fails after `task.md` was written
(for example the shim does not start), the previous content is restored, or the
new file is removed when there was none.

A canonical bundle over 1 MiB is refused with `limit`.

What the runner guarantees: the session runs exactly that bundle. It stores the
canonical bundle as `bundle.json` in the session directory (mode 0600, before
the agent starts), writes the agent's context file `<cwd>/.tmgr/task.md` from
that stored copy only, never fetches task data from anywhere and never puts
bundle text into the agent's argv or environment. `task.md` holds a fixed
instruction header, then the title, body, comments and pages, each inside
`<<<task data (untrusted, from <author kind>)>>>` ... `<<<end of task data>>>`
markers (the persona name and page titles are inside blocks too), and ends with
`bundle sha256: <hash>`. If `<cwd>/.git` is a directory, `.tmgr/` is added to
`.git/info/exclude`, unless that file is not a plain single-link file of at most
1 MiB (then it is left alone). `SessionInfo.bundle_sha256` echoes
the hash (kept in `meta.json`, so it survives runner restarts).

What it does not guarantee: whether a human approved the bundle. The runner
cannot know that. Approval is the client's job: for example the desktop shows
the bundle and sends it only after the human confirms.

```json
{"type":"request","id":7,"method":"runs.create","params":{"profile":"codex","task":{"workspace_id":-3,"task_id":412},"cwd":"/work/project","model":"gpt-5","cols":120,"rows":40}}
```

### Work log

Capability `worklog.v1`. The runner keeps an append-only log per session,
`<runner dir>/sessions/<id>/worklog.jsonl` (mode 0600, opened without following
links, at most 10,000 lines and 1 MiB; past that it records one `truncated`
event and stops, keeping a reserve for the `exited` and `commits` events).
`sessions.worklog {id}` returns a `WorkSummary` computed from it:

- `started_ms`, `ended_ms` (absent while running), `duration_s` (to now while running);
- `working_s`, `waiting_s`, `blocked_s`: time in each state, from state changes (`starting` is not counted);
- `branch`, `commits` (`[{sha, subject}]`, oldest first): git facts, absent when the folder is not a git work tree or git fails;
- `exit` `{code, signal}`; `task`, `profile`, `agent` (the profile id, else the program's base name); `trust` (`pre-accepted` when the run used `trust_cwd`, `remembered` for a trusted folder, `repo` for a clone of a trusted repository); `truncated`.

Events: `created` (time, profile, program base name, task, bundle hash, branch
and head of the folder, and `trust: "pre-accepted"` when `trust_cwd` was used), `state` (time, kind, and for `waiting` the detail
`bell`, for `blocked` the rule id only), `exited`, `commits` (the final branch
and the commits made since the head at creation, at most 200, each subject at
most 200 characters with control and bidi characters replaced), `truncated`.
Git is run by the runner with the hook-free environment of its worktree
module (no hooks, no filters, no user config).

Never recorded: terminal output, input, screen content, the text of a question
or prompt, environment, or file contents. The log survives runner restarts;
adopting a session does not repeat `created`. A session started before this
capability has no log and returns an all-zero summary.

### Checkpoints

Capability `checkpoints.v1`, off unless a CLI client calls `checkpoints.enable`. After a turn the
runner stores the session's working folder as a commit on `refs/tmgr/checkpoints/<id>/<n>`, built
with plumbing and a temporary index: the user's branch, index and stash are not touched. Ignored
files are not stored. The runner's own manifest in the session folder decides what exists; a ref
that disagrees with it is refused.

`checkpoints.restore` answers `busy` while the session is working or while another live session
runs in the same folder, a subfolder or a parent folder. It first stores the current state (`before_n`), then only overwrites or
deletes what that stored state holds byte for byte; anything else is listed in `left_as_is`.
Link workers and the desktop get `forbidden`: restore is started on this machine, from the CLI.

### Chat

Capability `chat.v1`. The message envelope (`ChatMessage`, `RoomRef`, `RoomInfo`, the request types and
the limits) is defined in `tmgr_ipc::chat` and `schema/chat-message.v1.json`. Only `cli` and `desktop`
may call these methods; every other client gets `forbidden`.

A message the runner captures from a session transcript (author `session`) carries `session_meta`: `agent`
(the profile), `model` (the model id from the transcript, printable ASCII, at most 128 characters),
`context_tokens` (the input tokens of the last turn) and `context_pct` (0 to 100: tokens over the context
window the agent reported, or over 200 000, or 1 000 000 above that, when it reports none). Every field is
optional; readers ignore the object when it is absent.

| method | params | result |
|---|---|---|
| `chat.rooms` | none | `[RoomInfo]`: the Manager room first (title `Manager`; `state`, `context_pct` and `context_tokens` of the live manager session when there is one), then one room per session except the live manager, whose terminal replies are posted into the Manager room as `session` messages without `reply_to` and whose user turns are not mirrored (`title` = session name, `agent` = profile, `state`, `last`, and `context_pct`, `context_tokens` and `model` once the session's transcript reported usage since the runner started) |
| `chat.history` | `{room, before?, limit?}` (`limit` 1 to 500, default 500) | `[ChatMessage]`, oldest first: the newest `limit` messages stored before the message with id `before` (store order, which is append order); an unknown `before` gives `[]` |
| `chat.send` | `{room, body_md, reply_to?, attachments?, id?, ts_ms?}` | the stored `ChatMessage`, author `{kind: owner, id: owner}` |
| `chat.blob.put` | `{v: 1, sha256, size, part, parts, data}` | `{have, parts}` while parts are missing, `{complete: true}` once the file is stored |
| `chat.search` | `{query, room?, limit?}` (`query` 1 to 256 characters, `limit` default 50, max 500) | `[ChatMessage]`, newest first: case-insensitive substring match on `body_md` |
| `chat.subscribe` | none | `null`; the connection then also receives a `chat` push |
| `chat.relay_self` | none | `{id, device_id, fp}`: this runner's machine id and its public relay device id and fingerprint (no keys); `not_found` when the relay is not configured or not ready |

Errors: `invalid_params` (bad room, a room of another machine, empty body, bad query, `id` without `ts_ms`, bad `id`), `not_found` (an ingest into a session room this runner does not know), `conflict` (an ingest id that already holds different content), `busy` (the target
session is not at its prompt), `forbidden`.

`room.machine` must be this runner's machine id: `mx_` and 16 lowercase hex characters, created once in the
runner directory (`machine.id`, mode 0600). It is never a hostname. Read it from the `room.machine` of the first
`chat.rooms` entry.

`chat.blob.put` uploads one part of a file attached to a message (`chat.blob.v1`). `sha256` is the lowercase hex digest of the whole file, `size` its length (1 byte to 25 MiB), `parts` must equal `ceil(size / 737280)`, `part` is the zero-based index, and `data` is the standard base64 of exactly 737280 bytes (the last part holds the rest), so one request stays under the 1 MiB frame limit. Parts may arrive in any order and may be repeated; the runner verifies the whole file against `sha256` when the last part lands and refuses parts that disagree on `size` or `parts` with stored ones. Send the parts first, then `chat.send` with `attachments` carrying the same `sha256`. When a message with attachments is handed to a session, the runner copies each stored file into the session's `tmgr-attachments/` directory (in its private home, or a private runner folder for a run without the sandbox) and appends `Attached files:` lines with the paths to the text the agent receives; a file that is not stored shows as `(not received)`. The stored message is not changed.

With `id` (a valid `m_<32hex>`, and then `ts_ms` is required) `chat.send` ingests an owner message written on
another device. The id and `ts_ms` are kept, the author is always `{kind: owner, id: owner}`. A `device` sent over IPC is ignored: only the in-process relay pump sets
`author.device`, from the sender device it verified, and the message is always stored first. The room must belong to this machine; the Manager room always exists, a
session room exists when the session is known or the room already has a file (`not_found` otherwise). The same id with
the same stored message (every field) is a no-op that returns the stored message and delivers nothing; the same id with
other content is `conflict` and never overwrites. The message is delivered only when `now - ts_ms` is at most 10 minutes and
`ts_ms` is at most 5 minutes ahead of `now`; otherwise a system message ("Not delivered: older than 10 minutes." or a
clock-skew note) is stored after it. For a message from the relay pump, `ts_ms` was already moved to this machine's clock by the relay engine's per-sender clock offset (the highest `sent_at - now` seen in the last 30 minutes, at most 24 hours; 0 before the first message from that sender), so a sender whose clock is off still passes these checks; the first message from a sender with no history is checked unconverted. A live target that is not at its prompt gets the system message "Not delivered: the
session was busy." and nothing is queued; ingest never answers `busy`. Without a live target the message is only stored.
In process the relay pump calls `Chat::ingest`.

Remote Manager rooms (top manager only, `manager-top` set): `chat.rooms` appends one Manager room per paired machine
(`room.machine` = that machine's id, `title` = its label or id, no `last`). `chat.history` of such a room asks the
machine's `room` op (`limit` 1 to 200, default 200; `before` is not supported and gives `[]`; messages of other rooms or
invalid ones are dropped). `chat.send` asks its `ask` op with `body_md` and returns a local echo (author
`{kind: manager, id: manager, machine: <this machine>}`, the id the remote machine stored); attachments are refused.
Both go through the manager-remote allow list of the remote machine: a refusal reads `machine <id> refused <op>: allow
"<op>" in manager-remote.toml on that machine`. Other errors are `invalid_params` with `unknown machine <id>; known: ...`,
`relay not ready` or the remote error text. On a runner that is not top, and for a session room of another machine, a room of another
machine is still `invalid_params` ("the room belongs to another machine" on a non-top).

`chat.send` without `id` into a session room whose session is running types the body at the agent's prompt as a bracketed
paste followed by Enter, exactly like `sessions.send` with `at_prompt`, and answers `busy` (and stores nothing)
when the agent is not at its prompt. A room without a running target (an exited session, or the Manager room
while no manager session exists) only stores the message.

The `chat` push is `{"type":"chat","message":{ChatMessage}}`, sent for every message appended to any room
(owner messages, captured agent replies). A client that does not know the type decodes it as an unknown push
and ignores it. A subscriber that falls behind is closed with `closed: client too slow`.

Replies of Claude Code and Codex sessions are read from the agent's own conversation log about every 3
seconds and stored as messages from `{kind: session, id: <session id>, machine, agent: <profile>}`: only
visible assistant text, never tool calls or results. A log that was already larger than 256 KiB when the
runner first saw it is not replayed. Sessions of other agents are not captured.

Storage is `chat/<room key>.jsonl` in the runner directory (mode 0700, files 0600), one JSON message per
line, append only. The key is `manager` or `session-<session id>`. At 16 MiB a room file is renamed to
`<key>.jsonl.1` (one older generation is kept) and a new file starts. Unreadable lines are skipped.

## 6. Who may call what

`hello.client` is declared by the client. The runner uses it for these rules:
`sessions.create` (raw argv) and `origin.revoke` are accepted only from `cli`;
`origin` on `runs.create` is accepted only from, and required of, `worker`
(section 5, "Link-originated runs"); other clients get `forbidden`. `sessions.text` is refused for `worker`;
`autopilot.start` and `orchestrator.create` only from `cli`; `chat.*` only from `cli` and `desktop`; `autopilot.status`, `autopilot.control` and `autopilot.log` from `cli` and `desktop` (the `suspend` command from `cli` only); `trust_cwd` and `trust_via_repo` on `runs.create` are accepted only from `cli` (section 5,
"Pre-accepted folder trust", "Trusted folders"). This is a guardrail so well-behaved clients cannot start arbitrary
commands by mistake (the desktop webview must never be able to send argv). It
is not a security boundary: anything that holds `client.secret` as the same user
can claim to be `cli`. The real boundary is that agents run in a sandbox that
cannot read the runner directory.

## 7. Attach streaming

After `attach.open`, for every watched session the runner sends, at most about
every 33 ms and only when the screen changed:

- first, one `event` with the current state;
- `screen` pushes.

`event` also arrives for state changes of every session, watched or not:

```json
{"type":"event","session":"s_abcdefghij","state":{"kind":"blocked","since_ms":1760000001000,"detail":"Allow command?","exit_code":null},"task":{"workspace_id":-3,"task_id":412}}
```

`screen` is `ScreenPush`: `{session, full, snapshot, lines, cursor, modes,
scroll_offset, scroll_max}`.

- `full: true`: `snapshot` is a Snapshot (new size or first push). Its `l` may
  hold only the first rows; allocate `r` empty lines and put them in.
- `full: false`: `lines` is a list of `[row_index, Line]` for changed rows.
- Every push carries the current `cursor` (`[row, col]` or null), `modes`
  (may be null: keep the old ones), `scroll_offset` and `scroll_max`.
- A screen that does not fit in 256 KiB (a whole JSON frame) is split into
  several pushes: a `full` push followed by `full: false` pushes with the
  remaining rows. Apply them in order.

Stream messages from the client (all need the session in the watched list,
otherwise they are ignored):

| message | effect |
|---|---|
| `{"type":"resize","session":id,"cols":n,"rows":n}` | resizes the PTY; the latest viewer to resize or type owns the size |
| `{"type":"scroll","session":id,"offset":n}` | scrollback offset in lines from the bottom; the next pushes show that view |
| `{"type":"focus","session":id}` | reserved, no effect yet |
| `{"type":"attention","session":id\|null,"window":bool}` | cap `human_time.v1`; the viewer is present now, its focused pane is `session` (null: none) and its terminal window is focused when `window` (default true); `session` may be omitted; ignored unless `client` is `cli`; it is also an activity ping (see "Human time") |
| `{"type":"detach"}` | the runner closes the connection |

Input is the binary frame with tag 3 (section 2); it is accepted for any
existing session.

The runner may send `{"type":"closed","reason":"..."}` and then close
(`client too slow`, `bad message`, `input frame too large`,
`unexpected binary frame`). A client that does not read for about 10 seconds or
lets 256 messages pile up is dropped.

### Human time

Cap `human_time.v1`. The runner measures how long a person actually watches or
types into each session. A client sends `attention` only when the welcome caps
contain the cap (older runners close the connection on an unknown message). It
sends one right after `attach.open`, on every pane focus change, on terminal
window focus in or out, and as a heartbeat (at most once per 5 s) on any user
action that is not input forwarded to a session. `attention` is the only
message that is not tied to the watched list.

Rules:

- A connection counts only if `hello.client` is `cli` and it has sent at least
  one `attention`. `sessions.send`, other clients and silent connections never count.
- A session is active while some counting connection has it focused, its
  window focused, and its last activity (an `attention`, an input frame to the
  focused session, or a `scroll`) is less than the idle limit ago (default
  120 s, 30 to 3600). Several connections on one session count once.
- Active time is typing for 10 s after an input frame, otherwise reading.
- Time is cut into chunks of at most 60 s. A chunk ends when the session stops
  being active, at 60 s, and when the runner stops. A computer sleep is not
  counted. Chunks under one counted second are dropped.

Files, under `<runner dir>/time/` (directory 0700, files 0600). On Linux, when
the runner dir is the default `$XDG_RUNTIME_DIR/tmgr` (no `TMGR_RUNNER_DIR`),
that is a tmpfs wiped at logout, so the files live in
`$XDG_STATE_HOME/tmgr/time` (default `~/.local/state/tmgr/time`) instead:

| file | writer | content |
|---|---|---|
| `intervals.jsonl` | runner | one object per closed chunk: `id` (`<session>-<start_s>`), `session`, `name`, `cwd`, `task?`, `start_s`, `end_s`, `typing_s`, `reading_s`; records older than 45 days are removed at runner start |
| `settings.json` | CLI | `{"sync":bool,"idle_s":n}`, read by the runner at most every 5 s |
| `sent.jsonl`, `send.json`, `send.lock` | CLI | upload bookkeeping |

`name`, `cwd` and `task` are copied when the chunk opens. Keystrokes, input
bytes and screen content are never recorded. The runner never sends anything to
a server; the CLI uploads the records.

### Snapshot JSON

`{"r":rows,"c":cols,"l":[Line],"cu":[row,col]|null,"m":Modes,"so":scroll_offset,"sm":scroll_max}`.

- `Line` is an array of spans `{"a":{"f":fg,"b":bg,"x":flags},"t":"text"}`.
  Rows may be shorter than `c`; missing cells are blank in the default attribute.
- Color: `null` default, an integer 0 to 255 for the 256-color palette, or
  `[r,g,b]`.
- Flags (`x`, bit set): 1 bold, 2 dim, 4 italic, 8 underline, 16 inverse.
- `Modes` `m`: `{"ac":app cursor keys,"bp":bracketed paste,"alt":alternate screen,"hc":cursor hidden}`.

```json
{"type":"screen","session":"s_abcdefghij","full":true,"snapshot":{"r":1,"c":4,"l":[[{"a":{"f":null,"b":null,"x":0},"t":"hi"}]],"cu":[0,2],"m":{"ac":false,"bp":false,"alt":false,"hc":false},"so":0,"sm":0},"lines":[],"cursor":[0,2],"modes":{"ac":false,"bp":false,"alt":false,"hc":false},"scroll_offset":0,"scroll_max":0}
```

### Viewers never see raw bytes

The runner feeds the agent's output into its own terminal model and publishes
only this snapshot: printable text, color and attributes, cursor position and a
few modes. Clipboard (OSC 52), hyperlinks, title changes, DCS, C1 controls and
any query that would make the viewer's terminal answer are never forwarded, so
a hostile agent cannot act on the viewer's terminal. Replies to terminal
queries are produced by the runner itself.

## 8. Limits

| what | limit |
|---|---|
| JSON frame / binary frame | 1 MiB / 4 MiB |
| one `screen` push | 256 KiB |
| input frame, `sessions.send` text | 64 KiB |
| terminal size | 500 x 200 |
| scrollback | 5000 lines |
| concurrent connections / unfinished handshakes | 64 / 16 |
| handshake time | 5 s |
| sessions per runner | 16 by default |

A full runner answers `limit`.

## 9. Error codes

`invalid_params`, `unknown_method`, `unknown_profile`, `not_found`,
`not_running`, `busy`, `limit`, `sandbox`, `unauthorized`, `proto_mismatch`,
`internal`, `forbidden`. New codes may appear later.

**Remote spawn and switch (top manager).** On the top manager `propose_spawn`, `spawn_session` and `switch_agent` accept `machine`; when it is this machine the call runs locally as before, a foreign id goes over the manager RPC and a non-top manager gets `not available`. `propose_spawn` sends `spawn_challenge` with `{profile, cwd, prompt?, name?, task?, permission_mode?}`; `spawn_session` sends `spawn` with the same keys plus a required `card_id`; `switch_agent` sends `switch` with `{session, to}`. The `machine` key is removed before sending, unknown keys are refused, and `approval`, `no_sandbox`, `manager` or a missing `card_id` are refused on the top manager before anything is sent. A remote `prompt` is at most 1000 bytes, so the owner sees all of it. The target machine posts a Decision card that starts with "Proposed by the top manager on <label> (<mx id>)." and quotes the prompt; the owner approves it on the phone. Until then `spawn_session` fails with `this card is not approved yet`, so the top manager polls by retrying it; an expired, closed or foreign card fails with `this card has expired or is closed; propose the spawn again` and is not retried. A spawn needs a stored approval for that card; an inline approval is refused, and a remote card is bound to the proposer (`remote:<mx id>`): another proposer or a card from before a runner restart is refused for the remote caller (propose again), and a local manager is refused while the target runner still knows who proposed the card; the binding is kept in memory only, so after a target restart a card the owner approves is usable by a local manager on that machine with the exact approved arguments. A card proposed by a local manager is usable by any local manager on that machine. A remote prompt may not contain `\r`, U+2028, U+2029, U+0085, control characters other than newline and tab, or invisible formatting characters. The Decision card quotes the prompt in a code fence and carries optional `proposed_by` and `prompt` fields in its `spawn` object. If the session fails to start after the approval, the owner sees `The session did not start: <reason>. Ask for a new card.` `switch` works only for sessions the same remote proposer spawned (recorded in the session as `spawned_by`, which survives a restart and carries over to the session a switch creates), and every other target gets one generic refusal. A remote cwd must be a trusted folder on the target; the error is the same for every cause (`cwd is not a trusted folder on this machine`). The target's `manager-remote.toml` is default-deny: `spawn_challenge`, `spawn` and `switch` must each be allowed there, and a refusal tells the top manager which op to allow. `allow` is per op for every accepted machine, not per sender, and the target serves spawn and switch on its serve thread, so other RPCs wait while one runs. The top manager runs the foreign `switch_agent`, `spawn_session` and `propose_spawn` calls off its spool loop. The audit log records `card_id` for `spawn_challenge` and `spawn` and the spawned session id. Local limits stay: 4 spawns and 2 switches per minute.
