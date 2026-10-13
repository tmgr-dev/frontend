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
{"type":"welcome","proto":1,"runner_version":"0.3.0","caps":["sessions.v1","attach.v1","runs.create.v1","task_ref.v1","layout.v1"]}
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
| `runs.create` | `RunCreateReq` | `SessionInfo` | `unknown_profile`, `sandbox`, `limit` | any |
| `sessions.kill` | `{id, signal}` (`term`, `kill`, `int`, `hup`) | `null` | `not_found`, `not_running` | any |
| `sessions.send` | `{id, text}` (text at most 64 KiB, typed as input) | `null` | `not_found`, `not_running`, `busy` | any |
| `sessions.remove` | `{id}` (exited sessions only) | `null` | `not_found`, `busy` | any |
| `layout.get` | none | the stored JSON value or `null` | | any |
| `layout.set` | `{layout}` (any JSON, at most 64 KiB serialized) | `null` | | any |
| `attach.open` | `AttachParams` | `null`, then pushes start | `not_found` | any |
| `attach.set` | `AttachParams` | `null` | `not_found` | any |

`CreateReq`: `{argv:[str], cwd, name?, profile?, cols=80, rows=24, env_pass?:[str], task?}`.
argv has 1 to 64 entries, each at most 4096 bytes; argv[0] is an absolute path.
`sessions.send` answers `busy` when the input queue is full or the shim dropped earlier input; retry later.

`RunCreateReq`: `{profile, task?, cwd, name?, model?, cols=80, rows=24}`. The
runner loads the agent profile (built-in or the user's `agents` directory),
builds argv, resolves argv[0] through its own `PATH` and then follows the exact
same validation, sandbox and creation path as `sessions.create`. A missing
executable is `invalid_params`. A desktop webview never sends argv.

Both: `cwd` is an existing absolute directory (not `/`, the home directory or a
protected path); `name` is 1 to 64 printable characters; `cols`/`rows` are
clamped to 1..500 and 1..200.

`SessionInfo`: `{id, name, argv, cwd, profile: str|null, state, created_ms,
cols, rows, shim_pid, child_pid, task?}`. `task` is absent when not set.

`TaskRef`: `{workspace_id: i64, task_id: i64}`. Local workspaces have negative
ids. The runner treats it as opaque, stores it in the session's `meta.json` (it
survives runner restarts) and echoes it in `SessionInfo` and `Event`.

`state`: `{kind, since_ms, detail: str|null, exit_code: int|null}`, `kind` is one
of `starting`, `working`, `waiting`, `blocked`, `exited`.

`StatusInfo`: `{pid, version, proto, uptime_s, sessions}`.

`AttachParams`: `{sessions:[id] (at most 64), sizes?: {id: [cols, rows]}}`. It
replaces the watched list; ids must exist. `attach.open` also turns the
connection into a push stream (once). An empty list gives state events only.

```json
{"type":"request","id":7,"method":"runs.create","params":{"profile":"codex","task":{"workspace_id":-3,"task_id":412},"cwd":"/work/project","model":"gpt-5","cols":120,"rows":40}}
```

## 6. Who may call what

`hello.client` is declared by the client. The runner uses it for one rule:
`sessions.create` (raw argv) is accepted only from `cli`; other clients get
`forbidden`. This is a guardrail so well-behaved clients cannot start arbitrary
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
