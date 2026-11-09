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

Stopping the runner does not stop your agents. Sessions keep running, and you can reach them again once the runner is back.

```sh
tmgr runner stop
```

To stop a single session, use `halt`. See [Agent sessions](/docs/cli/sessions#stop).

After you update `tmgr`, restart the runner so it uses the new version. Sessions keep running through the restart.

```sh
tmgr runner restart
```

Good to know:

- If the running runner is newer than your `tmgr`, restart refuses. Run the newer `tmgr`, or add `--force` to go back to the older one.
- If the autopilot loop was running, the restart stops it. Start it again afterwards.
- If a login service is installed, restart uses it. When the service file is outdated, restart tells you to reinstall it (next section).

## Keep the runner alive after a reboot {#service}

To start the runner at login, first look at what would be installed. This changes nothing:

```sh
tmgr runner install-service
```

The preview shows the service file and the commands that would run. It uses launchd on macOS and a systemd user unit on Linux. When it looks right, apply it. You need a terminal, and you confirm by typing a phrase:

```sh
tmgr runner install-service --install
```

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

Copies with a running agent, or with an open pull request, are kept. The command exits with an error code if some copy could not be removed.

## Related commands {#more}

Cloud workspaces and project clones are set up in [The agent queue](/docs/cli/the-agent-queue). To check them later:

```sh
tmgr runner allowed
tmgr runner list
```

`allowed` lists the cloud workspaces that may start agents here, with expiry. `list` shows the repositories bound to projects. To undo, use `runner disallow` and `runner unbind`.

To bring the branch of a session clone into your repository, run this from inside the repository with the session name, or pass the path of the clone. Nothing runs in the clone.

```sh
tmgr runner import <session>
```
