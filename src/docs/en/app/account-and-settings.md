---
title: Account and settings
section: Web app
order: 36
description: Change your profile and password, manage signed-in devices and tokens, work with several workspaces, and set notifications, theme, language, timezone and privacy.
---

Your account holds your profile, your devices and your own preferences. Settings also hold the settings of the workspace you are in. Both are behind the account menu at the bottom of the sidebar: **Profile** and **Settings**.

## Where settings are {#settings}

Settings have four groups:

| Group | Items |
|---|---|
| **Account** | **Profile**, **Notifications**, **Smart Devices**, **Privacy** |
| **Workspace** | **General**, **Modules**, **Personas** |
| **Agents** | **Agent work**, **Agent alerts**, **Plugins** |
| **App** | **Theme**, **Desktop**, **Local data**, **Experimental**, **About** |

Some items show only when they apply. **Personas** needs the Personas module, and **Agent alerts** needs the Agent alerts & alarms module. **Plugins**, **Desktop** and **Local data** are in the desktop app only.

## Your profile {#profile}

Open [Settings → Profile](/settings?tab=profile). The same page also opens at [/profile](/profile), without the settings menu.

- **Profile picture**: press **Change picture**, then crop the image and save. PNG, JPEG, WebP and GIF work. **Remove** deletes the picture.
- **Account**: change your **Name**. You see your **Email**, but you cannot change it here.

Press **Save** to keep the changes.

### Change your password {#password}

Under **Change password**, fill in **Current password**, **New password** and **Confirm password**, then press **Save**. Leave the fields empty to keep your password. The new password needs at least 6 characters.

Did you sign up with Google, GitHub or Apple and never set a password? Use Forgot password on the sign-in page to set one.

A new password turns off the tokens of your account: the device token, the persona tokens and the notification tokens. You see "Device, agent and notification tokens were revoked — generate new ones." Create new tokens where you need them.

**Sign out of other devices** is on by default. With it on, a new password also signs you out everywhere else: other browsers, the desktop app on other computers and the command line. This device stays signed in. The box does nothing unless you change the password.

### Signed-in devices {#devices}

**Signed-in devices** lists the command-line sessions you started with `tmgr login`. Each entry shows the computer, when you signed in, when the session was last used and its scopes.

- **Edit scopes** takes scopes away from a session. You cannot add new ones here.
- **Revoke** signs the session out, after you confirm.

More about tokens and sessions: [MCP server: tokens](/docs/mcp#tokens).

### Agent connections {#agent-connections}

**Revoke all persona tokens** turns off the tokens of all your personas in all your workspaces at once. Agents that used them lose access until you give them new tokens.

### Telegram {#telegram}

With the Telegram bot module on, the profile has a **Telegram** block. Press **Connect Telegram** to link your account, or **Unlink** to remove the link. See [Integrations](/docs/app/integrations#telegram).

### Deleting your account {#delete-account}

The app has no button to delete your account yet.

## Smart Devices {#smart-devices}

[Settings → Smart Devices](/settings?tab=device) holds one API token for your account. Agents, scripts and the task MCP server use it.

- **Generate token** creates it. Copy it right away: "Copy it now — it won't be shown again."
- **Generate new token** replaces the old one, so the old one stops working.
- **Revoke token** turns it off.

The page also shows how to connect an MCP client. The token block shows only while the MCP & API tokens module is on; it is on by default. More: [MCP server: tokens](/docs/mcp#tokens).

## Workspaces {#workspaces}

### Switch {#switch}

Open the workspace switcher at the top of the sidebar and pick a workspace. You can also use the keys ⌘1 … ⌘9 or Ctrl+1 … Ctrl+9, see [Search and shortcuts](/docs/app/search-and-shortcuts#global). Switching does not change your default workspace.

### Add a workspace {#add}

In the switcher, choose **Add workspace**, or press **New workspace** in [Settings → General](/settings/workspaces). Type a name and press **Create**. You become the owner of the new workspace, and it opens.

### Workspace settings {#workspace-settings}

[Settings → General](/settings/workspaces) always shows the workspace you are in.

- **Workspace**: the owner changes the **Name** and the **Colour** of the workspace and presses **Save**. Other members see the name only.
- **Preferences** are settings of your own account, not of the workspace. They apply in every workspace. Their names are shown in English.
- **Default workspace**: choose one and press **Set as default**. It is "used by Telegram, MCP, smart devices, and when you open TMGR on a new device."
- **Members** and **Invitations**: see [Your team](/docs/app/team).

### Leave or delete {#leave-delete}

To leave a workspace you do not own, press **Leave workspace** next to it in the switcher, or **Exit from workspace** in [Settings → General](/settings/workspaces). To come back, you need a new invitation. The owner cannot leave their own workspace.

The owner can press **Delete workspace** and type the workspace name to confirm. It deletes the workspace with its tasks, pages and files for every member, and cannot be undone. A default workspace cannot be deleted.

## Notifications {#notifications}

[Settings → Notifications](/settings?tab=notification) has a block for each channel: **Web notifications**, **Telegram notifications**, **Email notifications** and **Agents**. Changes save by themselves. What each notification means: [Notifications](/docs/app/notifications).

## Theme, language and timezone {#theme}

All three are in [Settings → Theme](/settings?tab=theme).

- **Theme**: pick one of the colour themes. On the **Default** theme, **Mode:** switches between **Light**, **Dark** and **System**.
- **Language**: pick the **Interface language**. tmgr has 10 languages. Until you pick one, it follows your browser.
- **Timezone**: "Daily routines fire and days start in this timezone." If no timezone is set yet, tmgr takes the timezone of your device. Search for another one, or press **Use detected** to go back to the detected one.

## Privacy {#privacy}

[Settings → Privacy](/settings?tab=privacy) has **Share usage data**. It is off by default. The app describes it as "Not linked to your account: event names, a random install ID, app version and platform. Never task content, names or emails." Technical details such as the screen size class and the account mode are sent too. The choice is kept in this browser. Turning it off stops sharing at once.

The full [privacy policy](/privacy) is at /privacy.
