# Building a TMGR plugin

Plugin API 1.10. This guide is the entry point; the reference follows it in the same document
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
   `^1.10` to use the newest nodes) and `main.js`.
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
| `engines.tmgr` | `^1.0` up to `^1.10`; newer nodes and permissions need a newer minor (`menus:task` 1.6, pages 1.5, `views:badge` 1.4) |
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

---

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
| `tmgr.pageData.get/set/delete/getMany` | Per-page JSON storage of the plugin, like `taskData`. | `pages:read` (`get`, `getMany`) / `pages:write` (`set`, `delete`) | Value at most 64 KB, key at most 200 characters, `getMany` up to 500 page ids. Own quota of 5 MB / 1000 keys. Local workspaces only. |

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
  plugin, and the server applies the same rules (on a backend that supports pages for plugins). `pageData` has
  no server endpoints and works only in local workspaces. `taskData` has server endpoints, but the desktop app
  does not use them in shared workspaces yet, so `tmgr.taskData` rejects there with `NOT_SUPPORTED`.
- `pageData` of a page is removed when the page is permanently deleted, not when it goes to the trash.

### Page events

`page.created`, `page.updated`, `page.deleted`, `page.restored` and `page.moved` need `pages:read`. The
payload is `{ type, workspaceId, pageId, slug, title, parentId, version, author: { kind, id }, changedSections }`;
`page.deleted` carries only `{ type, workspaceId, pageId }`.

- `changedSections` lists the managed sections whose text changed in this write. In local workspaces the app
  reports the sections whose text differs between the stored version and the new one. Events from shared workspaces arrive over realtime without a body and always
  report `[]`.
- A plugin does not receive the events of its own writes; other plugins and the user's writes are delivered.

## Text and documents (API 1.7)

Three additions let a plugin show longer text and documents. Set `"engines": { "tmgr": "^1.7" }`; older
versions drop `markdown` and `code` nodes and ignore `spans`. Types: `TmgrNode` in `plugin-sdk/template/tmgr.d.ts`.

- `{ type: 'markdown', text }` renders Markdown: headings (levels 4 to 6 show as level 3), paragraphs,
  bold, italic, strikethrough, inline code, line breaks, lists, task lists, tables, code blocks, quotes and
  rules.
- `{ type: 'text', text, spans }` mixes styles in one line of text. Each span is `{ text, bold?, italic?, code?,
  strike? }`; `text` is replaced by the joined span texts.
- `{ type: 'code', text, lang?, wrap? }` shows monospace text with a copy button. `wrap` defaults to `true`.

Limits:

- A `text`, `code` or `markdown` string can be up to 20000 characters; every other string stays at 1000.
- In one tree, the characters past the first 1000 of each long string add up to at most 200000. After that,
  long strings are cut to 1000.
- A Markdown paragraph, heading or table cell longer than 4000 characters shows without inline formatting. If
  one tree has a lot of Markdown, later parts may also show without formatting so the page stays responsive.
- Up to 200 spans per text, 12 columns and 200 rows per Markdown table, and Markdown lists and quotes nest 6 deep.
  Everything counts toward the 3000 node limit of the whole tree.
- Newlines in a `text` node show as line breaks.

What is shown:

- Raw HTML in Markdown shows as plain text.
- Images are never loaded; the alt text is shown instead.
- Only `https` links to a domain in `links.allowedDomains` are clickable, and only with the `links:open`
  permission. `http`, `mailto` and other domains render as plain text. This is the same rule as the `link`
  node.

## Layout, controls and icons (API 1.8)

Set `"engines": { "tmgr": "^1.8" }`. Older versions drop the new nodes and ignore the new fields. Types:
`TmgrNode` in `plugin-sdk/template/tmgr.d.ts`.

- `{ type: 'collapse', title?, open?, visible?, children }`. With `title` the children hide behind a header until
  the user opens it (`open: true` starts open). With `visible` (a whole number from 1) only the first N
  children show, plus an "N more" button that expands and a "Show less" button. Both can be combined. The open
  state stays in the app and is never sent to the plugin.
