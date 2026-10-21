# TaskUsageAggregate (`task_usage.v1`)

The per-task summary that `tmgr usage task <REF> --json` prints under `aggregate`, and that the
opt-in upload sends to the server (`POST /api/tasks/{id}/agent-usage`). Built from local AI tool
logs by `tmgr_usage::task::TaskUsageAggregate`.

It carries only numbers, enums, timestamps and names. No prompts, replies, file contents, tool
arguments or results, hook commands or output, paths, usernames or URLs.

## Identity and idempotency

- One aggregate per `(task.workspace, task.ref, source_id)`. Sending again replaces the previous one.
- `source_id`: opaque random id of the install, `[A-Za-z0-9_-]{1,64}`, created once and kept in the
  local usage database. It is not derived from the machine, user or paths.
- `generated_at`: RFC 3339 UTC. The server may drop an upload older than the stored one.

## Names

The client checks every name before sending; the server rejects anything else. A generic name that does
not fit its pattern is sent as `other` rather than rewritten, so no path, email or free text passes in a
cleaned-up form.

| Field | Allowed |
|---|---|
| tool, skill, plugin, MCP server and tool, hook name and event, slash command, agent type, origin, permission mode, entrypoint | `[A-Za-z0-9._:+-]{1,128}`, not starting with `.` or `~`, no `..` |
| model | `[A-Za-z0-9._:+@-]{1,128}`; `/` becomes `:` |
| version | `[A-Za-z0-9._+-]{1,64}` |
| file extension | `[A-Za-z0-9]{1,16}`, otherwise `other` |
| `prs[].repo` | `owner/name`: `[A-Za-z0-9_.-]{1,64}/[A-Za-z0-9_.-]{1,100}`, no leading `.`; `number` 1..2^31 |
| node id | `[A-Za-z0-9_-]{1,32}`; a `parent_id` always names an earlier node; depth ≤ 32 |

Integers are JSON integers (0..2^53). `cost_usd` is a number ≥ 0 with at most 6 decimals.
`generated_at` must not be more than 5 minutes in the future.

## Caps

- `nodes` ≤ 200 (deepest and cheapest dropped first; `truncated.nodes = true`).
- Each name/count list ≤ 100 entries, all lists together ≤ 500 (`truncated.lists = true`).
- Whole body ≤ 256 KB.

## Shape

```json
{
  "schema": "task_usage.v1",
  "source_id": "k3J9x0Qe2bVt",
  "generated_at": "2026-10-16T12:00:00Z",
  "task": { "workspace": "tmgrdev", "ref": "TM-571", "id": 9975 },
  "period": { "from": "2026-10-16T08:00:00Z", "to": "2026-10-16T11:10:00Z" },
  "totals": {
    "cost_usd": 18.4,
    "events": 412,
    "unpriced_events": 0,
    "tokens": {
      "input": 1200000, "output": 210000, "cache_read": 18400000,
      "cache_write_5m": 600000, "cache_write_1h": 300000, "reasoning": 40000
    },
    "active_ms": 9300000,
    "waiting_ms": 2100000,
    "turns": 31,
    "turn_ms": 7800000
  },
  "limits": [
    { "tool": "claude", "window": "week", "used_pct": 6.0, "approximate": true }
  ],
  "nodes": [
    {
      "id": "n0", "parent_id": null, "depth": 0,
      "tool": "claude", "agent_type": "main", "model": "claude-opus-5-5",
      "origin": "cli", "link": null,
      "started_at": "2026-10-16T08:00:00Z", "ended_at": "2026-10-16T10:10:00Z", "duration_ms": 7800000,
      "cost_usd": 14.9, "events": 300,
      "tokens": { "input": 0, "output": 0, "cache_read": 0, "cache_write_5m": 0, "cache_write_1h": 0, "reasoning": 0 },
      "tools": [ { "name": "Bash", "count": 42, "errors": 1, "duration_ms": 61000 } ]
    },
    {
      "id": "n3", "parent_id": "n2", "depth": 2,
      "tool": "codex", "agent_type": "codex", "model": "gpt-6-astra",
      "origin": "exec", "link": "likely",
      "started_at": "...", "ended_at": "...", "duration_ms": 1020000,
      "cost_usd": 0.0, "events": 12, "tokens": { "...": 0 },
      "tools": [ { "name": "shell", "count": 14, "errors": 0, "duration_ms": 0 } ]
    }
  ],
  "context": {
    "tools": [ { "name": "Read", "count": 80, "errors": 0, "duration_ms": 9000 } ],
    "skills": [ { "name": "superpowers:brainstorming", "count": 1 } ],
    "skills_available": 214,
    "plugins": [ { "name": "superpowers", "count": 3 } ],
    "mcp": [ { "server": "tmgr", "tool": "update_task", "count": 5, "errors": 0 } ],
    "hooks": [ { "event": "PostToolUse", "name": "token-goat", "tool": "Read", "count": 80, "errors": 0, "duration_ms": 4000 } ],
    "slash_commands": [ { "name": "tm", "count": 1 } ],
    "permission_modes": [ { "mode": "bypassPermissions", "count": 1 } ],
    "compactions": 1,
    "away": 2,
    "file_edits": { "count": 17, "extensions": [ { "name": "rs", "count": 15 } ] },
    "prs": [ { "repo": "tmgr-dev/frontend", "number": 497 } ],
    "agents": [ { "tool": "claude", "version": "2.1.90", "entrypoint": "cli", "count": 4 } ]
  },
  "truncated": { "nodes": false, "lists": false }
}
```

