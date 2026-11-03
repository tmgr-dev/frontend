---
name: tmgr-plugin
description: Use when the user asks to make, change, test or debug a TMGR plugin, a folder with manifest.json, main.js and optional ui/*.html pages that runs in the TMGR desktop app. Covers scaffolding, the test, validate, check and dev loop, permissions and what a plugin may not do.
---

# TMGR plugin

A plugin is a folder: `manifest.json`, `main.js`, optional `ui/*.html` pages. It runs in the TMGR desktop app
sandbox (QuickJS) and talks to the app through the global `tmgr`. Target API 1.10: `engines.tmgr` `^1.10`.

References:
- Guide: https://tmgr.dev/docs/plugins.md
- Types for `main.js`: https://tmgr.dev/docs/plugin-sdk/tmgr.d.ts; for pages: https://tmgr.dev/docs/plugin-sdk/window.d.ts
- Index for LLMs: https://tmgr.dev/llms.txt

## Preconditions (the user does these, not you)

- TMGR desktop app. Folder plugins run only in a local workspace, not in a cloud one.
- Developer mode: the user switches it on in Settings, Plugins.
- The user switches the plugin on once in Settings, Plugins. The app reloads changed files only while its window is visible.

## The loop

1. Scaffold: `npm create tmgr-plugin@latest publisher.name [dir]`. Until that package is published, copy
   `plugin-sdk/template/` from https://github.com/tmgr-dev/frontend next to `plugin-sdk/testing`.
2. Edit `manifest.json`: `id` is `publisher.name`, command ids start with the id, `permissions` lists what you call.
   Then edit `main.js`, and `ui/page.html` if you add a page.
3. `npm test`: runs `main.test.mjs` against `createTestHost` from `tmgr-plugin-testing`. It uses the app's own broker,
   manifest rules and UI sanitizer over an in-memory mock of TMGR data. Write or update a test for each behaviour you change.
4. `npm run validate`: checks the manifest and files. It reports permissions used but not declared, and declared but
   never used. `--strict` turns warnings into a failure.
5. `npm run check`: runs every command in the test host and reports the errors.
6. `npm run dev`: validates, checks, copies the plugin into `~/.tmgr.dev/plugins/<folder>/` and watches it.
   If the plugin is not switched on yet, tell the user to switch it on.

## Permissions

- Ask for the smallest set. Declare only what `main.js` and the pages call.
- Name them in the groups the app shows when someone installs a plugin:
  - Reads your data: `tasks:read`, `statuses:read`, `categories:read`, `time:read`, `comments:read`, `relations:read`, `agent_work:read`, `routines:read`, `pages:read`, `files:attachments`
  - Changes your data: `tasks:write`, `statuses:write`, `categories:write`, `time:write`, `comments:write`, `relations:write`, `agent_work:write`, `routines:write`, `pages:write`, `pages:sections`
  - Changes the interface: `tray`, `views:badge`, `menus:task`
  - Reaches outside the app: `notifications`, `alarms`, `deeplinks`, `links:open`, `files:export`, `files:pick`
- `pages:write` and `pages:sections` need `pages:read`. Storage, settings, `commands.register` and the status bar need no permission.
- Explain each permission and let the user approve the set. An update that adds a permission asks the user again.
- The app rejects unknown permission names.

## Rules

- UI is data: `tmgr.ui.providePage(viewId, async () => tree)` returns nodes such as `stack`, `heading`, `list`, `button`, `card`, `markdown`. Pages in `ui/` use `window.tmgr`.
- Register every command from `contributes.commands` with `tmgr.commands.register(id, handler)`.
- Keep state in `tmgr.storage`, `tmgr.taskData` or `tmgr.pageData`.
- Network: only `http://localhost` origins listed in `network.allowedOrigins`.
- Limits: 1 MB per file, 10 pages, 3 task menu items, 5 tray items.

## Do not

- Use the DOM, `fetch`, timers or any browser or Node global in `main.js`. The sandbox has `tmgr` and `console` only.
- Switch developer mode on, or switch a plugin on, for the user.
- Commit signing keys (`*.key`, `*.pem`) or `dist/`.
- Edit generated files (`manifest.generated.js`, `dist/`). Change the source and rebuild.
- Widen permissions quietly, or skip `npm test` to reach `npm run dev` sooner.
- Publish on the user's behalf. Releases are signed tags; the template README describes them.
