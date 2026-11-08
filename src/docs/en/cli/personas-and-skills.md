---
title: 8. Personas and skills
section: CLI tutorials
order: 37
description: Upload a skill to the shared library, give it to a persona and manage its versions.
video: https://tmgr.dev/l/video/en/tmgr-cli-08-skills.mp4
---

This lesson covers personas and skills: how to put a skill into the shared library and give it to a persona.

## Video transcript {#transcript}

Lesson eight: personas and skills. A persona is an AI teammate in TMGR, with its own role and instructions, and skills are shared recipes you can give to any persona.

### New skill {#t-put}

A skill is a plain Markdown file. The front matter holds a short slug, a title, when to use it, and the actions it covers; the instructions go below.

```sh
cat skills/release-notes.md
tmgr skills put release-notes --file skills/release-notes.md
```

`tmgr skills put` uploads the file to your shared library, and every change becomes a new version.

### Attach to persona {#t-attach}

`tmgr skills attach` gives a skill to a persona, using the persona's ID from the web app.

```sh
tmgr skills attach <persona-id> release-notes
```

Now the list shows which personas have it.

```sh
tmgr skills list
```

An agent working as that persona gets its instructions and all of its skills.

### Show {#t-show}

This prints the whole skill, with its version and personas.

```sh
tmgr skills show release-notes
```

Update the file and run `tmgr skills put` again, and every persona gets the new version. `tmgr skills detach` and `tmgr skills delete` remove a skill from a persona, or for good.

### Next {#t-next}

In the next lesson: how agents report their work on tasks. See [9. Agent work reports](/docs/cli/agent-work-reports).
