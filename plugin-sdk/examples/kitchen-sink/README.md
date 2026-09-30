# Kitchen Sink

An example plugin (not built in) that touches every plugin API v1.4 feature, for manual acceptance
testing of the desktop app. Not a template to build a real plugin from — see `plugin-sdk/template/` for
that.

## Automated coverage

`node plugin-sdk/template/validate.mjs plugin-sdk/examples/kitchen-sink` checks the manifest and files.
The app's own Jest suite (`src/pluginSystem/__tests__/kitchenSink.test.ts`) loads the manifest through
`parseManifest` and drives `main.js` through `plugin-sdk/testing`'s `createTestHost` for the setup,
sample-task and badge/alarm flows. Everything below is for a human, in the real app.

## Acceptance scenario

1. Copy this folder into `~/.tmgr.dev/plugins/kitchen-sink` (any folder name works; the plugin's own id
   comes from `manifest.json`).
2. In the app: **Settings → Plugins → Developer mode → on**, then **Reload plugins**. "Kitchen Sink"
   appears in the list, switched off (folder plugins always start off).
3. Turn it on. Its status should read **Running**.
4. Click **"Set up workspace"** (a command button on its card). Expect:
   - A status named **"Needs answer"** (active type) exists (Settings → Statuses, or the board's status
     switcher).
   - A category with code **KS** exists.
   - A toast: "Status and category are ready."
5. Click **"Create sample task"**. Expect:
   - A new task titled "Kitchen sink sample task" in the KS category, status "Needs answer", priority
     High, due tomorrow. Its key is `KS-1` (or the next free number if you clicked this more than once).
   - A comment "Created by the Kitchen Sink plugin." on it, shown as posted by the Kitchen Sink plugin,
     with a 👍 reaction already on it.
   - A "relates to" link to another task (a small "Kitchen sink reference task" is created for this on
     the very first run).
   - Opening the task: a **"Kitchen Sink"** section in its side panel with a copyable fake command, a
     key/priority table, a "time ago" (updated) and a due-time line, a link to example.com, and a "Say
     hi" button that asks to confirm before running.
   - The task's card shows two badges: its priority (e.g. **HIGH**, red) and **KS** (blue) — the KS badge
     only appears once `taskData` has been set, i.e. after this command has run for that task.
   - The board's quick filters gain a **"Touched by Kitchen Sink"** filter (from `contributes.boardFilters`),
     which matches tasks with the KS badge.
6. The menu bar (status bar) shows an item like "N KS tasks"; clicking it re-runs "Create sample task".
7. The "Kitchen Sink view" item in the sidebar's Plugins section shows a counter with the number of active
   sample tasks (API 1.4 `setViewBadge`); it is set again on every tray refresh and cleared when the plugin
   is turned off.
8. In the app tray/menu bar icon, a "Kitchen Sink" section lists the same tasks (see "Manual-only checks"
   for the parts of tray behaviour Jest cannot see).
8. Visit `tmgr://plugin/tmgr-dev.kitchen-sink/command/ping` (e.g. paste it into a browser, or use
   whatever the OS's URL-open mechanism is) — a "Pinged via a tmgr:// deep link." notification appears.
9. Visit `tmgr://plugin/tmgr-dev.kitchen-sink/view/view?task=42` — opens the declarative "Kitchen Sink
   view" and shows "Opened from a deep link for task 42". Below that, a "Card & grid demo (API 1.3)"
   section shows 4 accented lanes (blue/yellow/purple/green) as a horizontally-scrolling grid, each with
   a heading row (badge + "More" menu, one item asks to confirm), 3 nested cards (whole card clickable),
   and a row of clickable stats — all run `tmgr-dev.kitchen-sink.cardClicked`, which logs and notifies.
10. Open "Kitchen Sink window" from the plugin's card — a separate window opens showing the task count,
    read through `window.tmgr`.
11. Switch to a different workspace and back — the plugin's log (Settings → Plugins → its card → Log)
    should show `kitchen-sink: workspace.switched from … to …`.
12. In a LOCAL workspace, add a note under Daily Routines with no date, then click **"Capture today's
    notes"** on the plugin's card. Expect: a new task with the note's title in the KS category, a toast
    naming it, the note gone from Daily Routines, and the log showing
    `kitchen-sink: routine.created …` for the note's creation. Routines are local-workspace only: the
    command does nothing useful in a shared workspace (`routines.*` rejects with `NOT_SUPPORTED`).

## Manual-only checks

These cannot be exercised by `createTestHost` (it has no real notification, tray, or persistence across
restarts):

- **Native notification click**: click the OS notification for step 8 (when the app window is not
  focused) and confirm it focuses the app.
- **Alarm while hidden / after restart**: the `ks-tick` alarm (every 5 minutes) should still fire and
  refresh the status bar/tray count while the app window is hidden, and its schedule should survive
  quitting and reopening the app (alarms are persisted, not held in memory).
- **Tray menu**: opening the actual menu bar icon and confirming the "Kitchen Sink" section and its task
  items are there and clicking one focuses that task.
- **Menu bar text**: pick "Kitchen Sink" under Settings → Plugins → "Menu bar text" and confirm "KS N"
  appears next to the timer; without picking it, `setTrayTitle` is refused and only logged as a warning.
