---
title: Dashboard and files
section: Web app
order: 28
description: See what happens in the workspace on the dashboard, and find every file attached to its tasks on the Files page.
---

The dashboard shows what the whole workspace did: activity, members, tracked time and a feed of events. The Files page collects every file attached to the tasks of the workspace.

## Open the dashboard {#open}

Open **Dashboard** in the **Main** group of the sidebar. The Dashboard module turns it on. Only the workspace owner can switch modules, in [Settings → Modules](/settings/modules). If the module is off, you see a preview of the dashboard instead. The dashboard is not available in a local workspace of the desktop app.

Every number on the dashboard is for the whole workspace, not only for you. The layout is fixed: you cannot hide or move the blocks.

At the top, pick **Last 7 days** or **Last 30 days**. The period changes the numbers at the top, the chart, the top categories and the window of the team block. **Refresh** loads the data again.

## The numbers at the top {#kpis}

- **Activity**: actions people made in the workspace.
- **Active members**: members who did at least one thing in the period, out of all members.
- **Tracked hours**: time on task timers. It shows only while the Timer module is on.
- **Completed**: tasks moved to a done status.

Some numbers also show the change against the previous period of the same length. The per-day line and the change of **Tracked hours** need the Reports & export module too.

## Active members {#team}

The **Active members** block lists the members and what they did. Pick its window: **Today**, **7d**, **30d** or **All time**. Above the list you see how many members are active now.

For each member you see their activity, done tasks, tracked time, active tasks and when they were last active, plus the task they are working on or worked on last. Click the task to open it. Click the member to open their page; see [The member page](/docs/app/team#member-page).

## Activity over time {#chart}

**Activity over time** draws one line per day for activity, active members and tracked hours. Each line is scaled to its own peak, so compare their shapes, not their heights. The tracked hours line needs the Timer and Reports & export modules.

## Activity heatmap {#heatmap}

**Activity Heatmap** shows one square per day. The darker the square, the more people did that day. It counts actions of people only, not of personas or plugins. Days are counted in UTC.

Pick **Last 12 months** or a year above the grid. Above it you also see the current and the longest streak of active days.

Click a day to show only that day in the **Recent Activity** feed. Click the same day again to show everything.

## Statistics and top categories {#statistics}

**Workspace Statistics** has cards with task counts, tracked time and team size. The period at the top does not change them.

**Top categories** lists the five categories with the most tracked time in the period. It shows only while the Timer and Reports & export modules are on.

## Recent Activity {#feed}

**Recent Activity** is the feed of what happened in the workspace, newest first.

- Pick a member in **All Users** to see only their actions.
- When the feed has actions of personas or plugins, chips appear: **All**, **People**, **Personas**, **Plugins**. They filter the entries already loaded.
- Press **Load More Activities** to see older entries.

Agent work shows in the feed only while the Agent work module is on.

## Make the dashboard your start page {#landing}

You can open the dashboard first when you sign in. Open [Settings → Modules](/settings/modules) and find **Personal preferences**. In "Default Landing Page", choose "Dashboard". This setting is yours only; it does not change anything for other members.

The dashboard can be your start page only while the Dashboard module is on. If you switch to a workspace where it is off, the setting goes back to "List".

## Files {#files}

Open **Files** in the **Tools** group of the sidebar, or find it in the [command palette](/docs/app/search-and-shortcuts#palette). The Attachments & files module turns it on.

The page lists the files attached to the tasks of the workspace, newest first, including files added to comments. Files of pages are not here; they are in the **Files** list of each page, see [Pages](/docs/app/pages#files).

- Switch between **All files** and **Images** at the top.
- Each row shows the file name, its size and the task it belongs to. Click the task to open it.
- Click an image to open it in the viewer.
- Press **Download** to save a file.
- The page shows 40 files at a time. Use **Previous** and **Next** to see more.
