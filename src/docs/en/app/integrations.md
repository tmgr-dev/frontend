---
title: Integrations
section: Web app
order: 35
description: Use the Telegram bot, link GitHub repositories, run Cursor agents from tasks, and sign in with GitHub, Google, Apple or Telegram.
---

TMGR works with Telegram, GitHub and Cursor. Each of them is a module, and all three are off by default: Telegram bot, GitHub and Cursor agents. The owner turns them on in [Settings → Modules](/settings/modules), in the Integrations and AI agents groups. See [Modules reference](/docs/app/modules).

## Telegram bot {#telegram}

With the bot you create routines, tasks and page notes from Telegram, and get TMGR notifications there. The bot answers in English.

### Link your account {#telegram-link}

1. Open [Settings → Profile](/settings?tab=profile). The **Telegram** section is there while the Telegram bot module is on.
2. Press **Connect Telegram**, then **Open Telegram**.
3. Telegram opens the bot. Press Start. The bot replies "Telegram account linked to tmgr."

The link works once and for 30 minutes. If it expired, press **Connect Telegram** again. When the account is linked, the section shows **Connected as @name**. **Unlink** removes the link after you confirm.

If this Telegram account is already linked to another TMGR account, the bot asks whether to move the link. Answer **Yes** or **No**.

Everything you create with the bot goes to your default workspace. That is a workspace you own, usually your first one. The bot works only while the Telegram bot module is on in that workspace.

### Routines {#telegram-routines}

- `/add Buy milk` creates a routine right away. It has no date and shows on today until you complete it. Add `!high`, `!medium` or `!low` to set the priority.
- Any other text creates a draft: the bot asks "Create routine: …?" with **Confirm** and **Cancel**. A draft you do not answer expires after an hour.
- A voice message is turned into text and becomes the same kind of draft. It can be up to 20 MB.

### Notes on pages {#telegram-notes}

The bot adds text to the end of a page. This needs the Pages module in your default workspace.

- `/note Ideas Try the new layout` adds "Try the new layout" to the page "Ideas".
- Reply to a text or voice message with `/note Ideas` to add that message.
- Send a voice message with the caption `/note Ideas` or `to Ideas`.
- Forward a message whose first line is `to Ideas` and the text below it.

The page is found by its title, its address or its number. You can put the name in quotes. If no page matches, the bot offers up to 3 similar pages as buttons. Each note is added as a line with the date. On a person page, it goes under the Timeline heading.

### Tasks {#telegram-tasks}

- `/task Fix the login page` creates a task with that title.
- Reply to any message with `/task` to make a task from it. The message goes into the description.

The task goes to your default workspace, in the default status and with no category. The bot replies "Task created" with a link.

`/task` also works in a group chat. Add the bot to the group and reply to a message with `/task`. Anyone in the group who linked their own Telegram can do this, and the task goes to their own default workspace. The group sees only "Task created."; details come to you in a private chat with the bot.

`/help` lists the commands.

### Notifications in Telegram {#telegram-notifications}

Open [Settings → Notifications](/settings?tab=notification) and find **Telegram notifications**. Turn on **Send notifications to Telegram bot** and tick the types you want. TMGR sends these types to Telegram:

- **Task status changed** and **New comment**, to the author and the assignees of the task. **Task assigned**, to the people who were just assigned. You do not get a notification about your own change.
- **New page**, **Page updated** and **You were mentioned on a page**.
- **Member removed**.

In the **Agents** section of the same page, the **Telegram** switch sends you the notifications that AI agents send with a notify token. See [Notifications and agent alerts](/docs/app/notifications).

## GitHub {#github}

The GitHub App links a repository to a category. Commits, branches and pull requests that mention a task key then show up on that task. This needs the GitHub module.

### Link a repository {#github-link}

