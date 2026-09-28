# @tmgr/local-access

Reference Node client for the TMGR desktop **local access socket** — a unix
socket the desktop app opens so an agent (Claude Code, Codex, a custom
companion) can act as a connected persona, without any network access.

Zero dependencies, Node ≥ 20, ESM only. See
[`../../docs/local-access-api.md`](../../docs/local-access-api.md) for the
full protocol (routes, error codes, SSE format, security boundary).

## Install-free usage

This package is not published; use it straight from the repo:

```js
import { createLocalAccessClient } from './plugin-sdk/local-access/index.mjs';

const client = createLocalAccessClient({ tokenId: 'lt_xxxxxxxxxxxxxxxx' });

const { data: tasks } = await client.get('/api/tasks?per_page=1');
```

`createLocalAccessClient(options?)`:

| Option | Default | Notes |
|---|---|---|
| `socketPath` | `resolveSocketPath()` | see below |
| `token` | — | explicit token, takes priority over `tokenId` |
| `tokenId` | — | resolved via `resolveToken()` on each first use |
| `timeoutMs` | `20000` | per-request timeout |
| `retry` | `true` | auto-retry on `503 APP_NOT_READY` / `429 RATE_LIMITED`, honoring `Retry-After`, up to 3 tries |

The client exposes `request(method, path, body?, opts?)`, `get/post/patch/put/delete`,
`health()`, `whoami()`, `mcpCall(message)`, and `events(opts)`. `request()` returns
the parsed JSON body as-is (the `{data}` envelope from the Java-shaped REST is not
unwrapped).

`resolveSocketPath()` checks, in order: `TMGR_LOCAL_SOCKET` env var → the
`<app data dir>/local-access/socket-path` override file → the default
`<app data dir>/local-access/local-access.sock`.

## Token setup

A local persona token (`tmgrl_…`) is issued from **Settings → Personas → On this
device → Connect an agent**. The secret never touches disk or logs outside the
OS Keychain. Three ways to hand it to this client:

1. `TMGR_LOCAL_TOKEN=tmgrl_…` env var — simplest for scripts.
2. `token: 'tmgrl_…'` passed directly to `createLocalAccessClient`.
3. `tokenId: 'lt_…'` — on macOS the client shell out to
   `security find-generic-password -s dev.tmgr.local-access -a <tokenId> -w`
   to read the secret from Keychain (macOS will prompt once, the app that
   wrote the entry is shown). The secret is never printed or logged.

`resolveToken({token, tokenId})` implements this precedence and can be called
directly if you need the raw token string.

## Error codes

Every non-2xx response throws `LocalAccessError` with `{status, code, message,
retryAfter}`. Transport failures are mapped too: `ENOENT`/`ECONNREFUSED` on the
socket → `{status: 0, code: 'APP_NOT_RUNNING'}`, a client-side timeout →
`{code: 'TIMEOUT'}`. Server-side codes (`TOKEN_INVALID`, `TOKEN_EXPIRED`,
`PERSONA_ARCHIVED`, `ROUTE_NOT_ALLOWED`, `RATE_LIMITED`, `APP_NOT_READY`, …) are
passed through unchanged — see the full table in `docs/local-access-api.md`.

```js
try {
  await client.get('/api/tasks/1');
} catch (err) {
  if (err.code === 'APP_NOT_RUNNING') { /* TMGR is not running */ }
  else if (err.code === 'TOKEN_EXPIRED') { /* re-issue the token */ }
}
```

## SSE (`events()`)

```js
const controller = new AbortController();
await client.events({
  cursor: lastKnownCursor, // "<boot>:<seq>", optional
  signal: controller.signal,
  onEvent: (event) => console.log(event.event, event.data),
  onReset: (cursor) => resync(cursor), // buffer overrun — refetch via REST
  onRevoked: () => console.log('token revoked, stopping'),
});
```

`events()` resolves once the stream stops: on `revoked`, or when `signal` is
aborted. On any other drop it reconnects with `Last-Event-ID` and an
exponential backoff from 1s up to 30s. `: ping` comment lines (sent every 15s)
are ignored.

## MCP bridge

`bin/tmgr-local-mcp.mjs` is a stdio↔socket bridge for MCP clients that only
speak stdio (Codex, Claude Code) — a fallback for the app's own `TMGR mcp`
subcommand. It reads newline-delimited JSON-RPC from stdin, `POST`s each
message to `/mcp` over the socket, and writes the JSON response as one line to
stdout. Notifications (202 responses) produce no output. A transport failure
on a request that has an `id` is turned into a JSON-RPC error response
(`code: -32000`); it is also logged to stderr.

```bash
node plugin-sdk/local-access/bin/tmgr-local-mcp.mjs --token-id lt_xxxxxxxxxxxxxxxx
# or: TMGR_LOCAL_TOKEN=tmgrl_... node plugin-sdk/local-access/bin/tmgr-local-mcp.mjs
```

Options: `--token-id <id>`, `--socket <path>` (defaults to `resolveSocketPath()`).

## Measurement script

`bin/tmgr-local-measure.mjs` is a manual tool for the app owner to characterize
the socket, not something shipped to end users.

```bash
node plugin-sdk/local-access/bin/tmgr-local-measure.mjs latency --token-id lt_...
node plugin-sdk/local-access/bin/tmgr-local-measure.mjs watch --token-id lt_... --interval 10
node plugin-sdk/local-access/bin/tmgr-local-measure.mjs perm
```

- `latency` — 50 sequential + 10 parallel `GET /api/tasks?per_page=1`, prints
  p50/p95/max and error counts by code.
- `watch` — polls `health()` + `whoami()` every `--interval` seconds (default
  10), prints timestamp/latency/status, and flags a wall-clock gap over
  2× the interval as `sleep/wake detected`. Meant to be run across hiding the
  window, sleeping the machine 5+ minutes, restarting, and quitting the app.
- `perm` — prints the socket file mode (expect `0600`) and its directory mode
  (expect `0700`).

## Tests

```bash
cd plugin-sdk/local-access
node --test "test/**/*.test.mjs"
```

Each test spins up a plain `node:http` server on a temporary unix socket to
stand in for the app. Note: on this Node build, `node --test test/` (a bare
directory path) fails with a spurious `MODULE_NOT_FOUND`; the glob form above
is the reliable invocation. `npm test` runs this same command.
