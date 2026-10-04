# TMGR plugin system

Notes for plugin authors on the UI calls that reach the user outside a plugin's own page. Types live in
`plugin-sdk/template/tmgr.d.ts`; the test harness is `plugin-sdk/testing`.

## Pages (API 1.5)

Plugins read and write the workspace's pages. Types are in `plugin-sdk/template/tmgr.d.ts` (`TmgrPage`,
`TmgrPageSection`, `TmgrPageConflictError`).

| Call | What it does | Permission | Limits |
| --- | --- | --- | --- |
| `tmgr.pages.search(q, { type?, limit? })` | Full-text search over titles and bodies. | `pages:read` (API 1.5) | `q` 1 to 200 characters, `limit` at most 50. Reads are rate limited like `tmgr.tasks.*`. |
| `tmgr.pages.tree()` | Every live page without bodies. | `pages:read` | |
| `tmgr.pages.get(idOrSlug)` | One page: body, properties, `version`, author and its `sections` (id, owner, heading). | `pages:read` | |
| `tmgr.pages.create({ title, type?, parentId?, body?, properties? })` | Creates a page authored by the plugin. | `pages:write` | Body at most 1 MB. Writes are rate limited like `tmgr.tasks.*`. |
| `tmgr.pages.update(id, { version, title?, body?, properties?, summary? })` | Changes a page. `version` is required. | `pages:write` | A stale `version` rejects with `page_conflict`, see below. |
| `tmgr.pages.append(id, { markdown, heading?, createHeading?, summary? })` | Adds markdown at the end of the page, or of the `##` section named `heading` (created when `createHeading` is set). No `version` needed. | `pages:write` | |
| `tmgr.pages.setSection(id, sectionId, markdown, { summary?, heading? })` | Replaces the text of a managed section the plugin owns, or creates it at the end of the page when it does not exist. | `pages:sections` | Only sections whose owner is `plugin:<this plugin's id>`. |
| `tmgr.pageData.get/set/delete/getMany` | Per-page JSON storage of the plugin, like `taskData`. | `pages:read` | Value at most 64 KB, key at most 200 characters, `getMany` up to 500 page ids. Own quota of 5 MB / 1000 keys. Local workspaces only. |

Rules:

- `engines.tmgr` must be `^1.5` for any `pages:*` permission; an older `engines.tmgr` is refused at load time.
- Writes are made as the plugin: the author's kind is `plugin`, and a managed section it owns carries the
  owner `plugin:<plugin id>`. A plugin cannot name another author.
- `pages:write` and `pages:sections` also need `pages:read` in the manifest; a manifest without it is refused.
- Section ownership is keyed by the manifest `id`: a section is the plugin's when its owner is
  `plugin:<manifest id>`. A plugin shipped under another id (or a fork that changes the id) does not own the
  sections of the original.
- `setSection` checks the page's `sections` first and refuses (`PERMISSION_DENIED`) an existing section not
  owned by this plugin; the app's page code enforces the same rule on its side. A section owned by `agents`
  belongs to personas only (a plugin cannot write it, and cannot append heading-less to a context page), and a
  section owned by `system` or by someone else is never writable by a plugin. Other sections of the page stay
  byte-identical when a plugin edits the body.
- When the section does not exist, `setSection` creates it at the end of the page, owned by the plugin and
  titled `heading` (default: the section id). The id must match `[a-z0-9][a-z0-9-]{0,63}` (`invalid_section_id`),
  the heading is one line of at most 200 characters (`invalid_heading`), and a context page refuses new
  sections.
- Plugins cannot declare page types. Use the core types (`plain`, `context`, `person`, `meeting`) and your own
  managed sections.
- A `409` on `update` rejects with an error whose `name` is `page_conflict` and whose `current` is the page as
  it is now. Re-read the data you need from `error.current`, merge, and call `update` again with
  `error.current.version`.
- In shared (cloud) workspaces the `pages:*` permissions are sent to the server when a workspace pins the
  plugin, and the server applies the same rules (backend release prod-java-0.0.128 or later). `pageData` is
  local-only, like `taskData`.
- `pageData` of a page is removed when the page is permanently deleted, not when it goes to the trash.

### Page events

`page.created`, `page.updated`, `page.deleted`, `page.restored` and `page.moved` need `pages:read`. The
payload is `{ type, workspaceId, pageId, slug, title, parentId, version, author: { kind, id }, changedSections }`;
`page.deleted` carries only `{ type, workspaceId, pageId }`.

