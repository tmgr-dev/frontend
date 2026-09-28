# TMGR desktop local access socket

TM-298. The TMGR desktop app opens a unix socket so a local agent (Claude
Code, Codex, a hand-written companion) can act as a connected **persona**:
same REST shape as the cloud API, plus a small local-only surface (health,
whoami, an event stream, and an MCP endpoint). There is no TCP, no CORS, no
network involved — the socket only accepts connections from processes on this
machine.

Reference client (Node, zero dependencies): [`plugin-sdk/local-access/`](../plugin-sdk/local-access/README.md).

## The honest boundary

> The token keeps an agent inside this persona's permissions and signs
> everything it writes. It is **not** a lock against other programs on this
> Mac: anything running as you can read this workspace's file directly.

The socket is mode `0600` and only accepts connections from the same OS user.
Token checks and the persona permission gate protect against **agent mistakes
and prompt injection** — a misbehaving or manipulated agent is confined to one
persona's permissions in one local workspace. They are not a security boundary
against other programs running as the same user: those can already read
`workspace.db` and the OS Keychain (with the usual OS confirmation prompt).

## Socket path

Default: `<app data dir>/local-access/local-access.sock` (directory `0700`,
socket file `0600`).

- macOS: `~/Library/Application Support/dev.tmgr.desktop/local-access/local-access.sock`
- Linux: `${XDG_DATA_HOME:-~/.local/share}/dev.tmgr.desktop/local-access/local-access.sock`

If that path is longer than the platform's `sun_path` limit (104 bytes on
macOS), the app creates the socket under `$TMPDIR/tmgr-<uid>/` instead and
writes the actual path to `local-access/socket-path` (plain text) next to it.
A client should therefore resolve the socket path in this order:

1. `TMGR_LOCAL_SOCKET` env var (explicit override)
2. the contents of `<app data dir>/local-access/socket-path`, if that file exists
3. the default path above

Windows (named pipe) is out of scope for this stage.

## Issuing a token

**Settings → Personas → On this device → Connect an agent.** The owner picks
an enabled persona in the current local workspace, a label, and an expiry (1
to 365 days, default 90). The app issues a local persona token
(`tmgrl_` + 32 random bytes, base62) and stores only its hash; the secret goes
straight to the OS Keychain (service `dev.tmgr.local-access`, account = token
id) and is never shown in the UI, written to `workspace.db`, or logged. The
dialog offers "Copy token" (Rust copies the Keychain secret to the clipboard
directly) and "Copy MCP config" (a snippet with `--token-id`, no secret in it).
Tokens can be revoked individually or all at once per persona; enabling "Local
agent access" is required (off by default, first token issue turns it on).

## Headers

| Header | Rule |
|---|---|
| `X-Persona-Token: tmgrl_…` | required on every route except `GET /api/local/health` |
| `X-Persona-Token: tmgrp_…` (a cloud token) | rejected immediately: `401 CLOUD_TOKEN` |
| `X-Smart-Device-Token`, `Authorization`, any `X-TMGR-*` | `400 HUMAN_TOKEN` if present alongside a persona token — the actor is only ever the token, headers cannot inject one |

## Routes

REST routes are the same contract as the Java backend: `/api/…` paths,
snake_case, `{data}` envelope. The socket exposes the **persona whitelist**
below (closed by default — anything not listed is `403 ROUTE_NOT_ALLOWED`),
plus four local-only routes.

### Local-only routes

| Route | Response |
|---|---|
| `GET /api/local/health` | no token needed: `{ok, ready, app_version, api: "local-1"}`. With a token, adds `{token: "valid"\|<error code>, workspace: {id, code}, persona: {id, name}}` |
| `GET /api/local/whoami` | `{user_id, persona: {id, name, description, owner, prompt_version, workspace: {id, code, permissions}, skills: [{slug, title, when, version}]}, token: {id, prefix, expires_at}}` |
| `GET /api/local/events` | SSE — see below |
| `POST /mcp` | MCP, streamable HTTP, stateless JSON responses (no SSE) |

### Persona REST whitelist

Copied from `src/local/personaGate.ts`'s `PERSONA_WHITELIST`, plus two routes
added by a companion PR (`PUT comments/{id}`, `GET project_categories/{id}`).
Permission names are the same vocabulary the cloud persona gate uses.

