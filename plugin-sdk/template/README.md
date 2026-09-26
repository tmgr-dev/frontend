# TMGR plugin template

1. Rename the plugin in `manifest.json`: `id` is `publisher.name`, and command ids start with it.
2. Write the logic in `main.js` against `tmgr` (types: `tmgr.d.ts`). Pages in `ui/` open in their own
   window and talk to the app through `window.tmgr` (types: `window.d.ts`).
3. Try it: copy the folder to `~/.tmgr.dev/plugins/<name>/`, turn on developer mode in TMGR
   (Settings → Plugins), open a local workspace and switch the plugin on.
4. Create your signing key once, outside the repository, and give it to GitHub as a secret:

   ```bash
   node sign.mjs keygen ~/.tmgr-plugin-signing.key      # prints your public key
   gh secret set TMGR_PLUGIN_SIGNING_KEY < ~/.tmgr-plugin-signing.key
   ```

   Keep a backup of the key. Every release must be signed with it: TMGR remembers the key on the first
   install and refuses updates signed with another one. Never commit it (`*.key` is ignored).
5. Publish: push a tag like `v1.0.0`. The workflow builds `dist/tmgr-plugin.json`, signs it and attaches
   the bundle, its `.minisig` signature and your public key to the release. People install it by pasting
   the repository link in Settings → Plugins.
6. To be shown as a verified publisher, open a pull request to
   [tmgr-dev/tmgr-plugins](https://github.com/tmgr-dev/tmgr-plugins) adding your plugin id, repository and
   public key to `catalog.json`.

Plugins run in the TMGR desktop app, in a sandbox: no DOM, no network except the `http://localhost`
origins you declare, and only the permissions listed in the manifest. They run in local workspaces, and in
a shared workspace once its creator turns them on for everyone.
