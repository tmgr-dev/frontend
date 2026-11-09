---
title: Plugins
section: Desktop app
order: 29
description: Install plugins from GitHub, turn them on per workspace, and stay in control with signatures, permissions and safe mode.
---

Plugins add things to the TMGR desktop app: badges on board cards, pages in the sidebar, sections in the task panel, commands, text in the status bar, items in the tray menu and in the task … menu, and notifications. You install and manage them in the desktop app.

In a cloud workspace, the web app also runs the plugins that the workspace creator turned on, from the same signed bundle. A plugin that reaches your computer (network, files, tray, links, pictures from other sites) or needs alarms, the task menu, routines or counters on its sidebar items does not run on the web. Its own HTML pages are not shown on the web either.

Manage them in [Settings → Plugins](/settings/plugins). [How plugins work](/settings/plugins/help) explains each permission in pictures.

TMGR comes with one built-in plugin, "Estimate vs actual". It compares tracked time with the estimate.

## What a plugin can do {#permissions}

Each plugin runs in its own sandbox. It can do only what its card lists under **Can:**, for example "read tasks" or "create and change tasks". The list is grouped: what reads your data, what changes your data, what changes the interface, and what reaches outside the app.

- Network: a plugin can connect only to the addresses on this computer that it declares. The card shows them in red.
- Links and pictures: the card lists the sites it may open in your browser and the sites it loads pictures from.
- Its own pages open in a separate window that reaches nothing else. Plugin windows are not available on Windows yet.

**Menu bar text** (**System tray text** on Windows and Linux) lets one plugin show text next to the timer.

## Install from GitHub {#install}

1. In **Install from GitHub**, paste the repository link or `owner/name` into **Plugin repository** and press **Check**.
2. TMGR reads the latest release of the repository. Every release must be signed by its author, and an unsigned one is refused.
3. A dialog asks whether to install the plugin. It shows where the plugin comes from, who published it, and what it can do. Read it and press **Install**.
4. Installed plugins start switched off. Turn the plugin on with the switch on its card. The switch is per workspace: turn it on in each workspace where you want it.

**Check for update** on the card looks for a new release. The update dialog shows what is new, including new permissions. **Remove** deletes the plugin from this computer. Its data in workspaces stays.

## Verified and unverified publishers {#publishers}

- **Verified publisher**: the release is signed with the key listed in the TMGR catalog.
- **Unverified publisher**: the release is signed, but the author is not in the TMGR catalog. Install it only if you trust the author.

On the first install TMGR remembers the publisher's key. An update signed with another key is refused. A plugin installed from one repository cannot be replaced by a release from another repository.

## Shared workspaces {#shared}

Plugins run in local workspaces, and in a cloud workspace once its creator turns them on there:

- Only the creator of a cloud workspace turns plugins on. A plugin turned on there runs for every member, and everyone gets the same version.
- The plugin uses its own short-lived key: this workspace only, and its permissions only.
- On your computer, its network, files, tray items, `tmgr://` links and web links wait until you press **Allow on this computer** on its card. **Stop allowing** takes that back.
- What the plugin saves is shared by all members.

Plugins loaded from a folder in developer mode cannot be turned on in a cloud workspace.

## When a plugin gets in the way {#trouble}

- Switch the plugin off on its card, or remove it.
- **Safe mode** turns every plugin off at once. Starting the app with `--safe-mode` does the same. On macOS, quit TMGR and run `open -a TMGR --args --safe-mode` in the Terminal. While the app runs with `--safe-mode`, agents cannot connect to local workspaces.
- A plugin that fails three times in five minutes turns itself off. Its card shows **Turned off after errors**, and you get a notification. **Log** and **Copy diagnostics** on the card help to find out why.
- TMGR keeps a signed blocklist. A harmful plugin on it is turned off everywhere, shows **Blocked by TMGR**, and cannot be installed or turned on.

## Developer mode {#developer-mode}

**Developer mode** loads unsigned plugins from `~/.tmgr.dev/plugins`. They start switched off.

To write a plugin, read [the plugin guide](/docs/plugins.md) or [build one with your agent](/docs/agents#plugins).
