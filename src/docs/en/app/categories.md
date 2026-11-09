---
title: Categories
section: Web app
order: 24
description: Group tasks into categories and subcategories, get task keys from the category code, and set defaults for new tasks.
---

A category groups tasks. Its code gives tasks their keys: a task in a category with the code `ABC` gets a key like `ABC-12`, the code plus the task's number in that category. Categories belong to the module that Settings → Modules lists as Projects; turn it on in [Settings → Modules](/settings/modules).

## Create a category {#create}

The **Categories** section of the sidebar shows your categories as a tree. To add one:

1. Press the plus next to the section title (**New category**).
2. Enter the **Name**. To put it inside another category, pick a **Parent**.
3. If other categories already have settings, **Copy settings from** fills the **New tasks** fields from one of them. You can still change them.
4. Fill in the **New tasks** fields if you need them (see [Settings](#settings)).
5. Press **Create**.

The new category opens on its **Settings** tab.

TMGR makes the code from the name: a category named "Mobile app" gets `MOBILE-APP`, or `MOBILE-APP-1` if that code is taken. What you type in **Code** is not used, and the code stays the same after that. If you change it and save, you see "The server kept the old code".

The search button in the section (**Filter categories**) filters the tree by name.

## Subcategories {#nest}

A category can hold other categories. To add one, open the category and press **Subcategory**, or pick a **Parent** when you create a category. In the sidebar, subcategories sit under their parent; press the arrow to expand or collapse them.

To move a subcategory under another parent, change **Parent** on its **Settings** tab. Moving it back to the top level does not work: **No parent** is not saved, and you see "The server kept the old parent".

## The category page {#tabs}

Click a category in the sidebar to open it. The header shows the path to it, its name, and two buttons: **Subcategory** and **Create task**, which starts a new task in this category. Below are three tabs:

- **Tasks**: the tasks of the category, in the same rows as [the task list](/docs/app/task-list), with selection and pages.
- **Subcategories**: a card for each direct subcategory, with its number of subcategories and tasks and its author. Click a card to open the subcategory. Its ⋮ menu has **Edit** (opens its settings), **Move to workspace** and **Delete**.
- **Settings**: see below.

## Settings {#settings}

The **Settings** tab has the sections **General**, **New tasks**, **Integrations** and **Danger zone**. When you change something, a bar at the bottom shows the number of unsaved changes with **Discard** and **Save**. If you leave with unsaved changes, TMGR asks "Discard unsaved changes?".

**General** holds the **Name**, the **Code** and the **Parent**.

**New tasks** holds defaults for tasks created in this category. TMGR copies the rate and the estimate into each new task when it is created; changing them later does not change existing tasks.

- **Task title pattern**: a prefix for the titles of new tasks. When you pick this category for a new task and its title is still empty, the title is filled from the pattern. Tasks that agents create through MCP get the prefix before their title, like `TM-157: Fix login`.
- **Rate per hour**: the hourly rate for time on tasks in this category. TMGR uses it to work out what a task cost.
- **Default estimate**: the estimate a new task gets, written like `45m`, `2h` or `1h 30m`. When the tracked time passes it, the time on the task turns red.

### The title pattern {#title-pattern}

A pattern is text with placeholders, for example `TM-{index#category}`:

- `{index#category}`: the number of tasks in this category, plus one.
- `{index#workspace}`: the number of tasks in the workspace, plus one.
- `{dts#<mask>#dte}`: the current date and time, written by the mask. For example, `{dts#dd.mm.yyyy#dte}` gives `09.11.2026`.

The mask tokens are:

- `yyyy` and `yy`: the year, as `2026` or `26`.
- `mm`: the month, `01` to `12`.
- `dd`: the day, `01` to `31`.
- `ddd` and `dddd`: the weekday, as `Mon` or `Monday`.
- `hh` and `h`: the hour, `00` to `23`, with or without a leading zero.
- `ii` and `i`: the minutes, with or without a leading zero.
- `ss` and `s`: the seconds, with or without a leading zero.

On a wide screen, the buttons under the field insert placeholders: **+ category number**, **+ workspace number**, **+ date** and **+ weekday**. **Preview** shows how a title will look.

The number from the pattern is not the task key. The key comes from the category code and the task's own number, and the two can differ.

A pattern without any placeholders goes into every title as it is. TMGR warns "No placeholders." under the field and offers a button that turns the text into a date mask. In the sidebar, such categories get a warning sign.

## Integrations {#integrations}

The **Integrations** section shows on a wide screen, once the category exists:

- **Cursor AI agent**, with the Cursor agents module on: sends tasks of this category to Cursor agents. Press **Connect**, add your **Cursor API Key** and press **Save API Key**.
- **GitHub**, with the GitHub module on: links tasks to a repository. Press **Install GitHub App** or **Manage**.
- **Agent repositories**, when **Let my agents run my tasks** is on in [Settings → Modules](/settings/modules): the repositories, as `owner/name`, where agent runs may work on tasks of this category. Leave it empty to use the linked GitHub repository. Press **Add repository**.

Details are in [Integrations](/docs/app/integrations): see [GitHub](/docs/app/integrations#github) and [Cursor](/docs/app/integrations#cursor).

## Move or delete {#danger}

The **Danger zone** section of the **Settings** tab has two actions:

- **Move to another workspace**: press **Move…**, pick a workspace and press **Move**. The category moves with its subcategories and tasks. Task statuses are matched in the other workspace by name, then by type. Assignees who are not members there are removed.
- **Delete category**: press **Delete…** and confirm. The category and its subcategories leave the list. Their tasks stay: they move to the parent category, or lose their category if it was a top-level one.

**Delete** in a card menu on the **Subcategories** tab works at once, without asking. The card stays on the tab, marked "(deleted, but can be restored)", and its menu has **Restore** until you leave the page. Restoring brings back the subcategory, its own subcategories and its tasks. After you leave the page, the web app has no place to restore it from.

## Next {#next}

- [Tasks in detail](/docs/app/tasks)
- [The task list](/docs/app/task-list)
- [Integrations](/docs/app/integrations)
