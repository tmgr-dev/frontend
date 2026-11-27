# TM-1129 · Optional closing note on the board

A person closing a task can tell the team why it was closed. The note appears in the task's existing comment thread. [TM-1129](https://tmgr.dev/tmgrdev/tasks/11462)

## Interaction

Add a personal preference, **Ask for an outcome when closing a card**, off by default. When it is on and task comments are available, offer a note after the board confirms a single card moved from an open status to one of type `completed` or `archived`. Use the status type even if the workspace has renamed the column.

Cover drag and drop, **Move to status**, and the card's **Archive** action. Archive keeps its existing confirmation. A cancelled or failed move, reordering a column, a transition between two closed statuses, a bulk action, and another user's realtime update produce no prompt. Reopening and subsequently closing a task starts a new opportunity to leave a note.

The dialog names the card and offers **Done**, **Won’t do**, **Duplicate**, and **Moved elsewhere** as optional shortcuts. Selecting one fills an editable text field. The person can also write their own note. **Post comment** submits the trimmed text once. **Skip** or Escape dismisses the dialog. Both actions keep the saved status. Provide English and Russian copy and return focus to the board if the card has disappeared.

## Saving

Keep the prompt on the board so archiving the card cannot unmount it. Bind each prompt to the initiating account, workspace, and task. If two closes succeed quickly, present each prompt in turn. Discard pending prompts on workspace or account changes. Use the existing comment action and the local task database for local workspaces.

Disable submission while a comment is in flight. On a rejected post, keep the text available and offer another attempt. If delivery is uncertain after a network interruption, tell the person to check the thread before resending. The move has already succeeded. The first iteration uses ordinary comments and a fixed set of localized shortcuts.

## Acceptance checks

- With the preference off or comments disabled, closing a card completes in a single flow.
- With the preference on, exercise drag, menu move, and archive. Each confirmed close offers one note. Skip produces no comment, and submitting a note writes to the intended task.
- Exercise a failed move, failed comment, double click, and a remote update. Confirm that retries only post the comment and that the board never asks about someone else's move.
- On a narrow screen and a local workspace, post a custom note, then open the archived task and read it. Local task text stays on the device.
