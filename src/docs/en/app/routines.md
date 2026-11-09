---
title: Routines in detail
section: Web app
order: 26
description: Every routine view, the routine menu, converting a routine to a task, calendar files, categories, and routines from the terminal and Telegram.
---

Routines are things you repeat: every day, every week or once in a while. The basics, quick add, the routine form and ticking a routine done, are in the lesson [4. Routines](/docs/web/routines). This page covers the rest.

## Where routines live {#where}

Routines are a module, the Routines module. The workspace owner turns it on in [Settings → Modules](/settings/modules). With it on, the sidebar has **Daily Routines**.

Routines are personal. The page shows only the routines you created, from every workspace where you are a member and the module is on. A new routine goes to the workspace you are in.

## Views {#views}

Switch views at the top of the page: **List**, **Day**, **Week**, **Month** or **Year**. The page remembers the last view you used.

- **List** shows today: **Unscheduled** routines without a time, then **Today** with timed routines in time order.
- **Day** shows the day's unscheduled routines, then a 24-hour grid. Click an empty slot to create a routine at that time.
- **Week** shows seven days, Monday first. Routines without a time are in the **ALL-DAY** row. Click an empty slot to create a routine there.
- **Month** shows up to three routines a day, then "+ N more". Click a day to create a routine on that date.
- **Year** shows twelve small calendars. The darker a day, the more of its routines you completed. Click a day to open it in the Day view.

Every view except List has **Today** and arrows for the previous and next day, week, month or year.

In the Day, Week and Month views, clicking a routine marks it done. In the Day and Week views, the pencil on a routine opens it. In the List view, clicking a row opens the routine. In every view, right-click opens the [routine menu](#menu).

### Move and resize {#move}

With a mouse, press and hold a row of the List view or a timed routine in the Day or Week view, then drag it to another time or day. You can drop it on an hour slot, a day of the month, the **ALL-DAY** row or **Unscheduled**. For a repeating routine, only that one occurrence moves, and it cannot be moved to **ALL-DAY** or **Unscheduled**.

In the Day view, drag the top edge of a routine to change its start, or the bottom edge to change its length. A new start changes that day only. A new length changes every occurrence of the routine.

On the page, press `N` to open a new routine and `/` to jump to the quick add line. See [Search and shortcuts](/docs/app/search-and-shortcuts).

## The routine menu {#menu}

Right-click a routine, or press the pencil on a row in the List view. The menu has:

- **Edit** opens the routine form.
- **Duplicate** makes a copy with the same schedule, category and reminder. Its title ends with "(copy)".
- **Convert to task** turns the routine into a task. See [Convert to task](#convert).
- **Archive** hides the routine. The app has no place to see or restore archived routines, so archive only what you no longer need.
- **Delete** removes the routine and all its occurrences. You have to confirm, and it cannot be undone.

When some unscheduled routines are done, the **Unscheduled** section shows **Archive done (N)**. It archives the completed one-time routines.

## Convert to task {#convert}

Choose **Convert to task** in the routine menu, or open the routine and press **Move to a workspace as a regular task…**. Pick the **Workspace** and the **Category** (a project of that workspace, or **No category**), then press **Convert**.

The routine becomes a regular task in that workspace, with the first active status. It stops repeating and leaves the Routines page. The app then asks "Open the task now?"

## Import and export calendars {#ics}

The toolbar has **Import** ("Import .ics calendar") and **Export** ("Export .ics calendar"). They are not available in a local workspace of the desktop app.

- **Import** reads an `.ics` file. It shows only while the Import module is on. Each event becomes a one-time routine on the event's date. The time of day and the repeat rule are not kept, and importing the same file twice creates the routines twice. A message tells you how many routines were imported.
- **Export** downloads an `.ics` file with your repeating routines from all your workspaces.

## Routine categories {#categories}

A routine has one of six fixed categories: Work, Health, Learn, Home, Social or General. General is the default. Pick one in the **Category** section of the routine form. The category sets the colour of the routine on the page.

You cannot add, rename or delete routine categories. They are separate from the categories of your tasks, see [Categories](/docs/app/categories).

## From the terminal {#cli}

`tmgr routine list` lists the routines due today: a tick for done ones and a circle for the rest. Add `--date` for another day:

```sh
tmgr routine list
tmgr routine list --date 2026-12-01
```

More commands: [More commands](/docs/cli/more-commands).

## From Telegram {#telegram}

Once Telegram is linked, the tmgr bot can add routines. Send `/add` and a title to create a routine at once. Send plain text or a voice message to get a draft first, then press Confirm or Cancel. The routine goes to your default workspace without a time, so it shows under **Unscheduled**. The Telegram bot module has to be on in that workspace. Setup: [Integrations](/docs/app/integrations#telegram).