- `{ type: 'tabs', tabs: [{ id, title, badge?, children }], selected? }`. Up to 12 tabs; extra tabs, and tabs
  with a missing or repeated id, are dropped. Ids are strings of at most 64 characters. `selected` is the start
  tab (default: the first). The chosen tab stays in the app.
- `{ type: 'segmented', options: [{ value, text }], value, command, args? }` is a switch with 2 to 4 options
  (any other count drops the node; repeated values are dropped). Clicking an option runs `command` with
  `{ ...args, value }`, where `value` is that option's own value. `value` is the selected option; update it by
  rendering again.
- `button` gains `variant: 'danger' | 'success' | 'outline'`, `icon`, `iconOnly`, `disabled`, `loading` and
  `tooltip`. `iconOnly` needs an `icon` and shows no text, but `text` stays as the accessible label and the
  default tooltip. A disabled or loading button does not run its command.
- `{ type: 'buttonGroup', children }` puts up to 12 buttons side by side; other children are dropped.
- `badge` gains `icon` and `tooltip`.
- `{ type: 'icon', name, tone? }` shows an icon; `tone` is one of the text tones.
- Tooltips are plain text of at most 200 characters; titles and tab titles are cut to 200.

Icon names (any other name is dropped; a button or badge keeps its text):
`check`, `x`, `plus`, `minus`, `alert-triangle`, `info`, `circle-help`, `clock`, `calendar`, `user`, `users`, `star`, `heart`, `flag`, `tag`, `link`, `external-link`, `copy`, `trash`, `edit`, `pencil`, `search`, `filter`, `settings`, `refresh`, `play`, `pause`, `download`, `upload`, `git-branch`, `git-pull-request`, `bug`, `zap`, `lock`.

Limits: every new node, every tab and every option counts toward the 3000 node limit of the whole tree, and
nesting counts toward the depth limit of 8.

## Input controls (API 1.9)

Set `"engines": { "tmgr": "^1.9" }`. Older versions drop these nodes. Types: `TmgrNode` in
`plugin-sdk/template/tmgr.d.ts`.

- `{ type: 'input', name, placeholder?, value?, command, args?, mode?, debounce? }` is a single-line text field.
  With `mode: 'change'` (the default) the command runs after the user stops typing for `debounce`
  milliseconds (default 300, kept between 150 and 2000). With `mode: 'submit'` it runs when the user presses
  Enter. The text is sent as `args.value`, cut to 500 characters. `placeholder` is cut to 200 characters and
  the start `value` to 500.
- `{ type: 'select', name, options: [{ value, text }], value?, placeholder?, command, args? }` is a drop-down
  of 1 to 100 options (extra options and repeated values are dropped). Choosing one runs `command` with
  `{ ...args, value }`, where `value` is that option's own value.
- `{ type: 'checkbox', name, label, checked?, command, args? }` runs `command` with `{ ...args, value }` when
  toggled, where `value` is `true` or `false`.

Rules for all three:

- `name` identifies the control on your side. It is 1 to 64 characters of letters, digits, `_`, `.` and `-`,
  and is not shown or sent. A node with a bad name or without a valid `command` is dropped.
- A control only sends its own value, as `args.value` of its own command, when the user acts on it. Nothing is
  sent when the control first shows or when you render again, and other controls' values are never included.
- There is no password field and no `type` option. Controls carry `autocomplete="off"`, have no `name` or `id`, are
  not inside a form, and are marked to be ignored by common password managers (`data-lpignore`, `data-1p-ignore`,
  `data-bwignore`, `data-form-type="other"`). Browsers and password managers can still change this behaviour, so
  never ask the user for a secret in an input.
- TMGR shows the line "Plugin: <plugin name>" under every input, select and checkbox, so the user can tell them
  from TMGR's own fields. You cannot hide or change it.
- While the user is typing in an input, the typed text stays; otherwise the value from your latest render is shown.
  Render the new `value` when the command ends.
- Every control counts toward the 3000 node limit; each option counts as well.

## Feeds and data (API 1.10)

Set `"engines": { "tmgr": "^1.10" }`. Older versions drop the new nodes and ignore the new fields. Types:
`TmgrNode` in `plugin-sdk/template/tmgr.d.ts`.

