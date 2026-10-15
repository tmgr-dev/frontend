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
| `permission_mode.v1` | `permission_mode` on `runs.create`, `SessionInfo` and `launch` of a bundle (section 5, "Queue capabilities") |
| `start_key.v1` | `start_key` on `runs.create`; `runs.lookup` (section 5, "Queue capabilities") |
| `run_status.v1` | `runs.status` (section 5, "Queue capabilities") |
| `clone_dir.v1` | `clone_of` on `runs.create` (section 5, "Queue capabilities") |
| `session_text.v1` | `sessions.text` (section 5, "Session text") |
| `trust_cwd.v1` | `trust_cwd` on `runs.create`; `trust` in `WorkSummary` (section 5, "Pre-accepted folder trust") |
| `agent_work.v1` | `runs.create` `agent_work` (section 5, "Queue capabilities") |
| `exe_pin.v1` | `exe_pin` on `runs.create` (section 5, "Pinned executable") |
| `argv_pin.v1` | `argv_sha256` on `runs.create` (section 5, "Pinned executable") |

## 5. Messages after the handshake

Client to server (JSON, `type` tag): `request`, `resize`, `scroll`, `focus`,
`detach`. Server to client: `response`, `screen`, `event`, `closed`.

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
| `runs.create` | `RunCreateReq` (optional `bundle` + `bundle_sha256`, `cwd_pin`, `origin`, `trust_cwd`, `exe_pin`, `argv_sha256`) | `SessionInfo` | `unknown_profile`, `sandbox`, `limit`, `invalid_params`, `busy`, `forbidden` | any; `client: "worker"` must send `origin`, every other client must not |
| `origin.revoke` | `{link, generation}` | `{killed, unconfirmed}` | `forbidden` | `client: "cli"` only |
| `candidates.list` | `{}` | `[{profile, source, created_ms, session}]` | | any client; cap `candidates.v1` |
| `runs.restore` | `{id}` (the interrupted session) | `{session: SessionInfo, resumed}` | `not_found`, `busy`, `forbidden`, `unknown_profile`, `sandbox`, `limit` | `client: "cli"` only |
| `runs.lookup` | `{start_key}` | `SessionInfo` | `not_found` | any client; cap `start_key.v1` |
| `runs.status` | `{id}` | `{id, alive, state, exit_code, signal, started_ms, ended_ms}` | `not_found` | any client; cap `run_status.v1` |
| `sessions.text` | `{id, lines?}` (`lines` 1 to 10000, default the screen height) | `{id, rows, cols, lines, scroll_max}` | `not_found`, `invalid_params`, `forbidden` | any client except `worker`; cap `session_text.v1` |
| `sessions.kill` | `{id, signal}` (`term`, `kill`, `int`, `hup`) | `null` | `not_found`, `not_running` | any |
| `sessions.send` | `{id, text}` (text at most 64 KiB, typed as input) | `null` | `not_found`, `not_running`, `busy` | any |
| `sessions.remove` | `{id}` (exited sessions only) | `null` | `not_found`, `busy` | any |
| `sessions.worklog` | `{id}` | `WorkSummary` | `not_found` | any |
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
cols, rows, shim_pid, child_pid, task?, bundle_sha256?, origin?}`. `task`, `bundle_sha256` and `origin` are absent when not set.

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
7 days, records with an `origin` (a link run is re-requested by its leader) and bypass
runs; then it sends the recorded launch through the same path as `runs.create`, with
the stored bundle and `cwd_pin`, so the bundle hash and the folder identity are checked
again (a replaced folder answers `invalid_params` "approved folder changed"). When the
old run's home still exists it is moved to the new session and, if the profile has
`[resume] args`, the agent starts with them (`resumed: true`); otherwise it starts fresh
with the same bundle. On success the old record and the old exited session are removed;
on failure the record is put back. The runner never restores on its own: each restore
is one explicit local request (`tmgr agents restore`).

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

**`permission_mode.v1`.** `runs.create` takes an optional `permission_mode`: `ask`, `accept-edits` or `plan` (there is no bypass). The profile maps each mode it supports to arguments (`permission_modes` table) and the runner appends them after the profile's own arguments. A request names a mode the profile has no entry for, or a profile that has a `permission_modes` table but no entry for `ask` gets no mode: `invalid_params` ("profile X cannot run in mode Y"). A profile without any `permission_modes` table keeps its old behavior when no mode is requested (no extra arguments, no recorded mode). The effective mode (request value, else `ask`) is stored and returned as `SessionInfo.permission_mode`. `launch.permission_mode` of a bundle is optional and hashed only when present; when the request carries a bundle, the bundle's value must equal the effective mode, and an absent value (old bundles) is accepted only when the effective mode is `ask` or none. `sessions.create` is not affected.

**`start_key.v1`.** `runs.create` takes an optional `start_key` (`[A-Za-z0-9._:-]`, 1 to 64 characters). The runner stores it with a hash of the other parameters (everything except `cols`, `rows` and `start_key`). A second `runs.create` with the same key and the same hash returns the existing `SessionInfo` in whatever state it is, and starts nothing; the same key with other parameters is `busy` ("start key already used for a different request"). Keys survive a runner restart and are forgotten when the session is removed. `runs.lookup {start_key}` returns the `SessionInfo` or `not_found`.

**`run_status.v1`.** `runs.status {id}` returns `{id, alive, state, exit_code, signal, started_ms, ended_ms}`: `state` is the state kind, `alive` is true while the shim is reachable and the child has not exited, `exit_code`, `signal` and `ended_ms` are `null` until the child exits, `started_ms` is `created_ms`. It carries no screen.

**`clone_dir.v1`.** `runs.create` takes an optional `clone_of`, the absolute path of a repository. Before the sandbox is prepared the runner checks, walking descriptors without following any symlink, that the repository is a work tree root with a real `.git` directory, that the canonical `cwd` lies strictly inside `<repo parent>/<repo name>-worktrees/` (any depth), that `cwd/.git` is a real directory (not a worktree file) and that `cwd/.git/objects/info/alternates`, if present (at most 4 KiB), lists only `<repo>/.git/objects`. Any violation is `invalid_params`. The sandbox then gets read-only access to `<repo>/.git/objects` and nothing else of the repository; writes stay limited to `cwd` and the per-run home.

**`agent_work.v1`.** `runs.create` takes an optional `agent_work: true` (absent means false). The runner then records the run as Agent work of its task in the local workspace: it needs a `task` of a local workspace (negative `workspace_id`; a cloud task is `invalid_params`, owner decision 16a.11), a profile with a `permission_modes` table (otherwise `invalid_params` naming the profile), and is `forbidden` for link runs. The runner calls its own executable as `<self> __agent-work` with one JSON request on stdin (`op` `start`, `update` or `finish`) and keeps the run id with the session: `start` when the session is created (agent = profile id, model, branch, session id), `update` with the branch and commits from the work log at most every 10 minutes, and `finish` once when the child has exited: `succeeded` for exit code 0, `cancelled` after `sessions.kill`, a revocation or a session lost while the runner was down, otherwise `failed`, with a one-line summary and the commits. Failed calls are retried and never block the session; a session that ended while the runner was down is finished after the restart. A `start_key` replay returns the existing session and opens no second run. The flag is part of the start-key hash.

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

- take the final argv exactly as the runner executes it for a new start: the profile `command` and `args`, then the model arguments (when `model` is set), then the permission-mode arguments (when the profile has `permission_modes`), in that order, without the `resume` arguments;
- replace `argv[0]` with the canonical executable path (the `exe_pin.path` that the pin check verifies);
- feed SHA-256 with each element's UTF-8 bytes followed by a single `0x00` byte, nothing else;
- write the digest as lowercase hex.

`tmgr_ipc::api::argv_sha256` implements this. Known vector: `["a", "b"]` gives `8fb20ef63ced4145fc2e983ffe597d1dcff39154c3bf21f0fa9dde6a0c50fdc9`. After the `exe_pin` check, the runner computes the digest of the argv it built and compares; a difference is `forbidden` ("the agent command line differs from the approved one"). A `start_key` replay still returns the existing session first, and `argv_sha256` is part of the start-key hash. The digest is stored with the restore record; `runs.restore` rebuilds the start-form argv from the current profile, checks it the same way (`forbidden` on a mismatch, the record is kept) and only then adds the `resume` arguments for the exec. Because the digest does not cover the `resume` arguments, a run created with `argv_sha256` also stores the profile's `[resume]` declaration (the `args` list and the `at` position, or its absence); `runs.restore` requires the current profile's declaration to be equal, otherwise it is `forbidden` ("the agent's resume arguments changed since the run was approved") and the record is kept.

### Pre-accepted folder trust

**`trust_cwd.v1`.** `runs.create` takes an optional `trust_cwd` (`true`). It asks the runner to pre-accept the agent's own folder-trust prompt for the session's canonical `cwd` and nothing else, for this run only. The profile declares how in a `[trust]` table (`file`, a relative path inside the per-run home, and `path`, 1 to 8 JSON keys of which `{cwd}` stands for the whole canonical `cwd`); the runner merges `true` at that path into the copy of the file in the per-run home after the seeds are written, creating the file and objects when missing. The owner's real files are never touched. A profile without `[trust]` gives `invalid_params`. `trust_cwd` is accepted only from `client: "cli"` (others get `forbidden`) and is `invalid_params` together with `start_key`, `clone_of` or `origin`, so it can never come from the queue, a link or a follower. A restored run does not carry it. The work log records `trust: "pre-accepted"` on the `created` event and in `WorkSummary.trust` (section 5, "Work log"); both are absent otherwise.

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

`launch.permission_mode` (`ask`, `accept-edits` or `plan`) is optional, omitted when absent, and part of the hash when present.

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
of `{prompt}` in the profile's prompt arguments, after the command line that
`argv_sha256` covers. A resumed run gets no prompt. The prompt starts with
`Task: <title>`, then the description and the comments when there are any (a
title alone is a complete task). Comments not written by a confirmed human are
fenced between `<<<` `>>>` markers and labelled untrusted. Control characters
and bidirectional overrides are dropped. Above 16 KiB the prompt is cut to the
title and a pointer to `.tmgr/task.md`, which always holds the full task. The
prompt is a fixed function of the bundle, so `bundle_sha256` binds it: what was
approved is what the agent receives. It holds nothing but the bundle's own text.
The prompt is an argument, so other local accounts that can list processes can
read it.

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
- `exit` `{code, signal}`; `task`, `profile`, `agent` (the profile id, else the program's base name); `trust` (`pre-accepted`, only when the run used `trust_cwd`); `truncated`.

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

## 6. Who may call what

`hello.client` is declared by the client. The runner uses it for these rules:
`sessions.create` (raw argv) and `origin.revoke` are accepted only from `cli`;
`origin` on `runs.create` is accepted only from, and required of, `worker`
(section 5, "Link-originated runs"); other clients get `forbidden`. `sessions.text` is refused for `worker`;
`trust_cwd` on `runs.create` is accepted only from `cli` (section 5,
"Pre-accepted folder trust"). This is a guardrail so well-behaved clients cannot start arbitrary
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
| `{"type":"detach"}` | the runner closes the connection |

Input is the binary frame with tag 3 (section 2); it is accepted for any
existing session.

The runner may send `{"type":"closed","reason":"..."}` and then close
(`client too slow`, `bad message`, `input frame too large`,
`unexpected binary frame`). A client that does not read for about 10 seconds or
lets 256 messages pile up is dropped.

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
