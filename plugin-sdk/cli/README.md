# tmgr-plugin

Command line for TMGR desktop plugins: lint, bundle, smoke-test in the sandbox, and install into the
desktop app while you edit. Node 22.12 or newer.

```bash
npm install --save-dev tmgr-plugin
npx tmgr-plugin validate [dir] [--json] [--strict]
npx tmgr-plugin build [dir]
npx tmgr-plugin check [dir]
npx tmgr-plugin dev [dir] [--once]
```

`dir` defaults to the current directory.

- `validate` checks `manifest.json`, the files it references and the permissions the code uses, with the
  same rules as the app. `--strict` fails on warnings, `--json` prints one machine-readable report.
- `build` writes `dist/tmgr-plugin.json`, the one file a GitHub release carries.
- `check` runs `validate`, loads `main.js` in the QuickJS test host (`tmgr-plugin-testing`), runs every
  command in `contributes.commands` with `null` arguments (task menu commands are clicked on a sample
  task), renders every view without a `ui` page, and prints thrown errors, `console` output and the
  permission groups. It exits 1 if anything threw.
- `dev` runs `validate` and `check`, installs the plugin into `~/.tmgr.dev/plugins/<manifest id>/`, then
  watches the project and repeats on every settled change. It also tails the app log and prints the
  lines that start a plugin message with `[plugin] <manifest id> `. `--once` stops after the first install.

## What `dev` needs from you

- A local workspace (folder plugins do not run in cloud workspaces).
- Developer mode, switched on by hand in Settings, Plugins.
- The plugin enabled once in Settings, Plugins.
- The app window visible: it notices file changes only then.

## What `dev` copies

Only `manifest.json`, the file named by `main` (installed as `main.js`) and `ui/*.html`: at most 10 pages,
each file at most 1 MB (it stops with an error instead of letting the app ignore them). Nothing else is
copied, so keys, `.env`, `.git` and `node_modules` stay in the project. The folder name is the manifest
id, never the project folder name. Symlinks are refused. A `.tmgr-dev` marker file holding the id is
written into the folder, and a folder without that marker, or with another id, is never overwritten.

## Environment

- `TMGR_PLUGINS_DIR`: plugins root instead of `~/.tmgr.dev/plugins`.
- `TMGR_LOG_FILE`: app log to tail. Defaults: macOS `~/Library/Logs/dev.tmgr.desktop/TMGR.log`, Linux
  `~/.local/share/dev.tmgr.desktop/logs/TMGR.log`, Windows `%APPDATA%\dev.tmgr.desktop\logs\TMGR.log`.

`lib/` is generated from `plugin-sdk/template` in the app repo (`node plugin-sdk/cli/sync.mjs`), so the
CLI never drifts from the template's `validate.mjs` and `build.mjs`.
