---
title: AI assistant and memory
section: Web app
order: 31
description: Ask the AI about your workspace, let it propose changes that you confirm, keep shared facts in memory and use AI on tasks and comments.
---

The AI assistant answers questions about your workspace and helps with tasks. It needs the AI assistant module, which is off by default. The owner turns it on in [Settings → Modules](/settings/modules), in the AI agents group. See [Modules reference](/docs/app/modules).

The cloud AI features work in cloud workspaces only. In a local workspace of the desktop app, the assistant talks to your own OpenAI-compatible server instead, see [Local workspaces](#local-assistant). Ask persona, described at the end of this page, also works there.

## Ask AI {#ask-ai}

Open the assistant with the **Ask AI** button:

- In the web app, it is the sparkle button at the top right of the page.
- In the desktop app, it is the sparkle button in the status bar at the bottom of the window.

The panel opens on the side. Type a question in "Ask about this workspace…" and press Enter or **Send**. For example: what is overdue, how much time went into a category this week, or what a task is about.

The assistant reads the workspace for you: tasks and their comments, files and links, statuses, members, tracked time, deadlines, pages, git activity of tasks, your notifications and your routines. While it works, it shows **Thinking…** and the steps it takes.

Under the input there are ready-made requests:

- **Morning summary**, **Day results**, **Week results**, **Blockers and stale tasks** and **Estimate by similar tasks** send the request right away.
- **Break an epic into subtasks** and **Review a meeting** put a prompt into the input. Paste your text after it and send.

You can also attach files with **Attach files**, or drop or paste them into the panel.

Each message has these actions:

- **Copy** copies the text.
- **Reply** quotes the message in your next one. To quote only a part, select the text and press **Quote**.
- **Pin** opens [pinned facts](#pinned-facts) with the message text in the input, cut to 500 characters. Press **Add** to pin it.

### Chats {#chats}

Each conversation is a chat. **New chat** starts an empty one. **Chat history** opens **Chats**, the list of your chats in this workspace:

- **Search chats** finds a chat.
- **Rename chat**, **Archive chat** and **Delete chat** are next to each chat. Deleting asks "Delete?" first.
- **Show archived** shows archived chats. Use **Unarchive chat** to bring one back.

### Pinned facts {#pinned-facts}

Pinned facts are short notes that the assistant gets with every message in this chat. They stay when the history is compacted. Open them with **Pinned facts** in the panel header.

- Type in "Add a fact…" and press **Add**, or press **Pin** on a message.
- **Edit** changes a fact, **Remove** deletes it.
- A chat can hold up to 20 pinned facts, up to 500 characters each and 4000 characters in total.

Pinned facts belong to one chat. Facts that the whole workspace should share go to [workspace memory](#memory).

### Compact history {#compact}

A long chat fills the assistant's context. **Compact history** replaces older messages with a summary, so the chat can go on. It works between answers. TMGR also compacts a chat on its own when it gets too long. The chat then shows "History compacted (automatic)".

## Assistant in local workspaces {#local-assistant}

In a local workspace of the desktop app, **Ask AI** (the sparkle button in the status bar) opens an assistant that uses your own OpenAI-compatible server, such as Ollama, LM Studio, vLLM or llama.cpp. Set the server up in [Settings → Personas](/settings/personas), under **Local LLM**, and turn on the AI assistant module for the workspace. Without a server address the panel offers **Open AI settings**.

The assistant can read and search tasks, comments, statuses and pages. Every change, such as creating or editing a task, a comment, a page, a status or a timer, waits for your **Confirm** or **Reject**. It does one request after another and stops after 8 tool calls in one answer. **Stop** cancels the answer.

The chat stays on this computer and is not saved after you close the app. Workspace data goes only to the server you set. If that server is not on this computer or your local network, the panel shows a warning, because the data you send leaves this device.

## Write mode {#write-mode}

By default the assistant only reads. With write mode, it can also propose changes:

- create a task, change a task, add a comment or a checkpoint, link two tasks;
- start or stop a timer;
- create and change your routines, check them off, move them to another day or turn one into a task;
- save a fact to memory, create a page or add text to a page.

Only the workspace owner can turn write mode on. Open [Settings → Modules](/settings/modules), find the **AI assistant** section and set **Write mode** to **Write with confirmation**. **Read only (default)** turns it off again.

### How confirmation keeps you safe {#safety}

The assistant never changes anything on its own. Every change goes through you:

1. The assistant shows a **Confirm changes** card: "TMGR AI wants to make these changes". It lists each change. Most of them carry a **High risk** mark. **Show all** opens a long change in full.
2. Press **Confirm** to apply the changes or **Cancel** to drop them. Confirm applies all changes on the card together: if one of them fails, none is applied, and the card says that nothing was changed. After a change is applied, **Open** takes you to the task or page.
3. Before applying, TMGR checks again that you are still a member, that the AI assistant module is on and that write mode is still on. If not, nothing changes.
4. A card expires after 15 minutes. An expired card shows "Expired — nothing was changed".
5. If you send a new message while a card is open, the card is skipped. The panel warns you: "Sending a new message will cancel the pending changes."

Write mode works only in the Ask AI panel. AI replies to comments never change anything.

One thing needs no card: when your own message asks the assistant to remember something, it saves that to your personal memory right away. This works in read-only mode too. Your personal memory is private and is not shown on the Memory page.

## Workspace memory {#memory}

Workspace memory holds facts the AI assistant remembers about the workspace. Every member can read them. Open **Memory** in the sidebar, in the tools group, or go to `/<workspace code>/memory`.

Memory is kept in pages, so it needs the Pages module. If Pages is off, the page says so and offers **Open modules**. Memory is available only in cloud workspaces.

Facts are grouped by topic:

- People and roles: who is who and who owns what.
- Project agreements: agreements and conventions of the team.
- Decisions and reasons: what was decided and why.
- Known limitations: constraints and things that do not work.

Each fact shows the date it was added and where it came from. A fact marked **stale** has passed the date by which it should be checked again.

- To add a fact, pick a **Topic**, type in "Add a fact" and press **Add**.
- To remove one, press **Delete**. The message that follows has **Undo**.

In write mode the assistant can also propose a fact for memory. It goes through a confirmation card like any other change.

## AI on a task {#on-a-task}

These tools need the AI assistant module. They are not available on tasks of a local workspace.

### Rewrite a description {#rewrite}

Next to the **Description** label of a task, press **Rewrite with AI**. The button is off when the description is empty ("Write a description first") or was written in Editor.js.

1. A dialog opens and the AI writes a first revision. "Review the proposed text. Nothing changes until you accept it."
2. Under **Revisions**, click **Original** or **Revision 1**, **Revision 2** and so on to compare them.
3. To ask for changes, type an instruction in "Refine: e.g. make it shorter, add acceptance criteria" and press **Refine**. The AI rewrites the revision you are looking at.
4. Press **Accept** to put the selected text into the description, or **Cancel** to close without changes.

After you accept, **Undo AI rewrite** appears. It brings the old description back, as long as you have not edited the description since. If someone changed the description while the dialog was open, Accept refuses and asks you to rewrite again.

### AI replies in comments {#ai-reply}

The comment box of a task has a sparkle toggle: **AI reply: off** or **AI reply: on**. With it on, the send button becomes **Ask AI (Enter)**. Your comment is posted, and the AI answers it in the comments. TMGR remembers the toggle for your account.

Everyone on the task sees the answer, so an AI reply does not use your personal memory or your routines. It only reads, and it never changes the task.

### Voice input {#voice}

The Ask AI input and the task comment box have a microphone button, **Dictate**.

1. Press it and speak. The browser or the desktop app asks for microphone access the first time.
2. Press it again (**Stop and transcribe**). Your speech turns into text at the cursor.
3. Edit the text if needed and send it yourself.

A recording stops at 2 minutes. If the button says "Voice input is not supported here", your browser cannot record audio.

### Ask persona {#ask-persona}

Ask persona is in the TMGR desktop app only, on tasks of a local workspace. It needs the Personas module and a local AI model server on your computer.

In the task window, above the comment box, pick a persona and press **Ask persona**. The persona reads the task and posts its answer as a comment. The persona needs permission to write comments and to report agent work. If the answer is cut off or too long, choose **Publish anyway** or **Discard**. See [Local workspaces](/docs/desktop/local-workspaces#local-agent).

## Modules each feature needs {#modules}

| Feature | Module | Where |
| --- | --- | --- |
| Ask AI panel, pinned facts, write mode | AI assistant | Cloud workspaces |
| AI replies in comments, Rewrite with AI, voice input | AI assistant | Cloud workspaces |
| Workspace memory | Pages | Cloud workspaces |
| Ask persona | Personas | Desktop app, local workspaces |
