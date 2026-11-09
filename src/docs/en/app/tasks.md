---
title: Tasks in detail
section: Web app
order: 21
description: What a task holds and how to work with it: status, priority, assignees, category and key, description, timer, Pomodoro, checkpoints, links, files, comments and sharing.
---

This page walks through everything on a task. To create your first task, start with [your first task](/docs/web/first-task). Some parts depend on modules; see [Settings → Modules](/settings/modules) and [modules and presets](/docs/web/workspace-modules).

## Open a task {#open}

Click a task in the list or on the board. It opens in a window, or as a full page if you open its link. Changes to an existing task are saved automatically. A new task is saved when you press **Create**.

The title can be up to 255 characters. If someone else changes the task while you have it open, you see "This task was updated by another user" with **Apply Changes** and **Dismiss**.

## Status {#status}

The status picker is at the top left. Pick a status to move the task. How statuses work is in [Statuses and the board](/docs/app/statuses-and-board).

If the Workflow module is off, you get a **Done** checkbox instead of the picker. In that mode the ⋮ menu (**Task actions**) shows **Archive**, or **Restore** on an archived task.

## Priority {#priority}

Priority has four levels: **Urgent**, **High**, **Medium** and **Low**. New tasks start at **Medium**. The list and the board show a badge for every level except **Medium**, and you can filter and sort by priority on the list and on the board.

## Assignees {#assignees}

With the Assignees module on, the **Assignee** row shows who works on the task. Click it, search for a member and pick one or more people. When the Personas module is on, the same list also has personas; see [personas and skills](/docs/web/personas-and-skills).

If you are not assigned yet, the button at the end of the row (**Assign to me**) adds you.

You can also change assignees without opening the task. On the list and on the board, each task has an assignee button: **Assign someone**, or **Change assignee** when someone is already assigned. Click it, search, and click a name to add or remove that person. On a board card the button appears when you point at the card, or stays visible once the task has assignees.

## Category and key {#key}

The **Category** row puts the task in a category. The task then gets a key: the category code and a number, like `ABC-12`. The key is shown at the top, next to the status. Codes and other category settings are on the [Categories](/docs/app/categories) page.

By default the number is assigned for you (the **Key** field says "auto"). To set your own, type a whole number of 1 or greater. If that number is taken, you get a conflict message; pick another or clear the field.

If the category has a task title pattern and the title of a new task is still empty, picking the category fills the title from the pattern. See [the title pattern](/docs/app/categories#settings).

The plus button next to the category starts another new task in the same category. A task without a category has no code and shows as `TMGR-T` plus its id.

## Description {#description}

The description is formatted text. In the editor, press `/` to see the available blocks. Which editor you get is a personal choice in Settings; a task written in the older Editor.js format always opens in it. When the AI assistant module is on, an AI rewrite panel sits above the description.

## Timer {#timer}

The Timer module adds a block with the label **Time tracked**. Press the round button to start the timer and press it again to stop. While it runs, the block shows **Running**.

If the task has an estimate and the tracked time goes past it, the time turns red. While the timer runs, the block then shows **Over time** instead of **Running**. A new task in a category gets the category's **Default estimate**; see [category settings](/docs/app/categories#settings).

If you start the timer on a task in the Backlog (default) status, TMGR asks "Task in Backlog" and offers to move it to the first status of type active, usually "In progress". Confirm to move it, or decline to keep the status. The question only appears when Workflow is on and the workspace has an active status.

### Change the tracked time {#edit-time}

Next to the time, press the info button (**Task time info**). The **Task time information** window opens; with an estimate it also shows the approximate end time and the time left. To change the tracked time:

1. Press **Edit time mode**.
2. If the timer is running, stop it first. The window says "You can edit time, but first you need to stop the timer" and shows **Stop the timer**.
3. Type the hours, minutes and seconds.
4. Press **Save**.

## Pomodoro {#pomodoro}

With the Pomodoro module on, a saved task has a tomato button in its header (**Enable Pomodoro**). Press it to add a Pomodoro block under the timer. To remove the block, press the button again (**Disable Pomodoro**) or the cross in the block (**Disable Pomodoro for this task**).

- Press the tomato in the circle to start. If the task timer is not running, it starts too. Press the button again to pause.
- Stopping the task timer also pauses Pomodoro.
- The label shows the phase: **FOCUS**, **SHORT BREAK** or **LONG BREAK**. A short break follows each focus interval, and a long break follows every few of them.
- **Skip phase** moves to the next phase. **Reset** starts the current phase over.
- Press **DISTRACTED** ("I got distracted") each time something pulls you away. The button then shows how many times.
- The dots and the line under the time show your completed pomodoros and the interval lengths.

### Pomodoro settings {#pomodoro-settings}

The gear in the block (**Pomodoro settings**) opens the settings. They apply to all your tasks.

- **Focus duration**, **Short break** and **Long break**: from 1 to 180 minutes. The defaults are 25, 5 and 15.
- **Long break every**: after how many focus intervals the long break comes, from 2 to 12. The default is 4.
- **Auto-start next interval**: the next phase starts on its own, without pressing the tomato.
- **End-of-interval sound**: pick **Bell**, **Chime**, **Wood** or **Digital** and set the volume. You hear the sound when you pick it. Under **Notifications**, **Silent** turns the sound off.

Press **Save**.

## Checkpoints {#checkpoints}

Checkpoints are a log of what you worked on, tied to the tracked time. Press **Add entry** to start a new line: it begins at the current tracked time and closes the previous line at the same moment. Write what you did in the line. Click the time range to edit it as `HH:MM:SS`; the start must be before the end. The header shows the total logged.

## Linked tasks {#links}

The Relations module lets you link tasks. In the **Linked tasks** row, press **Link**, search for a task by title (type at least two characters), pick a type and press **Link** again.

The types are: blocks, is blocked by, relates to, duplicates, is duplicated by, depends on, is dependency of. Click a linked task to open it. To delete a link, hover it and press the cross (**Remove relation**), then confirm in the **Remove link** window.

## Attachments {#attachments}

With the Attachments & files module on, an **Attachments** section lets you add files: click it, or drag files onto the task. Files added to a new task are uploaded when you create it. Files that are too large, empty or of a type that cannot be attached are rejected with a message.

## Comments {#comments}

Type in the box at the bottom (or in the right column on a full page) and press Enter to send; Shift+Enter adds a new line. You can paste or drop files into a comment. Under **Activity**, filter comments by **All**, **People**, **Personas** and **Plugins**. You can add a reaction to a comment or delete your own.

### AI reply {#ai-reply}

With the AI assistant module on, a sparkle button in the comment box toggles **AI reply: on** or **AI reply: off**. While it is on, sending a comment asks the AI; its answer appears as a comment from **AI Assistant**. It uses AI credits. The setting is saved for your account.

## Share a task {#share}

The **Share** button in the task footer, or the **Share** submenu in a task's menu in the list and on the board, offers **Copy link**, **Copy app link** and **Copy as Markdown**. Anyone you send the link to still needs access to the workspace.

## Delete {#delete}

The trash button in the task footer (**Delete**) deletes the task after you confirm. A deleted task leaves the list and the board, and the web app has no trash to bring it back from. To keep a task out of the way without deleting it, archive it instead: move it to an archived status, or with Workflow off choose **Archive** in the ⋮ menu.

## Next {#next}

- [The task list](/docs/app/task-list)
- [Statuses and the board](/docs/app/statuses-and-board)
- [Tasks from the terminal](/docs/cli/tasks-from-the-terminal)