- `{ type: 'timeline', items: [{ at, title?, text, tone? }] }` is a vertical list of events with a dot and a
  line. `at` is shown as given (at most 64 characters), `title` is cut to 200 and `text` to 1000. `tone` is a
  badge color name (default `gray`). At most 200 items.
- `{ type: 'messages', items: [{ author, text, at?, side? }] }` shows chat bubbles, `side` is `'left'`
  (default) or `'right'`. `author` is cut to 64 characters, `text` to 1000 and is plain text (line breaks are
  kept, no Markdown). At most 200 items.
- `{ type: 'sparkline', values, color?, label? }` is a small line chart. `values` holds up to 365 numbers;
  anything that is not a finite number is dropped. `color` is a badge color name (default `blue`). `label`
  (at most 200 characters) is the text screen readers read.
- `{ type: 'empty', title, text?, icon? }` is a placeholder card. `icon` must be one of the icon names above.
- `{ type: 'skeleton', variant?, lines? }` is a loading placeholder. `variant` is `'text'` (default),
  `'card'` or `'table'`; `lines` is 1 to 12 (default 3).

Table additions:

- Column `align`: `'left'`, `'center'` or `'right'`.
- Column `width`: `'xs'` (4rem), `'sm'` (7rem), `'md'` (12rem) or `'lg'` (18rem). Other values are dropped.
- Column `sortable: true`: clicking the header sorts the rows in the app, cycling ascending, descending and
  unsorted. Rows sort by `row.sort[key]` (a string or number) or, without it, by the text of the cell.
  Numbers sort before text. Sorting never sends anything to your plugin.
- Row `color`: a badge color name; the row gets a tint and a colored left edge.
- Row `onClick: { command, args? }`: runs when the row is clicked or activated with Enter or Space. It follows
  the same rules as a button command; an invalid one is dropped. Clicks on controls inside the row do not
  trigger it.
- Row `expand`: an array of nodes (at most 50) shown under the row when the user opens its arrow. Opening
  and closing happens in the app.

Every item, row, value and expanded node counts toward the 3000 node limit of the whole tree, and nesting
counts toward the depth limit of 8. Lists over their caps are cut.

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

## Migrating from 1.6 to 1.7

Nothing changes for existing plugins: `^1.0` to `^1.6` keep working.

To show longer text and documents:

1. Set `"engines": { "tmgr": "^1.7" }`.
2. Use `markdown`, `code` nodes and `spans` on `text` nodes. To make links in Markdown clickable, add the
   `links:open` permission and list the domains under `links.allowedDomains`.
3. Update `tmgr.d.ts` from the SDK template.
4. Copy `plugin-sdk/testing` again and run `npm install`: the test host now needs `marked`, and Node 20.19+ or
   22.12+ (it loads an ES module with `require()`).

## Migrating from 1.7 to 1.8

Nothing changes for existing plugins: `^1.0` to `^1.7` keep working.

To use the new layout and controls:

1. Set `"engines": { "tmgr": "^1.8" }`.
2. Use `collapse`, `tabs`, `segmented`, `buttonGroup` and `icon` nodes, and the new `button` and `badge` fields.
3. Update `tmgr.d.ts` from the SDK template.
4. Copy `plugin-sdk/testing` again so the test host knows the new nodes.

## Migrating from 1.8 to 1.9

Nothing changes for existing plugins: `^1.0` to `^1.8` keep working.

To use the input controls:

1. Set `"engines": { "tmgr": "^1.9" }`.
2. Use `input`, `select` and `checkbox` nodes.
3. Update `tmgr.d.ts` from the SDK template.
4. Copy `plugin-sdk/testing` again so the test host knows the new nodes.

## Migrating from 1.9 to 1.10

Nothing changes for existing plugins: `^1.0` to `^1.9` keep working.

To use the feed and data nodes:

1. Set `"engines": { "tmgr": "^1.10" }`.
2. Use `timeline`, `messages`, `sparkline`, `empty` and `skeleton` nodes and the new table fields.
3. Update `tmgr.d.ts` from the SDK template.
4. Copy `plugin-sdk/testing` again so the test host knows the new nodes.
