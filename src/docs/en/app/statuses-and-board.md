---
title: Statuses and the board
section: Web app
order: 22
description: Create custom statuses, learn what each status type does, and manage board columns and the archive.
---

Statuses and the board belong to the Workflow module. Turn it on in [Settings → Modules](/settings/modules). The Board module needs Workflow, so with Workflow off you get a Done checkbox on tasks and no board. See [modules and presets](/docs/web/workspace-modules) for presets.

## Statuses {#statuses}

Every task has one status, and every status belongs to the workspace. Each status has a name, a color and a type. The type is what TMGR looks at; the name is yours to choose.

Turning Workflow off does not delete anything. Your statuses and their tasks stay, and come back when you turn it on again.

## Status types {#types}

- **default**: the backlog. New tasks start here. A workspace can have only one, and it cannot be deleted.
- **active**: work in progress. When you start a timer on a task in the default status, TMGR offers to move it to the first active status.
- **completed**: finished work. It is counted as done on the board, and its column has an Archive all action.
- **hidden**: tasks you want to park. They stay on the board in their own column but are left out of the status filter in the list.
- **archived**: tasks you are done with. They have no column on the board and are left out of the current task list. Open them on the Archive page.

A new task never lands in a hidden or archived status by default.

## Create or edit a status {#edit-status}

1. Open the board and press Filters.
2. Press Add status. The form is titled Create status.
3. Enter the status name, pick a type (active, hidden, archived or completed) and a color.
4. Press Create.

You cannot create a second default status from this form. To change a status later, open the menu of its column and choose Edit status. Change the name, type or color and press Save.

To delete a status, open Edit status and press Delete. TMGR asks "Are you sure?". The tasks in that status move to the default status. The default status itself cannot be deleted.

## The board {#board}

Each status is a column, except archived ones. The board is a kanban: drag a card by its handle to another column to change its status, or up and down inside a column to change the order. While Sort by priority is on, the order inside a column follows priority.

Use Filters at the top to narrow the board: search by text, project, user, persona and priority. Sort by priority puts the highest priority on top. Clear filters resets all of them.

A column's menu has these items:

- Create a task: starts a new task with that status.
- Edit status.
- Move all tasks to…: moves every task in the column to another status after you confirm. It shows only when the column has tasks.
- Archive all: on completed columns only (see below).

Each column also has an "Add task" row where you type a title and add it right there.

## Reorder columns {#reorder}

On a wide screen, turn on Reorder statuses in the Filters menu. Drag handles appear on the column headers. Drag a column to its new place. On a narrow screen, choose Reorder statuses and drag the rows in the Reorder Statuses window, then press Save Order.

## The archive {#archive}

To archive a whole column of finished work, open the menu of a completed column and choose Archive all. TMGR asks you to confirm, then moves all its tasks to the archived status. If the workspace has no archived status, you get "Archive status not found": create one first, with type archived.

To archive one task, move it to an archived status.

Archived tasks disappear from the board and from the current list. Open Archive from the account menu to find them. To bring a task back, change its status to any other one.

## Next {#next}

- [Tasks in detail](/docs/app/tasks)
- [The start-done cycle in the CLI](/docs/cli/the-start-done-cycle)
