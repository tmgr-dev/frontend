---
title: The desktop app
section: Desktop app
order: 26
description: Download TMGR Desktop, start with or without an account, keep it updated, and use the tray timer, windows, links, the widget and the AI usage page.
---

TMGR Desktop is the TMGR app for macOS, Windows and Linux. It does what the web app does, and adds a timer in the menu bar or system tray, quick add from any app, local workspaces that work without an account, and plugins.

## Download {#download}

Download the app from [the latest release on GitHub](https://github.com/tmgr-dev/frontend/releases/latest). Pick the file for your system:

| System | File |
| --- | --- |
| macOS 13.3 or later, Apple Silicon | `.dmg` |
| Windows x64 | `-setup.exe` or `.msi` |
| Linux x64 or ARM64 | `.deb`, `.rpm` or `.AppImage` |

The Windows installer is not code-signed yet. If Windows SmartScreen warns you the first time, choose "More info", then "Run anyway".

## Sign in or stay local {#sign-in}

When the app does not open the demo on first start (see below), you see **Welcome to TMGR** with two buttons:

- **Start without an account** creates a local workspace on this computer. Enter **Your name** if you like (it is optional) and press **Continue**. Nothing is sent to a server. See [Local workspaces and backups](/docs/desktop/local-workspaces).
- **Sign in** opens the sign-in page. Signing in with a social account opens your browser and then brings you back to the app.

You can create an account later with **Create account** in the sidebar. After you sign in, your local workspaces stay on this computer, next to your cloud workspaces. Local ones carry the **Local** badge.

## The demo {#demo}

When you open the app for the first time on a computer, it creates your own workspace, "My workspace", and a separate demo workspace, and opens the demo. In the demo a simulated agent fixes a bug: the card moves, the timer runs and the agent run fills in. Nothing runs on your computer.

The demo has a banner with **Delete demo**. Press it, then confirm. The demo is never backed up, cannot be moved to the cloud, and agents cannot connect to it.

If you already use the app without an account, it may offer the demo once: **Want a quick tour with sample data?** Press **Watch the demo** or **No thanks**.

## Updates {#updates}

The app checks for updates shortly after it starts and then every 6 hours. It downloads a new version in the background. When the update is ready, the status bar at the bottom of the window shows **Update to v…**. Press it to install the update and restart.

To check right now:

- On macOS, choose File → "Check for Updates…" in the menu bar.
- On Windows and Linux, choose "Check for Updates…" in the tray menu.

If a new version is there, press **Install and restart**.

After an update, **What's new** opens with the notes of the new version. To open it again, press the version number (v…) in the status bar. Some notes have a **Take the tour: …** button that shows the new feature.

## The tray and the menu bar {#tray}

Closing the main window does not quit TMGR. On macOS it stays in the menu bar, and the menu bar shows the running timer: the elapsed time and the task. On Windows and Linux it stays in the system tray.

Click the icon to open the menu:

- Each running task, with "Stop timer" under it. Click the task to open it.
- "Switch to" and your recent tasks. Click one to move the timer to it.
- "Open TMGR" shows the main window.
- "Decisions (N)" opens the Decisions window, on macOS and Linux. See [The Decisions inbox](/docs/cli/the-decisions-inbox).
- "Do Not Disturb", see below.
- "Shortcuts…" opens the shortcut settings.
- "Launch at Login" starts TMGR when you log in to your computer.
- "Check for Updates…", on Windows and Linux.
- Items that plugins add, each under the plugin name.
- "Quit TMGR".

### Do Not Disturb {#dnd}

Do Not Disturb silences the system notifications of TMGR and of its plugins. In the tray menu, open "Do Not Disturb" and choose "For 1 hour", "For 3 hours" or "Until tomorrow" (until 9:00). "Off" ends it.

When you are signed in, the bell in the status bar does the same. Its tooltip is **Do not disturb**, and the bell is crossed out while it is on.

## Task and page windows {#windows}

A task or a page can have its own window:

- In an open task, press the **Open in a separate window** icon button.
- On a page, press **Open in window**.

The window stays in sync with the main window. If a task already has its own window, opening the task again brings that window to the front.

## tmgr:// links {#links}

TMGR Desktop opens `tmgr://` links from other apps:

- `tmgr://task/<workspace>/<id>` opens a task. **Copy app link** in the **Share** menu of a task copies this link.
- `tmgr://page/<workspace>/<slug>` opens a page. The **Share** menu of a page has **Copy app link** too.
- `tmgr://open?path=/<workspace>/board` opens a screen of the app, such as the list, the board, the dashboard, pages or a task.
- `tmgr://plugin/…` links belong to plugins. Before such a link runs a plugin command, the app asks you: **Allow once** or **Always**.

A link to a task on `https://tmgr.dev` that you click inside the app also opens in the app.

In the web app, the **Share** menu of a task page or a page has **Open in TMGR app**.

### The Open in TMGR Desktop bar {#open-in-desktop}

In the web app on a Mac, a bar can appear at the top: "You use TMGR Desktop." It shows when you are signed in and used the desktop app with the same account in the last 30 days. Press **Open in TMGR Desktop** to open the same screen in the app. Close the bar with the cross. It stays hidden until you sign in again.

## The macOS widget {#widget}

On macOS you can add a TMGR widget to the desktop or to Notification Center. Right-click the desktop or open Notification Center, choose "Edit Widgets", and find TMGR. The widget is called "Timer" and comes in a small and a medium size.

It shows the running task with a live elapsed time, "+N more" when several timers run, or "No timer running". You cannot start or stop a timer from it. The widget has no network access.

## AI usage {#ai-usage}

The [AI usage](/ai-usage) page shows what your AI coding tools use and cost. Open it with **AI usage** in the **Tools** group of the sidebar. The page is in the desktop app only.

TMGR reads the session logs that Claude Code and Codex write on this computer. The page shows tokens and cost for a period, your **Limits**, a table **By tool and model**, and **Tasks**: usage matched to your tasks by agent runs and git branches. Cost is API-equivalent (list prices), not your subscription bill. Sessions and hourly detail are kept for 90 days; older usage stays as daily totals for up to 730 days.

Claude Code limits need its status line hook. See [What AI costs you](/docs/cli/what-ai-costs-you).

### Allow reading the logs {#usage-consent}

The first time, the page asks **Read AI tool logs?**. Tick Claude Code, Codex or both, and press **Allow**. **Not now** keeps reading off.

For AI usage, TMGR keeps token counts, cost, model names, timestamps, session ids, project folders and git branches, the names of tools, skills and commands the AI used, the file types it edited and the pull requests it opened. It never stores what you or the AI wrote. Nothing leaves this computer unless you set up uploads with `tmgr usage sync`.

Your choice also covers background jobs, agent notifications, the tmgr CLI and the TMGR runner on this computer. With reading on, the runner reads the sessions it starts: it saves the questions it finds there, with your prompt and the reply, on this computer, and may send reply text to Claude (Haiku) to spot questions. To change it, open [Settings → Privacy](/settings?tab=privacy) and find **AI usage**:

- **Read AI tool logs** turns reading on or off.
- **Forget scanned data** deletes what TMGR keeps on this computer, the runner's saved questions included, and turns reading off. It also turns off uploads; what was already uploaded is not deleted. The logs of your AI tools stay as they are.
