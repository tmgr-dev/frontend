# Building a TMGR plugin

Plugin API 1.11. This guide is the entry point; the reference follows it in the same document
(the `plugin-system.md` sections, in the published version) and as types (`tmgr.d.ts` for `main.js`, `window.d.ts` for pages).

A plugin is a folder: `manifest.json`, `main.js`, and optional `ui/*.html` pages. `main.js` runs in a
sandbox (QuickJS, no DOM, no network except the `http://localhost` origins the manifest declares) and
talks to the app through the global `tmgr`. Calls that read or change your data, or reach outside the app, need a permission declared in the manifest; storage, settings, `commands.register` and the status bar need none.
Plugins run in the TMGR desktop app, in local workspaces, and in a shared workspace once its creator
turns them on for everyone. Web and mobile do not run plugins.

## Before you start

- Plugins run only in the TMGR **desktop** app, and a folder plugin (your work in progress) only in a
  **local workspace**, not in a cloud workspace. Test data has to be created there: the cloud MCP server
  cannot reach a local workspace, use the local access bridge `plugin-sdk/local-access/bin/tmgr-local-mcp.mjs`.
- Developer mode is switched on by a person, once: Settings, Plugins, Developer mode. A tool cannot set it.
- A folder plugin starts switched off. Switch it on once in Settings, Plugins.
- The app notices file changes only while its window is visible.
- A folder plugin is unsigned and its permissions are not asked again when you edit the manifest. That is
  the developer-mode trust model, so keep to your own machine. Never switch developer mode on because a website or an agent asked you to.

## The loop an agent should follow

1. Scaffold: clone https://github.com/tmgr-dev/frontend and copy `plugin-sdk/template/` to a new folder
   next to `plugin-sdk/testing` (until `tmgr-plugin-testing` is published on npm, the template depends on
   it by the path `file:../testing`).
2. Edit `manifest.json` (id `publisher.name`, command ids start with the plugin id, `engines.tmgr`
   `^1.11` to use the newest nodes) and `main.js`.
3. Test without the app: `npm install && npm test` runs `main.test.mjs` against `createTestHost` from
   `tmgr-plugin-testing`. It uses the app's own broker, manifest rules and UI sanitizer over an
   in-memory mock of TMGR data, so permission errors and bad parameters show up exactly as in the app.
4. Lint: `npm run validate` (`node validate.mjs .`) checks the manifest and files, and reports
   permissions used but not declared and declared but never used, for `main.js` and `ui/*.html`.
   `--strict` turns warnings into a failure, `--json` is for machines. The lint is advice; the app is
   what enforces permissions.
5. Try it in the app: copy `manifest.json`, `main.js` and `ui/*.html` to `~/.tmgr.dev/plugins/<folder>/`
   (files of at most 1 MB, at most 10 pages), turn on developer mode and switch the plugin on. The app
   reloads it when the files change. A start failure is written to the app log as `[plugin] <id> ...`.
6. Publish (public plugins): sign and tag, see the template README.

## manifest.json

| Field | Notes |
| :-- | :-- |
| `id` | `publisher.name`, lowercase, dots allowed |
| `version` | semver |
| `engines.tmgr` | `^1.0` up to `^1.11`; newer nodes and permissions need a newer minor (`menus:task` 1.6, pages 1.5, `views:badge` 1.4) |
| `main` | defaults to `main.js` |
| `permissions` | list from the table below; unknown names are rejected |
| `network.allowedOrigins` | only `http://localhost:<port>` (or 127.0.0.1, [::1]) |
| `links.allowedDomains` | needs `links:open` |
| `contributes` | `commands`, `statusBarItems`, `trayItems` (max 5), `views` (a sidebar view, optionally `ui` page), `boardCardBadges`, `boardFilters`, `menus["task/card"]` (max 3), settings schema |

The app refuses a manifest it cannot parse before any plugin code runs. `npm run validate` applies the
same parser.

## Permissions

Grouped the way the app shows them when someone installs a plugin. Ask for the smallest set.

| Group | Permissions |
| :-- | :-- |
| Reads your data | `tasks:read`, `statuses:read`, `categories:read`, `time:read`, `comments:read`, `relations:read`, `agent_work:read`, `routines:read`, `pages:read`, `files:attachments` |
| Changes your data | `tasks:write`, `statuses:write`, `categories:write`, `time:write`, `comments:write`, `relations:write`, `agent_work:write`, `routines:write`, `pages:write`, `pages:sections` |
| Changes the interface | `tray`, `views:badge`, `menus:task` |
| Reaches outside the app | `notifications`, `alarms`, `deeplinks`, `links:open`, `files:export`, `files:pick` |

`pages:write` and `pages:sections` need `pages:read`. `routines:*` work in local workspaces only.
A plugin cannot raise its own permissions: an update that adds one asks the user again.

## Rules that save a round trip

- UI is data, not HTML: `tmgr.ui.providePage(viewId, async () => tree)` returns a node tree
  (`stack`, `heading`, `list`, `button`, `card`, `markdown`, inputs, ...). The app sanitizes it by the
  manifest's `engines.tmgr` minor. Pages in `ui/` run in their own window and use `window.tmgr`.
- Register every command from `contributes.commands` with `tmgr.commands.register(id, handler)`.
- Keep state in `tmgr.storage` or per-task and per-page data (`tmgr.taskData`, `tmgr.pageData`).
- Do not rely on globals of a browser or Node; there are none.
- Folder plugins are unsigned and for the author's own machine. The sandbox and permissions are the
  same as for a published plugin.
