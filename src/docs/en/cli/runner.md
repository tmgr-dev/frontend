---
title: Runner maintenance
section: CLI guides
order: 60
description: Start, restart and stop the background runner, keep it alive after a reboot, and clean up what finished agent tasks leave behind.
---

The runner is the background process that owns your agent sessions. `tmgr run` starts it when needed, so most days you never touch it. This guide covers the cases where you do. For the first run, see [AI agents under supervision](/docs/cli/ai-agents-under-supervision) and [The agent queue](/docs/cli/the-agent-queue). The overview is in the [command line guide](/docs/cli#runner).

All runner commands are experimental.

## Check and start the runner {#status}

To see whether the runner is up, with its version, uptime and the number of sessions:

```sh
tmgr runner status
```

If the runner is older than your `tmgr`, the output tells you to restart it. If it is not running, the command says so.

To start it by hand:

```sh
tmgr runner start
```

## Stop and restart {#stop}

Stopping the runner does not stop your agents. Sessions keep running, and you can reach them again once the runner is back. If you installed the login service on Linux, its service file must be up to date; see the restart notes below.

```sh
tmgr runner stop
```

To stop a single session, use `halt`. See [Agent sessions](/docs/cli/sessions#stop).

After you update `tmgr`, restart the runner so it uses the new version. Sessions keep running through the restart. If some do not come back, the output says how many.

```sh
tmgr runner restart
```

Good to know:

- If the running runner is newer than your `tmgr`, restart refuses. Run the newer `tmgr`, or add `--force` to go back to the older one. Restart also refuses a runner that does not report its version, unless you add `--force`.
- On Linux, an older service file lets systemd end every agent session when the runner stops. If sessions are running, restart refuses and tells you to reinstall the service first (next section). `--force` restarts anyway, and systemd can then end those sessions. With no sessions running, restart only warns.
- If the autopilot loop was running, the restart stops it. Start it again afterwards.
- If a login service is installed, restart uses it.

## Keep the runner alive after a reboot {#service}

To start the runner at login, first look at what would be installed. This changes nothing:

```sh
tmgr runner install-service
```

The preview shows the service file and the commands that would run. It uses launchd on macOS and a systemd user unit on Linux. When it looks right, apply it. You need a terminal, and you confirm by typing a phrase:

```sh
tmgr runner install-service --install
```

The same command replaces an outdated service file.

On Linux the runner starts at login, not at boot. Starting at boot without a login needs `loginctl enable-linger`, which `tmgr` does not run for you.

The preview warns you if the `tmgr` binary sits in a temporary folder or is not owned by you. Fix that before you install.

To remove the registration:

```sh
tmgr runner uninstall-service
```

A file that `tmgr` did not create is left untouched.

## Trust a folder {#trust}

Agents ask whether they may work in a folder. To skip the question for a folder you know, trust it. This covers agents started in exactly that folder and in the task clones `tmgr run` makes of it. The command asks you to confirm.

```sh
tmgr runner trust ~/acme-site
```

Without a path, the current folder is used. To take the trust back, and to see what is trusted now:

```sh
tmgr runner untrust ~/acme-site
tmgr runner trusted
```

The list shows each folder, how it was added and when.

## Clean up finished task copies {#gc}

Each task started with `tmgr run <task>` or by the queue works in its own copy of the project. When the pull request is merged or closed and nothing runs in the copy, it can go. To see what would be removed, and how much space you would get back, without deleting anything:

```sh
tmgr runner gc --dry-run
```

To remove the copies:

```sh
tmgr runner gc
```

The runner must be running, so that `gc` can tell which copies are in use. `gc` does not start it for you.

A copy is kept, with the reason in the output, when:

- an agent or another process works in it;
- it has uncommitted changes, untracked files, or ignored files that are not build output;
- it has a stash, or a merge, rebase or similar git operation is in progress;
- HEAD is not on the task branch;
- its pull request is still open, there is no pull request, or its state cannot be confirmed, for example because of commits that were never pushed.

The command exits with an error code if some copy could not be removed.

## Rerun tests when files change {#watch}

`tmgr attach` can open a pane that reruns a command, such as your tests, each time files in the folder change. You can run the same watcher in any terminal:

```sh
tmgr runner watch --dir . --cmd "cargo test"
```

It runs the command once in that folder, then checks the folder about every half second and runs the command again once the files stop changing. Folders such as `.git`, `node_modules`, `target` and `dist` are skipped. Press Ctrl-C to stop.

This command and `runner git-diff` below do not appear in the built-in help.

## Show the changes of a folder {#git-diff}

`tmgr attach` can also show the uncommitted changes of a session's folder in a pane. In a terminal:

```sh
tmgr runner git-diff --dir .
```

It shows `git diff HEAD` for the folder in a pager: the one you pass with `--pager`, otherwise `delta` if it is installed, otherwise `less -R`. It does not run programs that the folder's git settings name. If those settings are unusual, it refuses with `this folder's git settings are not plain, so the diff is not shown`.

## Sign in agents of sandboxed sessions {#agent-login}

A sandboxed session runs with its own home folder, so Claude Code there does not find your usual login. To sign it in, create a long-lived token with `claude setup-token` in a normal terminal, then store it for the `claude-code` profile:

```sh
tmgr agents login claude-code
```

Paste the token at the hidden prompt, or pipe it on one line of stdin. `tmgr` writes it to a private file and never prints it. The next session is signed in. To replace a stored token, add `--replace`. To delete it:

```sh
tmgr agents logout claude-code
```

Codex can refresh its login inside a session. Your own login file is not changed by that. Instead, `tmgr agents` and `tmgr run` print a line that a refreshed login is waiting. To review and apply it:

```sh
tmgr agents sync codex
```

It shows a summary without token values and applies the login only after you type `apply`. It needs a terminal and does not run inside an agent session. `--discard` drops the waiting logins. `--auto on` lets the runner apply them by itself after each session; it explains the risk and asks you to type `I AGREE`. `--auto off` turns that off.

## Follow sessions as they change {#events}

To print a line each time a session changes its state, until you press Ctrl-C:

```sh
tmgr agents watch
```

With `--json`, each change is one JSON object per line. To see what the runner recorded of one session: how long it worked, waited and was blocked, the agent, the branch and how it exited:

```sh
tmgr agents log <session>
```

## Related commands {#more}

Cloud workspaces and project clones are set up in [The agent queue](/docs/cli/the-agent-queue). To check them later:

```sh
tmgr runner allowed
tmgr runner list
```

`allowed` lists the cloud workspaces that may start agents here, with expiry. `list` shows the repositories bound to projects.

To undo `runner allow`, stop letting a workspace start agents here. To undo `runner bind`, forget the clone of a project; this asks you to confirm:

```sh
tmgr runner disallow <workspace>
tmgr runner unbind <workspace>:<project>
```

To bring the branch of a session clone into your repository, run this from inside the repository with the session name, or pass the path of the clone. Nothing runs in the clone.

```sh
tmgr runner import <session>
```
