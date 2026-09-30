# TMGR plugin system

Notes for plugin authors on the UI calls that reach the user outside a plugin's own page. Types live in
`plugin-sdk/template/tmgr.d.ts`; the test harness is `plugin-sdk/testing`.

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