- `changedSections` lists the managed sections whose text changed in this write. In local workspaces the app
  reports the sections whose text differs between the stored version and the new one. Events from shared workspaces arrive over realtime without a body and always
  report `[]`.
- A plugin does not receive the events of its own writes; other plugins and the user's writes are delivered.

## Task menu items (API 1.6)

A plugin can add items to the task "…" menu. Types: `TaskMenuCommandArgs` in `plugin-sdk/template/tmgr.d.ts`.

```json
{
	"engines": { "tmgr": "^1.6" },
	"permissions": ["menus:task", "comments:write"],
	"contributes": {
		"commands": [{ "id": "acme.notes.comment", "title": "Add a comment" }],
		"menus": { "task/card": [{ "command": "acme.notes.comment", "title": "Add a comment" }] }
	}
}
```

```js
tmgr.commands.register('acme.notes.comment', async (args) => {
	await tmgr.comments.add(args.taskId, 'Added from the task menu');
});
```

Rules:

- Needs the `menus:task` permission and `engines.tmgr` `^1.6`; an older `engines.tmgr` with `menus:task` is
  refused at load time. The permission is shown to the user when the plugin is installed or updated.
- `"task/card"` is the only location; any other key under `contributes.menus` is refused.
- At most 3 items per plugin. `title` is at most 40 characters. `command` must be a command declared in
  `contributes.commands`. Any item without `menus:task` in the manifest is refused.
- For a plugin with `engines.tmgr` older than `^1.6` the `menus` key is ignored, so existing plugins load as
  before.
- The command runs with `{ taskId, workspaceId }` (`workspaceId` is `null` when unknown), through the same
  path as any other command. The app refuses a command that is not an item of the plugin and a `taskId` that
  is not a positive integer.
- The items show on board cards, in the task list and on the task page and window, as a separate group of
  the "…" menu. With more than 4 plugin items in total the group becomes one "Plugins" submenu.
- Items are hidden while the plugin is not running (stopped, crashed, disabled) and until it has registered
  the command, so register it at start.
- An error thrown by the command shows the usual plugin error toast.
- In the test host, `taskMenuItems()` lists the items the app would show and `clickTaskMenu(command, taskId)`
  clicks one, refusing what the app refuses.

## Background and attention

| Call | What it does | Permission | Limits |
| --- | --- | --- | --- |
| `tmgr.ui.notify(message, options?)` | Shows a notification, attributed to the plugin. Suppressed while Do Not Disturb is on. | `notifications` | Rate limited per plugin. |
| `tmgr.ui.refresh(kind, id)` | Asks the app to draw the plugin's badges, page or section again. | none | Throttled per plugin. |
| `tmgr.ui.setStatusBarItem(id, item)` | Text (and optional command) in the status bar. `null` removes it. | none | `id` must be in `contributes.statusBarItems`; `text` at most 60 characters. |
| `tmgr.ui.setTrayItem(id, item)` | A submenu in the menu bar icon. `null` removes it. | `tray` (needs machine consent in shared workspaces) | `id` in `contributes.trayItems`, at most 5; at most 10 entries; titles are shortened, see below. |
| `tmgr.ui.setTrayTitle(text)` | Text next to the menu bar icon. | `tray` | Only the plugin chosen in Settings; at most 12 characters, one line. |
| `tmgr.ui.dnd()` | Reads whether Do Not Disturb is active. | none | Read-only. |
| `tmgr.ui.setViewBadge(viewId, badge)` | A counter on the plugin's own sidebar item. `null` clears it. | `views:badge` (API 1.4, no consent) | One applied change per view per second, see below. |

### `setTrayItem`: long titles

`title` and each entry's `title` are counted in graphemes, so an emoji is one character. A longer text is
shortened to 59 graphemes plus a trailing "…" (60 in total) instead of being refused. Only input over 1000
UTF-16 units, an empty title or a non-string is still `INVALID_PARAMS`.

A refused `setTrayItem` (`INVALID_PARAMS`, or `PERMISSION_DENIED` when the tray is not allowed on this
computer) no longer rejects the plugin's handler in the app: the call resolves and the reason goes to the
plugin log. A tray problem therefore cannot break the rendering of a page or a section. An undeclared `id`
(`NOT_DECLARED`) still rejects, since it is a bug in the plugin rather than a property of the environment.
The test host in `plugin-sdk/testing` rejects in all these cases so that the mistake is visible in tests.

## `tmgr.ui.setViewBadge`

```ts
setViewBadge(
	viewId: string,
	badge: { count?: number; text?: string; tone?: 'default' | 'info' | 'warning' | 'danger' } | null,
): Promise<void>;
```

