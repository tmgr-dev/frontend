---
title: 2. Tasks from the terminal
section: CLI tutorials
order: 31
description: Create, list and open tasks, move them on the board and script tmgr with JSON.
video:
---

In this lesson you create tasks, look at them and move them across the board without opening the browser. You need a signed-in `tmgr`: see [1. Install and sign in](/docs/cli/install-and-sign-in).

## Create a task {#add}

```sh
tmgr add "Write release notes" -p <project>
```

Put the title in quotes and pass the project code with `-p` (or `--project`). `tmgr` prints the key of the new task and its link in the web app.

Set the status with `-s` (or `--status`) and a status name. Without it, the task gets the workspace's default status.

To add a description from a file, use `--desc-file`:

```sh
tmgr add "Write release notes" -p <project> --desc-file notes.md
```

## List and open tasks {#list}

```sh
tmgr task list
```

It shows the tasks of the current workspace: key, status and title. Filter the list:

- `--project <project>` (or `-p`): one project.
- `--status-type <type>`: one type of status, `default`, `active`, `completed`, `hidden` or `archived`.
- `--limit <n>`: how many tasks to show (50 by default). `--cursor` gets the next page.

```sh
tmgr task list --project <project> --status-type active
```

Open one task by its key:

```sh
tmgr task show TM-123
```

## The board {#board}

```sh
tmgr board
```

A kanban board right in the terminal.

- Arrow keys, or `h` `j` `k` `l`, move between columns and cards.
- Enter opens and closes the card details.
- Shift+L moves the card to the next status, Shift+H to the previous one.
- `s` starts the timer on the card, Shift+S stops it.
- `c` adds a comment.
- `/` searches, `?` shows help, `q` quits.

`--project <project>` shows one project, `--mine` only your tasks, and `--ascii` draws the board with plain ASCII characters.

```sh
tmgr board --project <project> --mine
```

## JSON for scripts {#json}

Add `--json` to a command to get machine-readable output with a stable schema, `tmgr.v1`. The result is inside `.data`. For example, print the keys of the tasks in Backlog with `jq`:

```sh
tmgr task list --json | jq -r '.data.items[] | select(.status.name=="Backlog") | .ref'
```

The schemas are in the [tmgr source](https://github.com/tmgr-dev/frontend/tree/master/crates/tmgr-cli/schemas/tmgr.v1).

## Create a task from stdin {#stdin}

Pass `-` instead of the title and `add` reads the task from standard input: the first line is the title, the rest is the description.

```sh
echo "Fix broken links on /blog" | tmgr add - -p <project>
```

## More {#more}

Back to the [command line guide](/docs/cli) for everything else `tmgr` can do.
