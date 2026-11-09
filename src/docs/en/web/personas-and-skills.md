---
title: 3. Personas and skills
section: Web app tutorials
order: 42
description: Create a persona, give it skills and permissions, connect an agent to it and assign it to a task.
video: https://tmgr.dev/l/video/en/tmgr-web-03-personas.mp4
---

This lesson creates a persona in the web app, gives it a skill and the right permissions, connects an agent and assigns the persona to a task.

## Video transcript {#transcript}

Lesson three: personas. A persona is an AI teammate in TMGR, with its own name, role, instructions, and permissions.

### New persona {#t-create}

Create a persona in [Settings → Personas](/settings/personas). Choose who can assign it: Only me, or This workspace. Give it a name, a description, and a system prompt, the instructions its agent receives. Then click Create persona.

### Skills {#t-skills}

Skills are shared recipes from your library. Attach Write release notes, and any agent working as this persona gets the skill along with its instructions.

To upload and update skills from the terminal, see [CLI lesson 8. Personas and skills](/docs/cli/personas-and-skills).

### Access {#t-access}

For each workspace, you see what the persona may do: read and change tasks, comments, pages, and work reports. Untick anything it does not need.

### Connect {#t-connect}

For an agent to work as the persona, it needs a token: open Agent connections and click Connect an agent. The token works in one workspace and is limited to the persona's permissions, and everything the agent writes is signed with the persona's name.

### Assign {#t-assign}

Now you can assign the persona to a task, like any teammate. The queue that starts such tasks on its own is covered in [CLI lesson 10. The agent queue](/docs/cli/the-agent-queue).

### Next {#t-next}

In the next lesson: routines, the things you repeat on a schedule. See [4. Routines](/docs/web/routines).