Any member can do this, in the category settings. See [Categories](/docs/app/categories#integrations).

1. In the **Integrations** section, press **Install GitHub App** next to **GitHub**.
2. When the app is installed, choose a repository under **Select Repository** and press **Connect Repository**.

A category has one repository. The panel then shows "Connected to" and the repository name, and when it last synced.

- **Sync Now** reads the branches of the repository again.
- **Relink Tasks** checks the stored commits, branches and pull requests again and links them to tasks.
- **Disconnect** unlinks the repository after you confirm.

Good to know: installing the GitHub App for the first time is paused for now, and **Install GitHub App** shows an error. Workspaces that already have the app installed can still link repositories.

### See git activity on a task {#github-activity}

Put the task key, for example TM-24, in one of these:

- a commit message;
- the name of a new branch;
- the title or description of a pull request.

TMGR takes the first key of the category in that text. The key is the one in front of the task title, or the task's ticket number.

In the task, the **Git activity** row shows how many events there are. Click it to open **Git Activity** with **Branches**, **Commits** and **Pull Requests**. A pull request shows whether it is open, merged or closed, and its branches. If the category has no repository, the task says "No GitHub repository linked" and offers **Open category settings**.

## Cursor agents {#cursor}

Cursor agents are cloud agents from Cursor. You can start one from a task, and it works on the code in your GitHub repository. This needs the Cursor agents module, a Cursor API key in the category and a GitHub repository linked to the same category.

### Add the API key {#cursor-key}

1. In the category settings, in **Integrations**, press **Connect** next to **Cursor AI agent**.
2. Paste the key into **Cursor API Key**. You get it from the Cursor Dashboard.
3. Press **Save API Key**.

The panel then says **Cursor AI Agent Configured**. TMGR never shows the key again. Use **Update Key** to replace it and **Remove Key** to delete it. Any member of the workspace can do this.

### Run an agent {#cursor-run}

1. In a task with a category, press the robot button **Run with Cursor Agent**. The **Cursor AI Agent** window opens.
2. Under **Launch New Agent**, choose the **Source Branch**. Type in "Search branches..." to find one.
3. Press **Launch Cursor Agent** to have the agent change the code, or **Research** for a report only.

The agent gets the task title, description, category and the comments of people. There is no separate prompt.

- **Launch Cursor Agent**: the agent works in a new branch and opens a pull request. It is asked to put the task key into the pull request title and the commit messages, so they show up in the task's git activity.
- **Research**: "The agent investigates the task and posts a report as a comment. It does not change code." No branch and no pull request are made.

**Agent History** lists the agents of the task with their status, **Branch:**, **Summary:** and **View Pull Request**. The window refreshes on its own while an agent runs; **Refresh status** checks right away. **Stop** stops a running agent. **Active agents** at the top of the page, or in the status bar of the desktop app, shows the agents running now.

### Send a follow-up {#cursor-follow-up}

While an agent runs, you can add instructions:

- in the agent window, under **Send Follow-up Instruction**, then **Send**;
- or write a comment and press **Send to Cursor Agent** in the comment box.

The text is also saved as your comment on the task. It can be up to 5000 characters.

## Sign-in methods {#sign-in}

The sign-in page offers **Google**, **Apple** and **GitHub**, then Telegram, then "Or continue with email" with your email and password. The desktop app has the same buttons and a **Telegram** button that signs in through your browser.

- **Telegram** signs you into the TMGR account that linked this Telegram account to the bot. If no account is linked, TMGR creates a new one. Telegram never signs you into an account by email.
- **Google**, **Apple** and **GitHub** sign you into your account. If no account has that email yet, TMGR creates one.

### Add a sign-in method to your account {#sign-in-link}

If you already have an account and sign in with a provider for the first time, TMGR has to make sure the email is yours:

- Google with a Gmail address or the domain of your Google Workspace, and Apple with a private relay address, are linked to your account right away.
- In other cases, and always with GitHub, TMGR emails you a confirmation link. It works for an hour. Open it while you are signed in to your account, press **Confirm linking**, then sign in with the provider again.

If the account's email was never verified, signing in for that address with a provider from the first point (Google with a Gmail or Google Workspace address, Apple with a private relay address) takes the account over. The old password stops working, other sign-in methods and the Telegram link are removed, and all sessions end.

There is no page that lists or removes your sign-in methods. If you signed up with Google, GitHub or Apple and never set a password, use "Forgot your password?" on the sign-in page to set one.
