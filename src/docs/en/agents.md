---
title: AI agents
section: Guides
order: 10
description: Connect Claude Code, Codex, Cursor and other MCP agents to TMGR.
---

Connect Claude Code, Codex, Cursor and other MCP-capable agents to TMGR. Two MCP servers cover two jobs: working with your tasks, and reaching you when an agent needs attention.

<!-- component:AiAgentsOverview -->

Need an agent to act inside one workspace as a named persona? Create a persona token in Settings → Personas → Connect an agent.

## Claude Code plugin {#claude-code-plugin}

The quickest way to use TMGR from Claude Code is the official plugin ([tmgr-dev/agent-plugin](https://github.com/tmgr-dev/agent-plugin), MIT). It adds the cloud MCP server and skills that teach the agent to take tasks, report its work, create tasks, and use Decisions and pages.

```sh
/plugin marketplace add tmgr-dev/agent-plugin
/plugin install tmgr@tmgr
```

Claude Code asks for your smart device token ([Settings → Smart Devices](/settings?tab=device)) and keeps it in the OS keychain. Decisions tools need the local `tmgr mcp` from the [command line](/docs/cli/the-decisions-inbox). For Codex and Cursor, see `AGENTS.md` in the plugin repository. To connect by hand instead, follow the steps below.

## 1. Get a token {#get-a-token}

Open [Settings → Agent alerts](/settings/agent-notifications) and press Create token. The token starts with `tmgrn_` and is shown only once, so copy it right away.

## 2. Connect your agent {#connect-your-agent}

Pick your client below and replace the placeholder with your token. You need Node 22+, and the TMGR mobile app for pushes and alarms.

<!-- component:AgentConnectTabs -->

## Task MCP (tmgr) {#task-mcp}

A remote HTTP MCP server to work with tasks, statuses, comments, time tracking and routines. Generate a device token in [Settings → Smart Devices](/settings?tab=device) and send it in the `X-Smart-Device-Token` header.

### Claude Code {#task-mcp-claude-code}

```sh
claude mcp add --transport http tmgr {{mcpUrl}} \
  --header "X-Smart-Device-Token: <your device token>"
```

### mcp.json {#task-mcp-json}

```json
{
  "mcpServers": {
    "tmgr": {
      "type": "http",
      "url": "{{mcpUrl}}",
      "headers": {
        "X-Smart-Device-Token": "<your device token>"
      }
    }
  }
}
```

This is not an OAuth connector, so claude.ai custom connectors that require OAuth are not supported.

## Alarms {#alarms}

The `alarm` tool rings in the iOS app first, then calls your phone if it is not acknowledged (press 1 to acknowledge). To make it reliable:

- Set and verify an alarm phone in Settings → Agent alerts.
- Add the TMGR caller number to your contacts or Favorites.
- Enable Repeated Calls in iOS settings.
- Allow alarms for the TMGR app.

## Troubleshooting {#troubleshooting}

- No push arrives: check that `~/.config/tmgr-notify/env` exists and is readable. Hooks always exit 0 and print nothing.
- Run `claude mcp list` to check that the server is connected.
- MCP servers do not go into `settings.json`; use `claude mcp add`.
- Codex ignores `notify` in a project-local `.codex/config.toml`; put it in `~/.codex/config.toml`.
- Requests are limited to 20 per minute per token.

## Links {#links}

- [@tmgr/notify on npm](https://www.npmjs.com/package/@tmgr/notify)
- [tmgr-notify on GitHub](https://github.com/tmgr-dev/tmgr-notify)

## Build a plugin with your agent {#plugins}

Ask your agent to make a TMGR desktop plugin. Point it at [the plugin guide](/docs/plugins.md) or `/llms.txt`: it covers the manifest, permissions and the whole write, test and try loop.
