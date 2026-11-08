# TM-903: Decisions on the phone: listening indicator and chat on every card

Task: https://tmgr.dev/tmgrdev/tasks/10643. Design source: `mobile/docs/plans/2026-11-08-tm-902-listening-indicator-and-card-chat.md`.
Scope shipped here: items 1 (listening) and 2 (chat). Item 3 (choices[] with notes, multi-select, code/diff) is planned at the end, not built.

## What exists already (desktop, frontend repo)

- `hub.rs` keeps per card `Listen { waiters, last, delivered }`. A card is listening while a `get_answer` / `wait` call is blocked on it, plus a 5 s grace. The waiter is tied to the live HTTP request, so a dead agent process stops listening within the grace. `CardView.listening` and `MessageView.delivered` (bool) already reach the desktop window.
- `deliveries(card_id, seq, state, via, at_ms)` records per event when the agent picked it up (`delivered`, via `agent`) or a push hook sent it (`sent`). This is the `delivered_ms` source.
- Owner messages exist: `Store::owner_message` → event `kind=message` + row in `messages`; the agent receives them through `get_answer`.
- Task comment ledger (`task_comments`, `pending_task_comments`, posted by the main window) covers final answers only.

## Design decisions

1. **Volatile state goes into `SnapshotCard`, not into the projection.** The phone answer is checked against `digest(projection)`. If `listening` or delivery times were inside the projection, every listener flip or pickup would change the digest and turn in-flight answers into `changed`. The snapshot body is signed as a whole (`snapshot_signed` covers the raw body text), so fields on `SnapshotCard` are as authenticated as the projection. Old phones drop unknown `SnapshotCard` keys (`parseSnapshot` picks known keys), old desktops never send them.
   New optional field (absent = unknown, the phone shows nothing):
   ```json
   "live": { "listening": true, "messages": [ { "seq": 3, "delivered_ms": 1731000000000 }, { "seq": 5, "saved_ms": 1731000100000 }, { "seq": 6 } ] }
   ```
   `listening` is present only for cards answered through a blocked `get_answer` (`reply_via` `mcp`/`cli`); push-delivered hook cards have no waiter, so the field is omitted there instead of claiming "offline". `messages` lists owner messages only, by message seq. `delivered_ms` = the agent picked it up (`deliveries.state` `delivered`/`sent`, `at_ms`). `saved_ms` = posted to the linked task. Neither = not yet.
   Delivery source: TM-896's `deliveries` ledger (`state` `delivered`/`sent`, `at_ms`). Gap fixed here: a `wait` whose cursor already passes a message marks it delivered only in memory; it now also writes the `delivered` row so the state survives a restart.
2. **Heartbeat.** The snapshot dedup signature includes `live`. While any card is listening, the desktop re-uploads at least every 60 s (`REFRESH_MS`) instead of 5 min. The phone treats `listening: true` as valid only when the snapshot is younger than 3 min, measured from when the phone fetched it (not the desktop clock) and also `generated_at` not older than 3 min plus 2 min skew allowance; when stale it shows no indicator (it cannot prove anything).
3. **Projection v2** (`p: 2`, stable content, in the digest): messages get `seq` and `created_ms`; new `chat: bool` (this desktop accepts `chat` for this card: live, eligible, `expects_agent`) and `task: bool` (card links a cloud task, so unread messages get saved there). The phone gates the composer on `chat === true`, never on `p`.
4. **Wire `chat`** = existing signed `AnswerBody` with `action: "chat"`, `text` required, `choice` null, `answer_id` = client id (idempotency through `phone_answers`, same 72 h ledger), `issued_at` window as answers. `answer_signed` already covers action and text. An old desktop would fail to parse `chat` and log a verification failure, which is why the phone sends it only when the projection says `chat: true`.
   Desktop handling (`phone_answer_in`): duplicate → `duplicate`; expired → `expired`; card closed/aliased → `closed`; not eligible or not chat-capable → `not_eligible`; empty/too long text → `invalid`. Neither the digest nor the session is compared for chat: the thread itself changes the projection, the message decides nothing, and a message to the card thread is meant for whichever session now serves the card. Applied → the owner-message writer (split out of `owner_message` so it takes `&Connection` and opens no transaction, because `phone_answer` already runs in `BEGIN IMMEDIATE`), author `owner`, event `via: "phone"`. The card stays open, the desktop notifies "Message from iPhone", the event is routed to the bound session like a desktop-typed message. The ack is always `applied` (label "Message") whatever the route outcome, so `TargetChanged` never makes the phone resend a stored message.
