# TMGR plugin template

1. Rename the plugin in `manifest.json`: `id` is `publisher.name`, and command ids start with it.
2. Write the logic in `main.js` against `tmgr` (types: `../tmgr.d.ts`). Pages in `ui/` open in their own
   window and talk to the app through `window.tmgr` (types: `../window.d.ts`).
3. Try it: copy the folder to `~/.tmgr.dev/plugins/<name>/`, turn on developer mode in TMGR
   (Settings → Plugins), open a local workspace and switch the plugin on.
4. Publish: push a tag like `v1.0.0`. The workflow builds `dist/tmgr-plugin.json` and attaches it to the
   release. People install it by pasting the repository link in Settings → Plugins.

Plugins run only in local workspaces, in a sandbox: no DOM, no network except the `http://localhost`
origins you declare, and only the permissions listed in the manifest.
