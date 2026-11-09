---
title: Skills library
section: Web app
order: 55
description: Keep shared skills in one place, track their versions and attach them to your personas.
---

A skill is a short Markdown file with instructions for an agent. The skills library holds skills that you write once and attach to any of your personas. Editing a skill updates every persona it is attached to.

The library is on [Settings → Personas](/settings/personas), in the Skills library section below your personas. To do the same from a terminal, see [Personas and skills in the CLI](/docs/cli/personas-and-skills). The web lesson on personas is [Personas and skills](/docs/web/personas-and-skills).

## Create a skill {#create}

Use the New shared skill form at the bottom of the section. It starts with a template:

```
---
slug:
title:
when:
actions: []
---
```

Fill in the front matter, then write the instructions below it.

- slug: lowercase letters, digits and `-`, up to 60 characters. The name `deleted` is reserved.
- title: up to 120 characters.
- when: when the agent should use the skill. Required.
- actions: the list of actions the skill covers.

A skill can be up to 16384 bytes. A counter under the field shows how much you have used. Press Create skill to save. The library holds up to 100 skills; the "N/100 used" line shows where you are.

To change a skill, press Edit, change the text and press Save changes. You cannot change the slug of an existing skill.

## Import skills {#import}

In the TMGR desktop app, the library has an Import from local agents button. It scans skill folders on your computer. It only reads them and runs nothing.

1. Tick the skills you want. Each one has a status, for example New, Up to date or Changed since last import.
2. Check the Library slug. If it is taken, you can overwrite the skill that has it or pick another slug.
3. Press Import selected.

The dialog lists which files will be uploaded. Only those are sent, as text. The check for secrets is best effort, so read the list yourself. Use Add folder to scan a project folder.

## Versions and history {#versions}

Every change makes a new version, and the list shows it as "v3" and so on. Press History next to a skill to see all versions.

- Each version shows what kind of change it was (Created, Edited, Restored or Converted), who made it and when.
- The Current badge marks the live version.
- Tick Compare with current to see a diff against it.
- Press Restore this version to bring an old one back. This does not rewind the history. It creates a new version with the old content.
- If a persona is pinned to a version, the history says so with "Pinned by".

A skill with files shows a Files button with the count, so you can look at its files and delete one.

## Attach a skill to a persona {#attach}

On the persona's card, open the From library list and press Attach a library skill. A persona can have up to 20 library skills.

By default a persona gets the latest version of a skill. You can pick a specific version in the Version for list instead, and the persona stays on it until you change it. Files are not pinned. Press Detach to remove the skill from the persona.

In the library, each skill shows the personas it is attached to, or Not attached. If a persona has its own skill with the same slug, it wins, and the library marks it "(overridden)".

## Delete and restore {#delete}

Press Delete next to a skill. TMGR asks you to confirm and lists the personas that will lose the skill right away. Those personas are detached, and no copy of the skill stays with them.

A deleted skill goes to the Recently deleted list under the library. It stays there for 30 days. Each row says when it was deleted and the exact date it will be removed for good.

To bring it back:

1. Press Versions on the row.
2. Pick the version you want and read the preview.
3. Press Restore.

Persona attachments are not restored. Attach the skill to your personas again. Restore fails if a skill with the same slug already exists or the library is full. Delete or rename one first.