| Method | Route | Permission |
|---|---|---|
| GET | `tasks` | `tasks:read` |
| POST | `tasks` | `tasks:write` |
| GET | `tasks/:id` | `tasks:read` |
| PATCH | `tasks/:id` | `tasks:write` |
| GET | `tasks/:id/comments` | `comments:read` |
| POST | `tasks/:id/comments` | `comments:write` |
| PUT | `comments/:id` | `comments:write` |
| DELETE | `comments/:id` | `comments:write` |
| POST | `comments/:id/reactions/toggle` | `comments:write` |
| GET | `tasks/:id/files` | `files:attachments` |
| GET | `files/:id` | `files:attachments` |
| GET | `files/:id/content` | `files:attachments` |
| GET | `workspaces/statuses` | `statuses:read` |
| GET | `project_categories` | `categories:read` |
| GET | `project_categories/:id` | `categories:read` |
| GET | `task-relation-types` | `relations:read` |
| GET | `tasks/:id/relations` | `relations:read` |
| POST | `tasks/:id/related-to/:otherId/with/:typeId` | `relations:write` |
| GET | `tasks/:id/agent-work` | `agent_work:read` |
| POST | `tasks/:id/agent-work` | `agent_work:write` |
| PATCH | `agent-work/:id` | `agent_work:write` |
| POST | `agent-work/:id/finish` | `agent_work:write` |

A persona can never delete a task (archive it via status instead), never
lists other personas' work, and never crosses into a different workspace than
the one its token is bound to — `workspace_id` in the query or body is
ignored, only the token's workspace is used.

## Error codes

Every error body is `{"message": "...", "code": "..."}`.

| HTTP | `code` | When |
|---|---|---|
| 400 | `BAD_REQUEST` | malformed request |
| 400 | `HUMAN_TOKEN` | a human token header is present alongside the persona token |
| 413 | `TOO_LARGE` | request body over 1 MB |
| 401 | `TOKEN_MISSING` | no `X-Persona-Token` |
| 401 | `CLOUD_TOKEN` | token has the `tmgrp_` (cloud) prefix |
| 401 | `TOKEN_INVALID` | bad format or unknown hash |
| 401 | `TOKEN_EXPIRED` | past `expires_at` |
| 401 | `TOKEN_REVOKED` | revoked |
| 401 | `PERSONA_UNKNOWN` / `PERSONA_ARCHIVED` / `PERSONA_DISABLED` | persona gate: not in this workspace's snapshot, archived, or disabled here |
| 401 | `OWNER_MISMATCH` | persona's owner is not the logged-in user |
| 403 | `ROUTE_NOT_ALLOWED` | route not in the whitelist |
| 403 | `PERMISSION_MISSING` | grant is missing the permission the route needs |
| 403 | `NOT_OWN` | acting on another actor's comment/agent-work |
| 403 | `WORKSPACE_MISMATCH` / `WORKSPACE_NOT_LOCAL` | token's workspace doesn't match, or isn't a local workspace |
| 404 / 409 / 422 | `NOT_FOUND` / `CONFLICT` / `UNPROCESSABLE` | as in the router; `409 WORKSPACE_GONE` when the local workspace folder was removed |
| 429 | `RATE_LIMITED` | over 50 GET/s or 10 non-GET/s for this token — response carries `Retry-After: 1` |
| 501 | `NOT_AVAILABLE_LOCALLY` | route doesn't exist in the local router (never falls through to the network) |
| 503 | `APP_NOT_READY` | window still loading, no user logged in, or local access disabled — carries `Retry-After: 2` |
| 504 | `TIMEOUT` | main window didn't answer within 15s |
| — | `APP_NOT_RUNNING` | client-side only: `ENOENT`/`ECONNREFUSED` connecting to the socket |

`APP_NOT_READY` and `RATE_LIMITED` are the two codes worth an automatic retry
honoring `Retry-After`; the reference client does this by default (max 3
tries).

## SSE: `GET /api/local/events`

- `id:` is a cursor `<boot>:<seq>` (`boot` changes every app launch). Resume
  with the `Last-Event-ID` header or a `?cursor=` query param.
