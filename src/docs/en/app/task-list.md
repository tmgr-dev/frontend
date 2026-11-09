---
title: The task list
section: Web app
order: 22
description: Search, filter and page through tasks, select several at once for batch actions and export, and look through the archive.
---

The list shows the current tasks of the workspace. Open it with **List** in the sidebar. Archived tasks are not here; they are on the [Archive page](#archive). Click a task to open it; what a task holds is in [Tasks in detail](/docs/app/tasks).

## Search and filters {#filters}

Type in the search box at the top. The list updates as you type and matches task titles and descriptions.

The sliders button next to it opens **Filters**:

- A category picker, when the Projects module is on.
- **Status**: pick one or more statuses. It shows when the Workflow module is on.
- **Priority**: pick one or more levels. **Sort by priority** puts the highest priority first.
- **reset** clears these filters.

The button is highlighted while a filter is on. If nothing matches, you see "No tasks match your filters" and a **Clear filters** button.

With the Workflow module off, buttons above the search choose which tasks to show: **Open**, **Done** or **All**. Next to them, **Archive done** moves every done task to the archived status after you confirm in the **Archive done tasks** window.

## Pages and totals {#totals}

The list shows 10 tasks per page. When there are more, the bottom of the list shows "Showing 1 to 10 of … tasks", a choice of **10 per page**, **25 per page** or **50 per page**, and **Previous** and **Next**.

With the Timer module on, the top of the list shows how many tasks match and their total tracked time. If tasks went past their estimates, the total overtime follows in red.

## One task in the list {#row}

Each row shows the category, the title, the priority badge, the tracked time and the status. With the Workflow module off, a **Done** checkbox takes the place of the status.

- The play button (**Start timer**) starts the timer; while it runs, the button stops it (**Stop timer**).
- The assignee button (**Assign someone** or **Change assignee**) adds or removes people without opening the task.
- The ⋮ menu (**Task actions**) has **Open details**, **Settings** and the **Share** submenu. With the Workflow module off, it also has **Archive**, or **Restore** on an archived task.

## Select several tasks {#select}

1. Press the selection button (**Tasks selection mode**). A dashed frame appears around the list.
2. Drag across the rows to select them, or press **Select all**. Hold Ctrl (Cmd on a Mac) while you drag to add more rows. Next to the buttons you see how many tasks you picked and their hours.
3. Press **Process (n)**, where n is the number of selected tasks. The **Batch Actions** window opens with the selected count and their total time.
4. Pick an action in the drop-down list and press **Execute**.

Press Esc or the selection button again to leave selection mode.

### Batch actions {#batch}

- **Move to status**: every status of the workspace. With the Workflow module off, this group is **Update tasks** with **Mark done**, **Mark open** and **Archive**.
- **Delete**: only on the [Archive page](#archive), after you confirm.
- **Confirm for agent runs**, in the **Agent runs** group: shows when **Let my agents run my tasks** is on in [Settings → Modules](/settings/modules). It shows exactly what agents will read from each selected task, so you can confirm it. See [agent work](/docs/app/agent-work).
- **Export to CSV**, **Export to JPEG** and **Export to XLSX**: see below.

## Export {#export}

Select tasks, press **Process (n)**, pick a format in the **Export** group and press **Execute**. The file downloads when it is ready.

The file has a row for each task: title, description with its checkpoints, tracked time, expected time and overtime, all in hours, plus a summary row. **Export to JPEG** draws the same table as an image. Only tasks you created go into the file.

With the Reports & export module on, the **Batch Actions** window also has **Export Settings**:

- **Round hours up**: every time in the file is rounded up to the next tenth of an hour.
- **Zero expected hours on off-days**: a task that falls on an off-day gets no expected time, so all its time counts as overtime.
- **Weekend days:** the days that count as off-days. Saturday and Sunday are checked at first.
- **Custom holidays:** pick a date and press **Add** to count it as an off-day too.
- **Parse date from task title**: a task whose title is only a date, like `2026-11-09`, is dated by its title. Other tasks are dated by when they were created. It is on at first.

The settings apply to the export you run from that window.

## The Archive page {#archive}

**Archive** in the sidebar opens archived tasks in the same layout. It differs from the current list in a few ways:

- A date sort: **Updated**, **Created**, **Deadline** or **Scheduled**. The arrow button next to it switches between **Newest first** and **Oldest first**.
- With the Timer module on, a summary shows **Total Time**, **Working Days** (8h/day), **Working Months** (160h/month), **Working Years** (2000h/year) and **Overtime**.
- The batch actions include **Delete**.
- **Filters** has only the category picker.

How tasks get archived is in [Statuses and the board](/docs/app/statuses-and-board#archive).

## Day summary {#day-summary}

In the [desktop app](/docs/desktop/app), the list has a **Day summary** button at the top. It collects today's tasks from your workspaces. Press **Summarize** to get a short summary written by the local AI model you set up in the desktop app; the window says where task titles, statuses and comment excerpts are sent. Without a local model, you get a plain summary marked **No AI** and a **Set up local AI** link. **Copy** copies the summary; in a local workspace, **Save as page** saves it as a page.

## Next {#next}

- [Tasks in detail](/docs/app/tasks)
- [Statuses and the board](/docs/app/statuses-and-board)
- [Categories](/docs/app/categories)
