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

## Task menu items (API 1.6)

A plugin can add up to 3 items to the task "…" menu (board card, task list, task page). It needs
`"engines": { "tmgr": "^1.6" }` and the `menus:task` permission. Each item names a command declared in
`contributes.commands`; the title is at most 40 characters:

```json
{
	"permissions": ["menus:task", "comments:write"],
	"contributes": {
		"commands": [{ "id": "yourname.hello.comment", "title": "Add a comment" }],
		"menus": { "task/card": [{ "command": "yourname.hello.comment", "title": "Add a comment" }] }
	}
}
```

The command's handler receives `{ taskId, workspaceId }` (`TaskMenuCommandArgs` in `tmgr.d.ts`):

```js
tmgr.commands.register('yourname.hello.comment', async (args) => {
	await tmgr.comments.add(args.taskId, 'Hello from the task menu');
});
```

The item is shown only while the plugin is running. Test it with `taskMenuItems()` and
`clickTaskMenu(command, taskId)` from `plugin-sdk/testing`.