- Heartbeat: `: ping` comment line every 15s — not an event, ignore it.
- Event shape matches what plugins receive from `domainEvents`:
  `{type, workspaceId, taskId, task?, comment?, …, actor}`. `actor` is one of
  `persona:<uuid>`, `plugin:<id>`, `user` — use it to filter out an agent's own
  writes.
  - Persona event filtering follows `PLUGIN_EVENTS`: `comment.*` needs
    `comments:read`, `task.*` needs `tasks:read`, `task.relationChanged` needs
    `relations:read`. `timer.*` is never sent to a persona. Permissions are
    re-checked on every event.
  - Disabling or archiving the persona closes the stream with a `revoked`
    event.
- Buffer: a ring of 1000 events or 10 minutes per workspace, held in the main
  window. A cursor older than the buffer, or from a previous `boot`, gets a
  `reset` event carrying `{cursor}` — resync via
  `GET /api/tasks?updated_since=` and keep consuming from the new cursor.

## MCP setup

The app serves MCP as **streamable HTTP** on `POST /mcp` inside the same
socket (stateless, JSON responses, no SSE). A JSON-RPC message with no `id`
(a notification) gets a bare `202` with an empty body.

Codex (`~/.codex/config.toml`) and Claude Code only speak stdio or networked
HTTP/SSE for MCP — neither has a unix-socket transport — so agents connect
through a thin stdio↔socket bridge. Prefer the app's own `TMGR mcp`
subcommand (signed binary, no Node needed, Keychain reads without a dialog):

```toml
# ~/.codex/config.toml
[mcp_servers.tmgr-local]
command = "/Applications/TMGR.app/Contents/MacOS/TMGR"
args = ["mcp", "--token-id", "<token id>"]
```

```bash
claude mcp add tmgr-local -- /Applications/TMGR.app/Contents/MacOS/TMGR mcp --token-id <token id>
```

### Node fallback bridge

For development, or wherever the app subcommand isn't available, use
[`plugin-sdk/local-access/bin/tmgr-local-mcp.mjs`](../plugin-sdk/local-access/README.md#mcp-bridge)
instead of the app binary in the same configs:

```toml
[mcp_servers.tmgr-local]
command = "node"
args = ["/path/to/task-manager-ui/plugin-sdk/local-access/bin/tmgr-local-mcp.mjs", "--token-id", "<token id>"]
```

```bash
claude mcp add tmgr-local -- node /path/to/task-manager-ui/plugin-sdk/local-access/bin/tmgr-local-mcp.mjs --token-id <token id>
```

It reads newline-delimited JSON-RPC from stdin, forwards each message to
`POST /mcp` over the socket, and writes the JSON response as one line to
stdout (nothing for a `202`). A transport failure on a request with an `id`
becomes a JSON-RPC error response (`code: -32000`) instead of hanging the
client, and is logged to stderr. Token resolution: `--token-id <id>` (reads
Keychain on macOS) or `TMGR_LOCAL_TOKEN` env var.

## Plugin → companion forwarding: an example, not a platform API

The `tmgr.agent-board` plugin owns "my comments become a command to the
agent"; it does not go through the socket, so this is **the board plugin's
own contract with its companion process, not something this platform
exposes or guarantees**. It is documented here only as a worked example of
how a plugin can forward a hint to a local companion.

The board plugin calls `tmgr.net.fetch('http://127.0.0.1:<port>/v1/comment-created', { method: 'POST', headers: { Authorization: 'Bearer <token>' }, body })`.
Port and token are set by the owner in the board's setup wizard.
`net.fetch` only allows loopback HTTP.

- The body is a **hint, not data**: `{v: 1, workspaceCode, taskId, commentId, createdAt}` — no comment text.
- The companion must **not trust the hint**. It re-reads
  `GET /api/tasks/{taskId}/comments` over the socket and only accepts the
  comment if `author.kind === 'user'` and `id === commentId`. Anything on the
  local machine that also knows the token can hit this port, but it cannot
  forge the comment's author — that's asserted from data read back through
  the socket, not from the ping.
- Idempotency is by `commentId` (the companion's own cursor). The companion
  replies `202` or an error; on error the plugin retries via its alarm (up to
  10 attempts, starting at 1 minute) and queues in `tmgr.storage`.
- The plugin only runs in the **active** workspace, so forwarding pauses while
  the local workspace isn't active and resumes on return. The companion's
  fallback path is the same filter (`author.kind === 'user'`) applied to the
  `comment.created` event from `GET /api/local/events`.