## Fields

| Field | Type | Notes |
|---|---|---|
| `task.workspace` | string | workspace code |
| `task.ref` | string | `TM-571` style key, or `#<id>` for a task without a category key (then `task.id` is that id) |
| `task.id` | integer or null | server task id when known |
| `totals.cost_usd`, `nodes[].cost_usd` | number | API-equivalent USD, 6 decimals max |
| `tokens.*` | integer ≥ 0 | |
| `totals.active_ms` | integer | union of every node's activity intervals, so no node exceeds it |
| `nodes[].active_ms` | integer, optional | the node's activity: timestamps closer than 5 minutes join one interval (tool calls count from start to end), longer gaps are idle; `duration_ms` stays first-to-last wall clock |
| `nodes[].role` | name, optional | `orchestrator`, `lead` or `worker`: set when the node got events as an orchestrator or is bound to a run with a known role |
| `rates[]` | list, optional | USD per 1M tokens (`model`, `input`, `output`, `cache_read`, `cache_write_5m`, `cache_write_1h`, all numbers 0..100000, 6 decimals max) for each model in the task's events that has a full price; max 100, a model without one is left out |
| `waiting_ms` | integer, optional | time agents waited for a person; left out until stage 2 fills it |
| `limits[].tool` | enum | `claude`, `codex` |
| `limits[].window` | enum | `five_hour`, `week`, `week_opus`, `week_sonnet`, `other` |
| `limits[].used_pct` | number | share of the window the task used, percentage points |
| `nodes[].tool` | enum | `claude`, `codex` |
| `nodes[].agent_type` | name | `main` for the top session, else the subagent type (`Explore`, `codex:codex-rescue`), `codex` for Codex |
| `nodes[].origin` | name or null | Claude entrypoint (`cli`, `sdk-cli`, …) or Codex `source` (`exec`, `cli`, `vscode`, `subagent`) |
| `nodes[].link` | enum or null | how the node was tied to its parent or task: `null` (logged by the tool), `exact`, `likely` (also an orchestrator whose own events were attributed by the window of its descendants) |
| `nodes[].depth` | integer | 0 for roots |
| `context.hooks[].event` | name | `PreToolUse`, `PostToolUse`, `Stop`, `UserPromptSubmit`, `SessionStart`, … |
| `context.hooks[].name` | name | basename of the hook script, or `inline` |
| `context.hooks[].tool` | name or null | tool the hook ran for |
| `context.file_edits.extensions[].name` | name | file extension without the dot, `none` when missing |