Shows a counter on the plugin's item in the sidebar's Plugins section, for example "Telegram 12".

Rules:

- `engines.tmgr` must be `^1.4` and the manifest must declare `views:badge`. A manifest that declares
  `views:badge` with an older `engines.tmgr` is refused at load time.
- `viewId` is an id from this plugin's own `contributes.views`.
- Give exactly one of `count` and `text`.
  - `count` is a safe integer of 0 or more. `0` clears the badge. Above 99 it is drawn as "99+".
  - `text` is 1 to 4 UTF-16 units, for example "!" or "new".
- `tone` is optional and defaults to `default`. `danger` is red, `warning` yellow, `info` blue.
- `null` clears the badge.

Errors:

| Code | When |
| --- | --- |
| `PERMISSION_DENIED` | The manifest does not declare `views:badge`. |
| `INVALID_PARAMS` | `viewId` is not one of this plugin's declared views (an undeclared or foreign id is `INVALID_PARAMS`, not `NOT_DECLARED`); both or neither of `count` and `text`; a bad `count`, `text` or `tone`; a `badge` that is not an object or `null`. |

Rate limit: the first call for a view is applied at once. Further calls within one second of the last
applied change are held, the last one wins, and it is applied when the second ends. This is not an error and
the promise resolves as usual. A held call never writes after the plugin has stopped.

Lifecycle:

- The badge belongs to the plugin and the workspace. It is cleared when the plugin stops: disabled, removed,
  crashed, or the workspace switches.
- Badges live in memory only. After an app restart nothing is restored: set the badge again on the
  `app.started` event.
- It works the same in shared (cloud) workspaces. It is pure UI, needs no machine consent and has no server
  side. The permission is not sent to the server when a workspace pins the plugin.
- In the test host, `tmgr.viewBadges[viewId]` holds `{ count, text, tone }` (one of `count`/`text` is `null`).
  It is set immediately, without the one-second coalescing; `null` or a `0` count deletes the key.

## Migrating from 1.3 to 1.4

Nothing changes for existing plugins: `^1.0` to `^1.3` keep working.

To use badges:

1. Set `"engines": { "tmgr": "^1.4" }` and add `"views:badge"` to `permissions`.
2. Call `tmgr.ui.setViewBadge('<view id>', { count, tone })` when your data changes, and again on
   `app.started`.
3. Update `tmgr.d.ts` from the SDK template. `validate.mjs` now maps `tmgr.ui.setViewBadge` to `views:badge`.
4. Copy `plugin-sdk/testing` again to get `tmgr.viewBadges` and the new validation.

Also in 1.4: `setTrayItem` shortens long titles instead of refusing them (see above).

## Migrating from 1.4 to 1.5

Nothing changes for existing plugins: `^1.0` to `^1.4` keep working.

To use pages:

1. Set `"engines": { "tmgr": "^1.5" }` and add the permissions you need to `permissions`: `pages:read`,
   `pages:write`, `pages:sections`. They are shown to the user when the plugin is installed or updated.
2. Call `tmgr.pages.*` and `tmgr.pageData.*`; subscribe to `page.*` events with `tmgr.events.on`.
3. To keep your own text on a page, put it in a managed section owned by the plugin:
   `<!-- tmgr:section id="<id>" owner="plugin:<your plugin id>" -->` ... `<!-- /tmgr:section -->`, and
   rewrite it with `tmgr.pages.setSection`. Users can read it but not edit it by hand in the page.
4. Update `tmgr.d.ts` from the SDK template. `validate.mjs` now maps `tmgr.pages.*` to the `pages:*`
   permissions.
5. Copy `plugin-sdk/testing` again: `createTestHost` takes a `pages` option, an in-memory page store, and
   exposes `tmgr.pages` and `tmgr.pageData`.

Plugins cannot declare their own page types in 1.5: pages render on the web and on mobile too, where a plugin
does not run.

## Migrating from 1.5 to 1.6

Nothing changes for existing plugins: `^1.0` to `^1.5` keep working.

To add task menu items:

1. Set `"engines": { "tmgr": "^1.6" }` and add `"menus:task"` to `permissions`.
2. Declare the command in `contributes.commands` and list it under `contributes.menus["task/card"]` with a
   `title`.
3. Register the command with `tmgr.commands.register` and read `args.taskId` (and `args.workspaceId`).
4. Update `tmgr.d.ts` from the SDK template for `TaskMenuCommandArgs`.
5. Copy `plugin-sdk/testing` again to get `taskMenuItems()` and `clickTaskMenu()` and the new manifest
   validation.
