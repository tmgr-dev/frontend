# TMGR plugin: agent guide

A plugin is a folder: `manifest.json`, `main.js`, optional `ui/*.html` pages. It runs in the TMGR desktop app
sandbox (QuickJS) through the global `tmgr`. Target API 1.10: `engines.tmgr` `^1.10`.

Read first: https://tmgr.dev/docs/plugins.md. Types: https://tmgr.dev/docs/plugin-sdk/tmgr.d.ts (`main.js`) and
https://tmgr.dev/docs/plugin-sdk/window.d.ts (pages). Index: https://tmgr.dev/llms.txt.

## Preconditions (the user does these)

- TMGR desktop app; folder plugins run only in a local workspace.
- The user turns on developer mode (Settings, Plugins) and switches the plugin on once.

## Loop

1. Scaffold: `npm create tmgr-plugin@latest publisher.name [dir]`.
2. Edit `manifest.json` (`id` = `publisher.name`, command ids start with the id, `permissions` = what you call),
   then `main.js` and any `ui/*.html`.
3. `npm test`: `main.test.mjs` runs the plugin in `createTestHost` (`tmgr-plugin-testing`) with the app's own
   broker, manifest rules and UI sanitizer over in-memory data.
4. `npm run validate`: manifest and files; warns about permissions used but not declared, or declared but unused.
5. `npm run check`: runs every command in the test host and reports errors.
6. `npm run dev`: validate, check, copy to `~/.tmgr.dev/plugins/<folder>/`, watch.

## Permissions

- Ask for the smallest set and explain each one. Groups:
  - Reads: `tasks:read`, `statuses:read`, `categories:read`, `time:read`, `comments:read`, `relations:read`,
    `agent_work:read`, `routines:read`, `pages:read`, `files:attachments`
  - Writes: `tasks:write`, `statuses:write`, `categories:write`, `time:write`, `comments:write`,
    `relations:write`, `agent_work:write`, `routines:write`, `pages:write`, `pages:sections`
  - Interface: `tray`, `views:badge`, `menus:task`
  - Outside the app: `notifications`, `alarms`, `deeplinks`, `links:open`, `files:export`, `files:pick`
- `pages:write` and `pages:sections` need `pages:read`.
- Storage, settings, `commands.register` and the status bar need no permission.

## Rules

- UI is data: `tmgr.ui.providePage(viewId, async () => tree)`. Pages in `ui/` use `window.tmgr`.
- Register every command: `tmgr.commands.register(id, handler)`.
- State goes in `tmgr.storage`, `tmgr.taskData` or `tmgr.pageData`.
- Network: only the `http://localhost` origins in `network.allowedOrigins`.

## Do not

- Use DOM, `fetch`, timers or other browser or Node globals in `main.js`.
- Turn on developer mode or switch a plugin on for the user.
- Commit signing keys or `dist/`.
- Edit generated files such as `manifest.generated.js`; change the source and rebuild.
- Widen permissions quietly, or skip `npm test`.
