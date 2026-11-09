---
title: Pages and the map
section: Web app
order: 25
description: Keep notes and documentation in a tree of pages linked to tasks, and see how everything connects on the workspace map.
---

Pages is the wiki of your workspace. It holds notes, context and documentation in a tree that people and agents both edit. The Map shows how your tasks, pages and people connect.

## Turn Pages on {#turn-on}

Pages and the Map are modules, and both are off by default. The Personal, Team and Developer presets leave them off. The Everything preset turns them on.

Only the workspace owner can switch modules. Open [Settings → Modules](/settings/modules) and turn on Pages. The Map is the Graph module, and it needs Pages. For more about modules and presets, see [Workspace modules](/docs/web/workspace-modules).

If Pages is off, the Pages section shows a preview and a short description of the module instead of your pages. If you are not the owner, ask the owner to turn it on.

## Create and nest pages {#create}

Open Pages in the menu. Press **Create page** to add a page. The sidebar has **New page**, where you can also choose a page type: Plain page, Context, Person or Meeting.

Type a title, then write in the editor:

- Type `/` to insert a block.
- Type `@` to link a task, a page or a member.
- Type `[[` to link a page.

The page saves by itself. The header shows "Saving..." and then "Saved, version N".

In the page tree, open the page actions menu to use these:

- **Add subpage** creates a page inside this one.
- **Rename** and **Pin**. Pinned pages appear first, in the **Pinned** list.
- **Open in window** opens the page in its own window. It is in the desktop app only.
- **Archive** moves the page and its subpages to the Archive. See [Archive](#archive) for who can do it.

Drag pages in the tree to reorder them or to move them under another page.

You can also bring Markdown in and take it out. **Import** accepts Markdown files, images or a .zip. **Export all** exports the whole tree, and **Export with subpages** exports one branch.

## Link pages and tasks {#link-tasks}

There are three ways to connect a page and a task:

- Type `@` in the page and pick the task. The page now links to it.
- Select some text on the page and press **Make a task**. Choose a category and, if you want, a status. The text becomes a link to the new task.
- Create the page from the terminal with `tmgr note --task`. See [From the terminal](#cli).

The task shows the pages that mention it under **Mentioned on pages**. The page shows pages that link to it under **Backlinks**, next to the **Contents** list.

## Page types and properties {#properties}

Under the title, a page shows its type, its **Author**, and who changed it last (**Edited by**). Person and Meeting pages also have fields of their own:

- **Person**: **Member** links the page to a member of the workspace. **Relationship** is Operational, Personal or Strategic. **Company** and **Role** are plain text. You cannot edit **Last contact**: tmgr fills it in.
- **Meeting**: **Date**, **Participants** (people pages or members) and **Tasks** related to the meeting. A new Meeting page starts with the headings Agenda, Outcomes, Decisions and Action items.

**Aliases** on a Person page list the person's accounts in other places. Press **Add alias**, choose a source (Telegram, Rocket.Chat, Slack, GitHub, Email or Other), then fill in **Identifier** and **Display name**. **Remove alias** deletes one.

## Action items {#action-items}

On a Meeting page, every list item under the Action items heading also appears in the side panel under **Action items**. Press **→ task** next to an item to turn it into a task. The **Make a task** dialog opens; choose a category and, if you want, a status. The item on the page becomes a link to the new task. Items that are only a task link already are not listed.

## Files {#files}

The **Files** list in the side panel shows the files of the page. Press **Upload** or drop files on the list to add them. Images you add in the editor appear here too. Click a file to open it.

## Follow a page {#follow}

Press **Follow** in the page header to get notifications about the page:

- when someone changes the page;
- when someone adds a subpage to it.

Several changes in a row bring one notification, not one for each save. You never get a notification about your own changes. **Unfollow** stops these notifications. People who edited the page before get change notifications even if they do not follow it, and a member you mention with `@` gets a mention notification. More in [Notifications](/docs/app/notifications).

## Version history {#history}

Every saved change becomes a version. Press **History** on a page to see the list. Pick a version and choose how to look at it:

- **Snapshot** shows the text of that version.
- **Changes since this version** compares it with the current text.
- **What this version changed** compares it with the version before.

**Restore** creates a new version with the old content. The current text stays in the history, so nothing is lost.

If someone saves the page while you are editing, you see "Page changed elsewhere". Choose **Take theirs** to load their version, or **Keep mine** to save your text over it.

## Archive {#archive}

An archived page waits in the Archive. Open it with the **Archive** button in the Pages header. Each entry has **Restore** and **Delete permanently**. **Delete all** empties the Archive, and you have to type a word to confirm. Permanent deletion cannot be undone.

Archiving, restoring and deleting take the whole branch: the page and its subpages. A member can do it only if they are the author of every page in the branch. Otherwise only the workspace owner can. **Delete all** removes only the pages you are allowed to delete; the rest stay in the Archive, and you see "Some pages were kept".

## The workspace map {#map}

Open **Map** in the menu. It draws the tasks and pages of the workspace as clusters, one cluster per category. Bright curves are links between areas. Dashed rings are items with no links. Orange marks a bottleneck.

- Search for a task, a page or a person in the box above the map.
- Pick a time range: Last 7 days, Last 30 days, Last 90 days or All time. The map shows items created or updated in that range.
- Turn **Group by category** on or off.
- Zoom in, zoom out or fit the map to the screen with the buttons on the right.
- Click an item to open it.

If the Graph module is off, the Map is not available. With it on, tasks and pages also get a Graph section.

## From the terminal {#cli}

`tmgr note` opens your `$VISUAL` or `$EDITOR`, then saves what you wrote as a page. Without `--title`, the title is the first line that is not empty, with any leading `#` removed.

```sh
tmgr note --title "Release plan" --task TM-123 --parent 42
```

`--task` links the page to a task and `--parent` puts it under another page. An empty note is not saved.

To edit pages as Markdown files, pull them into a folder:

```sh
tmgr page pull ./pages
tmgr page status ./pages
tmgr page push ./pages
```

`pull` writes every page of the workspace, or one branch with `--tree <page-id>`. `status` shows which files changed on your side or on the server. `push` uploads the files you changed; without arguments it uses the current folder. It updates pages you pulled and does not create new ones.

If a page changed on both sides, `push` does not overwrite it. It saves the server copy as `<name>.remote.md`. Merge the two, set the `version:` line in the front matter to the server version, delete the remote file and push again. `pull` keeps files with local changes unless you add `--force`.

More about the terminal: [Command line](/docs/cli) and [Install and sign in](/docs/cli/install-and-sign-in).
