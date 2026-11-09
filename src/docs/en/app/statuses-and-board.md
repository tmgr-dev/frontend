---
title: Statuses and the board
section: Web app
order: 23
description: Create custom statuses, learn what each status type does, and manage board columns and the archive.
---

Statuses and the board belong to the Workflow module. Turn it on in [Settings → Modules](/settings/modules). The Board module needs Workflow, so with Workflow off you get a **Done** checkbox on tasks and no board. See [modules and presets](/docs/web/workspace-modules) for presets.

## Statuses {#statuses}

Every task has one status, and every status belongs to the workspace. Each status has a name, a color and a type. The type is what TMGR looks at; the name is yours to choose.

Turning Workflow off does not delete anything. Your statuses and their tasks stay, and come back when you turn it on again. TMGR also makes sure the workspace has the statuses the **Done** checkbox needs:

- No default status: it adds "Backlog".
- No completed status: it adds "Done". If an active status is named "Completed", that status becomes the completed one instead.
- No archived status: it adds "Archived".

## Status types {#types}

- **default**: the backlog. New tasks start here. A workspace can have only one, and it cannot be deleted.
- **active**: work in progress. When you start a timer on a task in the default status, TMGR offers to move it to the first active status.
- **completed**: finished work. It is counted as done on the board, and its column has an **Archive all** action.
- **hidden**: tasks you want to park. They stay on the board in their own column but are left out of the status filter in the list.
- **archived**: tasks you are done with. They have no column on the board and are left out of the current task list. Open them on the Archive page.

A new task never lands in a hidden or archived status by default.

## Create or edit a status {#edit-status}

1. Open the board. On a wide screen, open the ⋮ menu next to **Filters**. On a narrow screen, press the **Filters** button instead.
2. Press **Add status**. The form is titled **Create status**.
3. Enter the status name, pick a type (active, hidden, archived or completed) and a color.
4. Press **Create**.

You cannot create a second default status from this form. To change a status later, open the menu of its column and choose **Edit status**. Change the name, the type or the color and press **Save**. A workspace has only one default status: if you try to make a second one, you get "Workspace already has a default status".

To delete a status, open **Edit status** and press **Delete**. TMGR asks "Are you sure?". The tasks in that status move to the default status. The default status itself cannot be deleted.

## The board {#board}

Each status is a column, except archived ones. The board is a kanban: drag a card by its handle to another column to change its status, or up and down inside a column to change the order. A card's menu also has **Move to top** and **Move to bottom**.

The controls at the top narrow the board:

- The magnifier button (**Search**) opens a box that searches tasks by text.
- **Filters** filters by category, user, persona and priority. The category, user and persona filters need the Projects, Assignees and Personas modules.
- **Sort by priority**, under **Filters**, puts the highest priority on top of each column. While it is on, you cannot change the order inside a column by hand, and **Move to top** and **Move to bottom** do nothing.
- **Clear filters** (the cross) resets all filters.

A column's menu has these items:

- **Create a task**: starts a new task with that status.
- **Edit status**.
- **Move all tasks to…**: moves every task in the column to another status after you confirm. It shows only when the column has tasks.
- **Archive all**: on completed columns only (see below).

Each column also has an **Add task** row where you type a title and add it right there.

## Reorder columns {#reorder}

On a wide screen, open the ⋮ menu next to **Filters** and turn on **Reorder statuses**. Drag handles appear on the column headers. Drag a column to its new place.

On a narrow screen, press **Filters** and choose **Reorder statuses**. Drag the rows in the **Reorder Statuses** window, then press **Save Order**.

## The archive {#archive}

To archive a whole column of finished work, open the menu of a completed column and choose **Archive all**. TMGR asks you to confirm, then moves all its tasks to the archived status. If the workspace has no archived status, you get "Archive status not found": create one first, with type archived.

To archive one task, choose **Archive** in its card menu on the board and confirm in the **Archive task** window. You can also move the task to an archived status.

Archived tasks disappear from the board and from the current list. Open **Archive** from the account menu to find them. To bring a task back, change its status to any other one.

## Next {#next}

- [Tasks in detail](/docs/app/tasks)
- [The task list](/docs/app/task-list)
- [The start-done cycle in the CLI](/docs/cli/the-start-done-cycle)
