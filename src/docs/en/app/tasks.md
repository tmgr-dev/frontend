---
title: Tasks in detail
section: Web app
order: 50
description: What a task holds and how to work with it: status, priority, key, description, timer, checkpoints, links, files, comments and sharing.
---

This page walks through everything on a task. To create your first task, start with [your first task](/docs/web/first-task). Some parts depend on modules; see [Settings → Modules](/settings/modules) and [modules and presets](/docs/web/workspace-modules).

## Open a task {#open}

Click a task in the list or on the board. It opens in a window, or as a full page if you open its link. Changes to an existing task are saved automatically. A new task is saved when you press Create.

The title can be up to 255 characters. If someone else changes the task while you have it open, you see "This task was updated by another user" with Apply Changes and Dismiss.

## Status {#status}

The status picker is at the top left. Pick a status to move the task. How statuses work is in [Statuses and the board](/docs/app/statuses-and-board).

If the Workflow module is off, you get a Done checkbox instead of the picker. In that mode a ⋮ menu (Task actions) offers Archive and Restore.

## Priority {#priority}

Priority has four levels: Urgent, High, Medium and Low. New tasks start at Medium. The list and the board show a badge for every level except Medium, and you can filter and sort by priority on the board.

## Project and key {#key}

The Category row puts the task in a project. The task then gets a key: the project code and a number, like `WEBSITE-12`. The key is shown at the top, next to the status.

By default the number is assigned for you (the Key field says "auto"). To set your own, type a whole number of 1 or greater. If that number is taken, you get a conflict message; pick another or clear the field.

The plus button next to the project starts another new task in the same project. A task without a project has no code and shows as `TMGR-T` plus its id.

## Description {#description}

The description is formatted text. In the editor, press `/` to see the available blocks. Which editor you get is a personal choice in Settings; a task written in the older Editor.js format always opens in it. When the AI assistant module is on, an AI rewrite panel sits above the description.

## Timer {#timer}

The Timer module adds a block with the label "Time tracked". Press the round button to start the timer and press it again to stop. While it runs, the block shows "Running". If the task has an estimate and you go past it, you see "Over time".

If you start the timer on a task in the Backlog (default) status, TMGR asks "Task in Backlog" and offers to move it to the first status of type active, usually "In progress". Confirm to move it, or decline to keep the status. The question only appears when Workflow is on and the workspace has an active status.

Pomodoro, if enabled, is a separate tomato button in the task header.

## Checkpoints {#checkpoints}

Checkpoints are a log of what you worked on, tied to the tracked time. Press Add entry to start a new line: it begins at the current tracked time and closes the previous line at the same moment. Write what you did in the line. Click the time range to edit it as `HH:MM:SS`; the start must be before the end. The header shows the total logged.

## Linked tasks {#links}

The Relations module lets you link tasks. In the Linked tasks row, press Link, search for a task by title (type at least two characters), pick a type and press Link again.

The types are: blocks, is blocked by, relates to, duplicates, is duplicated by, depends on, is dependency of. Click a linked task to open it. Hover a link and press the cross (Remove relation) to delete it.

## Attachments {#attachments}

With the Attachments & files module on, an Attachments section lets you add files: click it, or drag files onto the task. Files added to a new task are uploaded when you create it. Files that are too large, empty or of a type that cannot be attached are rejected with a message.

## Comments {#comments}

Type in the box at the bottom (or in the right column on a full page) and press Enter to send; Shift+Enter adds a new line. You can paste or drop files into a comment. Under Activity, filter comments by All, People, Personas and Plugins. You can add a reaction to a comment or delete your own.

### AI reply {#ai-reply}

With the AI assistant module on, a sparkle button in the comment box toggles "AI reply: on" or "AI reply: off". While it is on, sending a comment asks the AI; its answer appears as a comment from "AI Assistant". It uses AI credits. The setting is saved for your account.

## Share a task {#share}

The Share button in the task footer, or the Share submenu in a task's menu in the list and on the board, offers Copy link, Copy app link and Copy as Markdown. Anyone you send the link to still needs access to the workspace.

## Delete {#delete}

The trash button in the task footer deletes the task after you confirm. To keep a task out of the way without deleting it, move it to an archived status instead.

## Next {#next}

- [Statuses and the board](/docs/app/statuses-and-board)
- [Tasks from the terminal](/docs/cli/tasks-from-the-terminal)