5. **Saved to the task.** `pending_task_comments` also returns phone-sent (`via: "phone"`) owner message events that have no `delivered`/`sent` delivery 30 min (`UNPICKED_MS`) after they were written, or whose card closed, for cards that expect an agent and link a cloud task, within the 7-day window. Body: `Message on decision: <title>` / `Message: <text>` / `Card: <id>` / `[decision <id>#<seq>]`. Same ledger key (card, event seq). Desktop-typed messages are not posted (the owner is at the desktop); no backlog exists because no phone message events exist before this release.
6. **Desktop window**: a header pill on live cards that expect an agent: "Agent is listening" / "Agent offline". Strings in all 10 locales.
7. **Phone** (JS-only, OTA): parse `live`, `chat`, `task`, message `seq`/`created_ms`. Header: "Agent is listening" or "Agent offline - reply will be saved to the task" (`task: true`) / "Agent offline - it will see your reply when it comes back" (`task: false`). Shown only when `live` is present and fresh. Per owner message: "Read by agent" (`delivered_ms`), "Saved to the task" (`saved_ms`), "Delivered later" (listed with neither). Composer on every card with `chat: true`; send bypasses the undo/hide `pendingAnswer` flow; ack `applied` keeps the card (own `describeChatAck`), refetches the snapshot, and the answer buttons are disabled ("Updating card...") until a snapshot with a new digest for that card arrives, so a follow-up answer never carries the stale digest. Cards without `chat` keep today's behaviour.

## Order of work

1. Frontend worker (worktree `task-manager-ui-worktrees/tm-903-desktop`, branch `tm-903-decisions-listening-chat`): TDD in `crates/tmgr-phone` (Action::Chat, `SnapshotCard.live`), `projection.rs` (p 2, seq, created_ms, chat, task), `poller.rs` (live + heartbeat cadence), `store.rs` (`phone_answer_in` chat branch, message task comments + upgrade skip), `apply.rs` (chat notify/route), Vue header pill + locales. `cargo test -p tmgr-phone`, `cargo test` in `src-tauri` (decisions), jest for `src/decisions`.
2. Mobile worker (worktree `mobile-worktrees/tm-903-phone`): `src/lib/phone/proto.ts` (Action `chat`, parse `live`), `card.ts`, `controller.ts` chat send, `DecisionCardScreen.tsx`. Jest. Cross-language fixture: a Rust-sealed chat answer and a v2 snapshot verify in TS.
3. Reviews: Opus self-review of both diffs, then Codex adversarial review on the frontend PR (protocol/security).
4. Release: frontend PR ff-merged → `desktop-v*` (gated by orchestrator "go") → mobile OTA `1.21.0 (OTA 2)` (gated).

## Item 3 (not built): richer answers

- Wire: `AnswerBody.choices: Option<Vec<{id, note}>>` signed as `h(canonical JSON of choices)` appended to `answer_signed` under a new prefix `tmgr-decision-answer:v2` (v1 stays for single choice). Desktop advertises `multi: true` / `notes: true` in the projection; the phone sends v2 only then.
- Desktop: `AnswerIn` gains `choices`; `write_answer` keeps `choice` = first id for old agents and adds `choices` to the event; MCP `get_answer` returns both.
- Projection: `code: {file, start, lines[]}` and `diff: {file, lines: [{kind: add|del|ctx, text}]}` cut to the 16 KiB budget before `context`.
