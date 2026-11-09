---
title: Quick add, shortcuts and time
section: Desktop app
order: 27
description: Add routines, tasks and notes from any app, set global shortcuts, turn screenshots and selected text into tasks, and let the app keep your timer right.
---

The desktop app works while it is in the background. Global shortcuts reach it from any app, and it notices breaks and branch changes, so your timer stays right.

## Quick add {#quick-add}

Press ⌥Space on macOS or Alt+Space on Windows and Linux, in any app. A small window opens on top of everything. Pick what to add with the buttons at the top:

- **Routine** adds a Daily Routine. This is the default.
- **Task** adds a task to the backlog. Pick a **Category**, or keep **No category**.
- **Task + screenshot** adds a backlog task with an image. Press **Capture screenshot** (macOS) or **Choose image**.
- **Page** adds text to a page. Pick the page and a **Section**, or keep **End of page**.

Next to the heading, choose the workspace. The list has your cloud workspaces and the local workspace that is open.

Type a title and press Enter or **Add ↵**. **+ Description** adds a description.

Esc closes the window, and so does a click in another app. What you typed stays as a draft for the next time. **Discard** clears it.

## Global shortcuts {#shortcuts}

Global shortcuts work from any app, even when TMGR is hidden. The defaults:

| Action | macOS | Windows and Linux |
| --- | --- | --- |
| **Quick add** | ⌥Space | Alt+Space |
| **Start / stop timer** | ⌥⇧T | Alt+Shift+T |
| **Screenshot to task** | ⌥⇧S | not available |
| **Selected text to task** | ⌥⇧C | not available |

**Start / stop timer** stops the running timers. If none is running, it starts the timer of your last task again.

To change a shortcut, open [Settings → Desktop](/settings?tab=desktop) and find **Keyboard shortcuts**. The "Shortcuts…" item in the tray menu opens the same place.

- Click a shortcut and press the new keys. Esc cancels.
- A shortcut needs at least one of ⌘, ⌃ or ⌥ on macOS, or Ctrl, Alt or Win (Super on Linux) on the other systems.
- The arrow button next to it (**Reset to default**) brings the default back.
- The switch turns the shortcut off.

If another app already uses the keys, you see "Taken by another app — choose a different shortcut".

## Screenshots and selected text {#capture}

Both captures work on macOS. On Windows and Linux, use **Task + screenshot** in quick add and press **Choose image**.

**Screenshot to task** (⌥⇧S): select an area of the screen. Quick add opens in **Task + screenshot** with the image. Type a title and press **Add ↵**. The task goes to the backlog with the image attached.

**Selected text to task** (⌥⇧C): select text in any app and press the shortcut. TMGR copies the selection and opens quick add with it. A short selection becomes the title. A longer one gives the title its first line and goes into the description in full. If quick add already has a draft, the text is added to it. Text that was on your clipboard is put back.

### macOS permissions {#permissions}

- Screenshots need Screen Recording. Allow TMGR in System Settings → Privacy & Security → Screen Recording.
- Selected text needs Accessibility. macOS asks the first time. If the permission is missing, quick add says: "Allow TMGR in System Settings → Privacy & Security → Accessibility, then try again."

## Away detection {#away}

When a timer runs and you leave the computer for 10 minutes or more, or the computer sleeps, TMGR notices when you come back. It asks **You were away for …** and says since when the timer kept running and on which task:

- **Keep the time** closes the question. The timer keeps running.
- **Stop the timer** stops the running timers now.

Neither button takes the time you were away off the task. If TMGR is in the background, you also get a notification.

This works on macOS and Windows. On Linux, TMGR notices only that the computer slept, not that you left it awake.

## Git branch timer {#git-branch}

TMGR can start or switch your timer when you switch git branches. Set it up in [Settings → Desktop](/settings?tab=desktop), under **Git auto-tracking**:

1. Press **Add folder** and choose a git repository. You can watch up to 20 folders.
2. Choose the **Mode**:
   - **Ask (default)**: TMGR asks "Start timer on TM-157?" or "Switch timer to TM-157?" with a **Start timer** button.
   - **Auto**: the timer switches to the task right away, and you see "Timer switched to TM-157". Completed and archived tasks are skipped.
   - **Off**: branch changes are ignored.

The task key comes from the branch name, for example `feature/TM-157-login` gives TM-157. Only tasks of the current workspace are matched, and only the branch name is read. Nothing happens if the timer already runs on that task.
