---
title: Notifications and agent alerts
section: Web app
order: 25
description: Read your notifications, set up agent alerts and an alarm phone so an agent can reach you.
---

TMGR has two kinds of messages. Notifications tell you what happened in your workspace. Agent alerts let an AI agent reach you on your phone when it needs you.

## The notifications page {#notifications-page}

The bell in the sidebar shows how many notifications are unread. Click it to see the latest ones, then choose View all to open the full Notifications page.

A notification covers things like a new task, a status change, a new comment, a member joining, a page mention, or an agent starting or finishing work.

- An unread notification is bold and has a dot. A read one is plain.
- Click a notification to mark it read and open the task, page or other item it is about.
- Press Mark all as read at the top to clear every dot at once. The button shows only while something is unread.
- Point at a notification and press the cross to delete it.
- The page loads in parts. Press Load more to see older ones.

The Settings button at the top opens Notification Settings.

## Agent alerts {#agent-alerts}

Open [Settings → Agent alerts](/settings/agent-notifications). Here you:

1. Create a token for an agent.
2. Connect Claude Code, Codex, Cursor or another client with it.
3. Check that it works with Send test notification.

The steps for the token and the clients are in the [AI agents guide](/docs/agents#get-a-token). You do not need to repeat them here.

The Tokens list shows each token's prefix, when it was created and when it was last used. Press Revoke to stop a token from working. A token always sends alerts to you, the token owner, in every workspace you are in.

### An alert on your phone {#alert-on-phone}

An alert is a push notification with a title and a message from the agent. It arrives in the TMGR mobile app. If you linked Telegram in your profile, it can arrive there too. A test alert is titled "Test from TMGR".

If the test says nothing was delivered, install the TMGR mobile app and allow notifications, or link Telegram in your profile.

## Alarm phone {#alarm-phone}

An alarm is the loud kind of alert, for things like "production is down". The alarm phone is the number TMGR calls if you do not react to the push.

### Add and verify the number {#verify-phone}

1. In the Alarm phone section, enter your number in international format, for example +15550100123, and press Save.
2. TMGR texts you a code. Enter it in Verification code and press Verify.
3. No code? Press Resend code. You can ask again after a 60 second wait.

After too many wrong codes you have to enter the number again. The number is stored encrypted. Use Change to replace it and Remove to delete it.

Once the number shows Verified, press Test call to hear what an alarm call sounds like.

### What happens when an alarm goes off {#escalation}

1. The alarm rings in the TMGR mobile app first.
2. If you do not acknowledge it in time and the agent asked for a call, TMGR calls your verified alarm phone. The call reads the title and message and asks you to press 1 to acknowledge.
3. If a call is not answered, TMGR tries again after a short pause, up to 3 times unless the agent asked for another number (1 to 5).

The agent can change how long it waits before it calls and whether it calls at all. By default it waits 90 seconds for you to acknowledge the push.

### Make sure the call gets through {#reliable-calls}

The section shows the number alarms call you from. Press Add to contacts or Copy, then add it to Favorites.

- iPhone: in Focus → Do Not Disturb and Sleep, allow calls from Favorites and turn on Repeated Calls.
- Android: star the contact and allow starred contacts in Do Not Disturb.

If the section says the alarm phone is not configured on this server, calls are not available there.

## Phones {#phones}

The Phones section is in the TMGR desktop app only. It pairs an iPhone with your Mac so that you can answer Decisions cards from the phone. The Mac must be on while you pair. Use Pair an iPhone to start and Manage phones to see the paired ones. This is a separate feature from the alarm phone.
