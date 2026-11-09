---
title: Troubleshooting
section: CLI guides
order: 63
description: Fix the common `tmgr` problems: updates, signature errors, sign-in, stored credentials and missing token scopes.
---

Each section below is a problem and its fix. For install and first sign-in, see [Install and sign in](/docs/cli/install-and-sign-in). For the full command list, see the [command line guide](/docs/cli).

## Updating tmgr {#update}

Check whether a newer release exists without installing it:

```sh
tmgr self-update --check
```

Install it:

```sh
tmgr self-update
```

`tmgr` keeps the previous binary next to the new one as `tmgr.old`. `--channel beta` takes the beta channel instead of stable.

### "no stable release found" {#no-release}

Nothing is published on the stable channel yet. The hint suggests `--channel beta`.

### "tmgr is managed by ..." {#managed}

You installed `tmgr` with a package manager, and a self-update would fight it. Update it with the command the message prints.

### "no permission to write the install directory" {#permission}

`tmgr` never uses `sudo`. Reinstall to a folder you own with the install script from [Install and sign in](/docs/cli/install-and-sign-in).

### "refusing to downgrade" {#downgrade}

The release is older than one this machine already trusted. If you really want it, run `tmgr self-update --allow-downgrade` in a terminal. It asks you to confirm.

## A signature check fails {#signature}

`tmgr self-update` installs only signed releases. It checks the signature of the release manifest, then the checksum of the downloaded archive, and only then replaces the program. If any check fails, nothing is installed and your current `tmgr` keeps working. Typical messages:

- `the manifest signature does not verify; refusing to update` or `the release is not signed`: do not work around it. Try again later; if it keeps failing, reinstall with the install script.
- `the archive does not match the signed checksum; nothing was installed`: the download was damaged or changed. Run the update again.
- `the manifest has expired; refusing a stale release`: the release is too old to trust. Wait for a new one or reinstall.
- `this build has no update key; install a signed release`: this copy was not built from an official release. Reinstall with the install script.

## Where your sign-in is stored {#credentials}

`tmgr` saves your token in the operating system's credential store (the keychain). It never prints the token.

If there is no credential store (for example on a server without a desktop), `tmgr login` saves the token in a private file with mode 0600 and tells you where.

### "the OS credential store refused access" {#keychain-locked}

The keychain is locked or access was denied. Unlock it and retry. If you cannot use it, switch to a private file for the commands that follow:

```sh
TMGR_CREDENTIAL_STORE=file tmgr login
```

Set `TMGR_CREDENTIAL_STORE=file` in your shell profile so every later command uses the file as well.

### A credentials file error {#file-mode}

If the file is readable by others, `tmgr` refuses it and prints a `chmod 600` hint. Run that command. If the file is not valid, remove it and sign in again.

## Sign in again {#re-login}

Messages like `not logged in or the session expired`, `session expired or revoked` and `stored token is malformed` mean the token no longer works. Sign in again:

```sh
tmgr login
```

To start from scratch, sign out first. It deletes the local token:

```sh
tmgr logout
```

Check who is signed in with `tmgr whoami`. If the browser cannot open, add `--no-browser`. If the login code expired, run `tmgr login` again.

## "this login does not allow that" {#scopes}

The message continues with `(needs scope ...)`. Your sign-in was made without that permission. The hint prints a ready `tmgr login --scopes ...` command with the missing scope added. Run it and approve the request in the browser.

The same applies to errors that mention `agent-approvals:write`, which autopilot needs to sign tasks. See [Autopilot](/docs/cli/autopilot).

## Copying in tmgr attach over SSH does nothing {#clipboard}

Your terminal needs to allow programs to write to the clipboard (OSC 52). Run this on your own computer, not on the server:

```sh
tmgr setup terminal
```

It shows each change and asks `Apply this change? [y/N]` before it writes anything. Restart the terminal or reload its config afterwards if the output says so. To only see what would change, use `--check`.

## Server errors {#server}

- `server unavailable (HTTP 502)` and similar: the server or your connection is down. Retry in a moment.
- `rate limited by the server`: wait a minute.
