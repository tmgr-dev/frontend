---
title: 1. Install and sign in
section: CLI tutorials
order: 30
description: Install tmgr, sign in with a device code, pick a workspace and open the menu.
video: https://tmgr.dev/l/video/en/tmgr-cli-01-install.mp4
---

`tmgr` is the command line for the TMGR task manager. In this lesson you install it, sign in, choose a workspace and look around.

## Install {#install}

One command installs `tmgr` on macOS or Linux:

```sh
curl -fsSL https://tmgr.dev/install.sh | sh
```

The script downloads the signed release for your system, checks it and puts the program into `~/.local/bin` in your home folder. No admin rights or sudo are needed. If that folder is not on your `PATH` yet, the script tells you to add it.

The signature is checked with `minisign` when it is installed. Without `minisign` the script warns you and relies on the checksum from the release manifest, downloaded over HTTPS. A bad signature or a wrong checksum always stops the install.

On Windows, use WSL2 and run the same command inside it.

## Sign in {#sign-in}

```sh
tmgr login
```

You never type a password into the terminal. `tmgr login` prints a sign-in URL and a one-time code, and opens the URL in your browser. Check that the browser shows the same code, approve the request, and the terminal is signed in.

The code expires: the terminal shows how many minutes it is valid. If the browser cannot be opened on this machine (for example over SSH), open the URL on another device. `--no-browser` skips opening the browser:

```sh
tmgr login --no-browser
```

## Check who is signed in {#whoami}

```sh
tmgr whoami
```

It shows who is signed in, on which server, the token's scopes and expiry, and where the token is stored: in the system keychain or credential store, or in a private file when there is none.

## Workspaces {#workspaces}

Tasks live in workspaces. List them:

```sh
tmgr ws list
```

A `*` marks the current workspace. Switch to another one by its code:

```sh
tmgr ws use <workspace>
```

To run a single command in another workspace without switching, add `--ws <workspace>` to it.

This shows the active task and the time you tracked today:

```sh
tmgr status
```

## The interactive menu {#menu}

Don't want to remember commands? Type `tmgr` with no arguments. A menu opens with the commands grouped on the left.

- Arrow keys move through groups and commands. Enter or the right arrow opens a group.
- `/` opens the command search: type part of a name, for example `add`, and press Enter.
- The command opens as a form. Fill in the fields (arrows or Tab move between them) and press Enter to run it. Commands that change something important show a review step first, and the second Enter runs them.
- Esc goes back, `?` shows help, `q` quits.

The menu needs a terminal. It does not open inside an agent session.

## Next {#next}

`tmgr` is installed and connected to your account. In the next lesson you work with tasks right from the terminal: [2. Tasks from the terminal](/docs/cli/tasks-from-the-terminal).

## Video transcript {#transcript}

This is `tmgr` in your terminal: tasks, time tracking and AI agents, without a browser. Lesson one: install and sign in.

### Install {#t-install}

It installs with one command. The script downloads a signed release for your system, macOS or Linux, verifies the signature, and puts the program in your home folder. No sudo needed.

```sh
curl -fsSL https://tmgr.dev/install.sh | sh
```

### Sign in {#t-login}

Now sign in. You never type a password into the terminal: the login command opens your browser with a one-time code. Check the code, approve, and the terminal gets access.

```sh
tmgr login
```

### Who am I {#t-whoami}

This shows who is signed in, which server you're on, and where the token is stored: in the system keychain, or in a private file.

```sh
tmgr whoami
```

### Workspaces {#t-ws}

Tasks live in workspaces. `tmgr ws list` shows them, and the star marks the current one. `tmgr ws use` switches workspaces. And `tmgr status` shows your active task and today's tracked time.

```sh
tmgr ws list
tmgr ws use <workspace>
tmgr status
```

### Menu {#t-menu}

Don't want to memorize commands? Just run the CLI with no arguments. A menu opens: find a command (press `/` and type part of its name, for example `add`), fill in the fields, and the finished command line appears at the bottom, so the menu teaches you the syntax as well.

```sh
tmgr
```

### Next {#t-next}

Done: `tmgr` is installed and connected to your account. In the next lesson, we'll work with tasks right from the terminal: [2. Tasks from the terminal](/docs/cli/tasks-from-the-terminal).
