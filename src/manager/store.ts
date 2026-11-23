import { computed, reactive } from 'vue';
import * as realApi from './api';
import {
	createDeliveryTracker,
	failureReason,
	isFailureNote,
	newMessageId,
	SEND_TIMEOUT_REASON,
	SENDING_MS,
	type Delivery,
	type Receipt,
} from './delivery';
import {
	firstEnabled,
	injectGate,
	isMentionStateRow,
	mentionCandidates,
	mentionHandleText,
	mentionQuery,
	mentionSupported,
	nextEnabled,
	stripMention,
	type MentionItem,
} from './mentions';
import { CLAUDE_MODEL_LABELS } from './models';
import {
	MAX_ATTACHMENTS,
	MAX_BODY_BYTES,
	roomKey,
	type ActivityEvent,
	type ActivityView,
	type Attachment,
	type AuditEntry,
	type ChatMessage,
	type MentionReq,
	type ModelChoice,
	type RoomInfo,
	type RoomRef,
	type SlashCommand,
	type SpawnAgent,
	type SpawnRequest,
	type ToolCall,
} from './types';

export interface ManagerApi {
	rooms: typeof realApi.managerRooms;
	history: typeof realApi.managerHistory;
	send: typeof realApi.managerSend;
	commands: typeof realApi.managerCommands;
	search: typeof realApi.managerSearch;
	queued: typeof realApi.managerQueued;
	queuedWhy?: typeof realApi.managerQueuedWhy;
	cancel: typeof realApi.managerCancel;
	agentModels: typeof realApi.managerAgentModels;
	onMessage: typeof realApi.onManagerMessage;
	onActivity: typeof realApi.onManagerActivity;
	onReceipt?: typeof realApi.onManagerReceipt;
	audit: typeof realApi.managerAudit;
	spawn: typeof realApi.managerSpawnSession;
	switchAgent: typeof realApi.managerSwitchAgent;
	stop: typeof realApi.managerStop;
	remove: typeof realApi.managerRemove;
	caps?: typeof realApi.runnerCaps;
	mentionInject?: typeof realApi.managerMentionInject;
}

export type PanelTab =
	'session' | 'hierarchy' | 'audit' | 'encryption' | 'voice';

export const AGENT_MODELS: Record<SpawnAgent, string[]> = {
	claude: Object.keys(CLAUDE_MODEL_LABELS),
	codex: [
		'gpt-6.1-terra',
		'gpt-6.1-sol',
		'gpt-6-astra',
		'gpt-6-sol',
		'gpt-6-luna',
		'gpt-5.6-sol',
		'gpt-5.6-terra',
		'gpt-5.6-luna',
	],
	opencode: ['default'],
	agy: ['default'],
};

const emptyForm = (): SpawnRequest => ({
	agent: 'claude',
	machine: '',
	folder: '',
	name: '',
	task: '',
	prompt: '',
});

export interface FileLike {
	name: string;
	type: string;
	size: number;
	sha256: string;
}

const PAGE = 50;
export const MAX_FILE_BYTES = 25 * 1024 * 1024;
const MAX_JUMP_PAGES = 20;
const QUEUE_POLL_MS = 2000;
const POST_SEND_POLLS = 3;
const ACTIVITY_TTL_MS = 60_000;
const MAX_TOOLS = 50;
const MAX_LABEL = 80;
const MAX_SUMMARY = 200;
const MAX_RESULT = 8 * (MAX_SUMMARY + 1);
const MAX_TOOL_MESSAGES = 200;

const capCalls = (calls: ToolCall[] | undefined): ToolCall[] =>
	Array.isArray(calls)
		? calls.slice(-MAX_TOOLS).map((c) => ({
				n: typeof c.n === 'number' ? c.n : undefined,
				name: String(c.name ?? '').slice(0, 60),
				summary: String(c.summary ?? '').slice(0, MAX_SUMMARY),
				result:
					typeof c.result === 'string'
						? c.result.slice(0, MAX_RESULT)
						: undefined,
				running: c.running === true,
				error: c.error === true,
			}))
		: [];

const byTime = (a: ChatMessage, b: ChatMessage) =>
	a.ts_ms - b.ts_ms || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0);

const merge = (
	existing: ChatMessage[],
	incoming: ChatMessage[],
): ChatMessage[] => {
	const seen = new Set(existing.map((m) => m.id));
	const fresh = incoming.filter((m) => {
		if (seen.has(m.id)) return false;
		seen.add(m.id);
		return true;
	});
	if (!fresh.length) return existing;
	return [...existing, ...fresh].sort(byTime);
};

const slashName = (command: SlashCommand) => command.name.replace(/^\//, '');

class SendTimeout extends Error {}

const SEND_TIMEOUT_TEXT =
	'The message was not confirmed as sent in time; it may not have been sent. Retry is safe.';

const errorText = (e: unknown) => (e instanceof Error ? e.message : String(e));

export const createManagerStore = (api: ManagerApi) => {
	const commandCache = new Map<string, SlashCommand[]>();
	let searchSeq = 0;
	let queueSeq = 0;
	const modelsRequested = new Set<string>();
	let modelTouched = false;

	const unread = reactive({} as Record<string, number>);
	let queueTimer: ReturnType<typeof setInterval> | null = null;
	let postSendPolls = 0;
	const activityTimers = new Map<string, ReturnType<typeof setTimeout>>();
	const lastUsed = new Map<
		string,
		{ used: string[]; calls: ToolCall[]; at: number }
	>();

	const tracker = createDeliveryTracker((id, d) => {
		if (d) store.delivery[id] = d;
		else delete store.delivery[id];
	});

	const toolOrder: string[] = [];
	const toolRoom = new Map<string, string>();

	const dropTools = (id: string) => {
		delete store.toolsByMessage[id];
		delete store.callsByMessage[id];
		toolRoom.delete(id);
	};

	const rememberTools = (
		id: string,
		key: string,
		used: string[],
		calls: ToolCall[],
	) => {
		store.toolsByMessage[id] = used;
		if (calls.length)
			store.callsByMessage[id] = calls.map((c) => ({ ...c, running: false }));
		toolRoom.set(id, key);
		toolOrder.push(id);
		while (toolOrder.length > MAX_TOOL_MESSAGES) {
			const old = toolOrder.shift();
			if (old) dropTools(old);
		}
	};

	const forgetRoom = (key: string) => {
		lastUsed.delete(key);
		dropActivity(key);
		for (const [id, room] of [...toolRoom]) {
			if (room !== key) continue;
			dropTools(id);
			toolOrder.splice(toolOrder.indexOf(id), 1);
		}
	};

	const dropActivity = (key: string) => {
		const timer = activityTimers.get(key);
		if (timer) clearTimeout(timer);
		activityTimers.delete(key);
		delete store.activityByRoom[key];
	};

	const stopQueuePoll = () => {
		if (queueTimer) clearInterval(queueTimer);
		queueTimer = null;
	};

	const loadModels = async (agent: SpawnAgent) => {
		if (agent === 'claude' || modelsRequested.has(agent)) return;
		modelsRequested.add(agent);
		try {
			const list = await api.agentModels(agent);
			if (!Array.isArray(list) || !list.length) return;
			store.switcher.choices[agent] = list;
			const sw = store.switcher;
			if (
				sw.open &&
				!sw.busy &&
				sw.agent === agent &&
				!modelTouched &&
				!list.some((m) => m.value === sw.model)
			)
				sw.model = list[0].value;
		} catch {
			modelsRequested.delete(agent);
		}
	};

	const syncQueuePoll = () => {
		if (!store.queuedIds.length && postSendPolls <= 0) {
			stopQueuePoll();
			return;
		}
		if (queueTimer) return;
		queueTimer = setInterval(() => {
			if (!store.queuedIds.length)
				postSendPolls = Math.max(0, postSendPolls - 1);
			void store.refreshQueued();
		}, QUEUE_POLL_MS);
	};

	const replaceRow = (key: string, id: string, next: ChatMessage) => {
		const list = store.messagesByRoom[key] ?? [];
		const rest = list.filter((m) => m.id !== id && m.id !== next.id);
		store.messagesByRoom[key] = [...rest, next].sort(byTime);
	};

	const unsent = new Set<string>();

	const sendWithin = (ms: number, params: Parameters<ManagerApi['send']>[0]) =>
		new Promise<ChatMessage>((resolve, reject) => {
			const timer = setTimeout(() => reject(new SendTimeout()), ms);
			api.send(params).then(
				(sent) => {
					clearTimeout(timer);
					resolve(sent);
				},
				(e) => {
					clearTimeout(timer);
					reject(e);
				},
			);
		});

	const deliver = async (job: {
		key: string;
		room: RoomRef;
		text: string;
		body: string;
		mention?: MentionReq;
		attachments: Attachment[];
		replyTo?: string;
		fromDraft?: boolean;
		retryOf?: string;
		reuseId?: boolean;
		tsMs?: number;
	}) => {
		const { key, room, text, body, mention } = job;
		const fromDraft = job.fromDraft !== false;
		if (new TextEncoder().encode(body).length > MAX_BODY_BYTES) {
			store.error = 'Message is larger than 64 KiB';
			return;
		}
		const id = job.reuseId && job.retryOf ? job.retryOf : newMessageId();
		const draftAttachments = [...job.attachments];
		const draftReply = store.replyTo;
		const optimistic: ChatMessage = {
			v: 1,
			id,
			room,
			author: { kind: 'owner', id: 'owner' },
			ts_ms: job.tsMs ?? Date.now(),
			kind: 'text',
			body_md: body,
			...(job.attachments.length ? { attachments: job.attachments } : {}),
			...(job.replyTo ? { reply_to: job.replyTo } : {}),
		};
		store.messagesByRoom[key] = merge(store.messagesByRoom[key] ?? [], [
			optimistic,
		]);
		tracker.begin(id, !store.originLocal(key));
		if (job.retryOf && !job.reuseId) tracker.retried(job.retryOf);
		const cleared =
			fromDraft && store.activeKey === key && store.draft.text === text;
		if (cleared) {
			store.draft.text = '';
			store.draft.attachments = [];
			store.replyTo = null;
			store.closeSlash();
			store.mention.picked = null;
			store.closeMention();
		}
		try {
			const sent = await sendWithin(SENDING_MS, {
				id,
				ts_ms: optimistic.ts_ms,
				room,
				body_md: body,
				reply_to: job.replyTo,
				attachments: job.attachments,
				...(mention ? { mention } : {}),
			});
			store.error = null;
			store.slash.picked = null;
			unsent.delete(id);
			if (sent.id !== id) tracker.rename(id, sent.id);
			replaceRow(key, id, sent);
			tracker.accepted(sent.id);
			postSendPolls = POST_SEND_POLLS;
			void store.refreshQueued();
		} catch (e) {
			const timedOut = e instanceof SendTimeout;
			const reason = timedOut ? SEND_TIMEOUT_REASON : errorText(e);
			store.error = timedOut ? SEND_TIMEOUT_TEXT : reason;
			unsent.add(id);
			tracker.rejected(id, reason);
			if (cleared && store.activeKey === key && !store.draft.text) {
				store.draft.text = text;
				if (!store.draft.attachments.length)
					store.draft.attachments = draftAttachments;
				if (!store.replyTo) store.replyTo = draftReply;
			}
		}
	};

	const store = reactive({
		rooms: [] as RoomInfo[],
		activeKey: null as string | null,
		messagesByRoom: {} as Record<string, ChatMessage[]>,
		shownRooms: new Set<string>(),
		unreadByRoom: unread,
		unreadTotal: computed(() =>
			Object.values(unread).reduce((a, b) => a + b, 0),
		),
		queuedIds: [] as string[],
		queuedWhy: {} as Record<string, string>,
		delivery: {} as Record<string, Delivery>,
		activityByRoom: {} as Record<string, ActivityView>,
		toolsByMessage: {} as Record<string, string[]>,
		callsByMessage: {} as Record<string, ToolCall[]>,
		replyTo: null as ChatMessage | null,
		draft: { text: '', attachments: [] as Attachment[] },
		slash: {
			open: false,
			query: '',
			items: [] as SlashCommand[],
			index: 0,
			picked: null as SlashCommand | null,
		},
		mention: {
			supported: false,
			open: false,
			items: [] as MentionItem[],
			index: 0,
			picked: null as MentionItem | null,
			confirm: null as
				| {
						kind: 'send';
						key: string;
						room: RoomRef;
						text: string;
						body: string;
						target: MentionItem;
						attachments: Attachment[];
						replyTo?: string;
				  }
				| { kind: 'inject'; message: ChatMessage }
				| null,
			injected: {} as Record<string, boolean>,
			injecting: {} as Record<string, boolean>,
		},
		search: {
			open: false,
			text: '',
			scopeKey: null as string | null,
			hits: [] as ChatMessage[],
		},
		focusId: null as string | null,
		error: null as string | null,
		panel: {
			tab: 'session' as PanelTab,
			audit: [] as AuditEntry[],
			auditLoaded: false,
			auditError: '',
		},
		newSession: {
			open: false,
			busy: false,
			error: '',
			form: emptyForm(),
		},
		control: {
			open: null as 'force' | 'delete' | null,
			key: null as string | null,
			busy: false,
			error: '',
			stillRunning: false,
		},
		switcher: {
			open: false,
			busy: false,
			error: '',
			agent: 'claude' as SpawnAgent,
			model: AGENT_MODELS.claude[0],
			choices: {} as Partial<Record<SpawnAgent, ModelChoice[]>>,
		},

		get switchModels(): string[] {
			const dynamic = store.switcher.choices[store.switcher.agent];
			return dynamic
				? dynamic.map((m) => m.value)
				: AGENT_MODELS[store.switcher.agent];
		},

		get switchLabels(): Record<string, string> {
			const dynamic = store.switcher.choices[store.switcher.agent] ?? [];
			return Object.fromEntries(dynamic.map((m) => [m.value, m.label]));
		},

		get machines(): string[] {
			return [...new Set(store.rooms.map((r) => r.room.machine))];
		},

		roomRef(key: string): RoomRef | null {
			return store.rooms.find((r) => roomKey(r.room) === key)?.room ?? null;
		},

		ingest(message: ChatMessage) {
			const key = roomKey(message.room);
			const list = store.messagesByRoom[key] ?? [];
			const before = list.length;
			const next = merge(list, [message]);
			if (next.length === before) return;
			store.messagesByRoom[key] = next;
			if (
				message.author.kind === 'system' &&
				message.reply_to &&
				isFailureNote(message.body_md) &&
				next.some((m) => m.id === message.reply_to && m.author.kind === 'owner')
			) {
				store.ingestReceipt({
					id: message.reply_to,
					state: 'failed',
					reason: failureReason(message.body_md),
				});
			}
			if (
				message.kind === 'text' &&
				!message.user_turn &&
				message.author.kind === message.room.kind
			) {
				const seen = lastUsed.get(key);
				if (seen && Date.now() - seen.at <= ACTIVITY_TTL_MS) {
					rememberTools(message.id, key, seen.used, seen.calls);
				}
				lastUsed.delete(key);
				dropActivity(key);
			}
			const info = store.rooms.find((r) => roomKey(r.room) === key);
			if (info && (!info.last || byTime(info.last, message) <= 0)) {
				info.last = message;
			}
			if (
				key !== store.activeKey &&
				message.author.kind !== 'owner' &&
				!message.user_turn &&
				!isMentionStateRow(message)
			) {
				store.unreadByRoom[key] = (store.unreadByRoom[key] ?? 0) + 1;
			}
			if (key === store.activeKey) void store.refreshQueued();
			if (!info) void store.loadRooms();
		},

		ingestReceipt(receipt: Receipt) {
			if (!store.delivery[receipt.id]) {
				const own = Object.values(store.messagesByRoom)
					.flat()
					.find((m) => m.id === receipt.id && m.author.kind === 'owner');
				if (!own) return;
				tracker.begin(receipt.id, !store.originLocal(roomKey(own.room)));
			}
			tracker.receipt(receipt.id, receipt.state, receipt.reason);
		},

		async retry(id: string) {
			const d = store.delivery[id];
			if (!d || d.retried) return;
			for (const [key, list] of Object.entries(store.messagesByRoom)) {
				const m = list.find((x) => x.id === id);
				if (!m || m.mention) continue;
				await deliver({
					key,
					room: m.room,
					text: m.body_md,
					body: m.body_md,
					attachments: [...(m.attachments ?? [])],
					replyTo: m.reply_to,
					fromDraft: false,
					retryOf: id,
					reuseId: unsent.has(id),
					tsMs: m.ts_ms,
				});
				return;
			}
		},

		ingestActivity(event: ActivityEvent) {
			const key = roomKey(event.room);
			const cap = (text: string) => text.slice(0, MAX_LABEL);
			const used = event.used.slice(0, MAX_TOOLS).map(cap);
			const calls = capCalls(event.calls);
			dropActivity(key);
			if (used.length) lastUsed.set(key, { used, calls, at: Date.now() });
			else if (event.state === 'idle') lastUsed.delete(key);
			if (event.state === 'working') {
				store.activityByRoom[key] = {
					state: event.state,
					current: event.current === null ? null : cap(event.current),
					used,
					calls,
				};
			}
			if (event.state === 'working' || used.length) {
				activityTimers.set(
					key,
					setTimeout(() => {
						lastUsed.delete(key);
						dropActivity(key);
					}, ACTIVITY_TTL_MS),
				);
			}
		},

		async refreshQueued() {
			const seq = ++queueSeq;
			const key = store.activeKey;
			const room = key && store.roomRef(key);
			if (!key || !room) {
				store.queuedIds = [];
				store.queuedWhy = {};
				syncQueuePoll();
				return;
			}
			const ids = await api.queued(room).catch(() => null);
			if (seq !== queueSeq || store.activeKey !== key) return;
			if (ids) {
				const why = ids.length
					? await api.queuedWhy?.(room).catch(() => ({}))
					: {};
				if (seq !== queueSeq || store.activeKey !== key) return;
				store.queuedIds = ids;
				store.queuedWhy = why ?? {};
			}
			syncQueuePoll();
		},

		async cancelQueued(id: string) {
			const key = store.activeKey;
			const room = key && store.roomRef(key);
			if (!room) return;
			try {
				await api.cancel(room, id);
				store.error = null;
			} catch (e) {
				store.error = errorText(e);
			}
			await store.refreshQueued();
		},

		stopQueuePoll,

		async loadRooms() {
			const rooms = await api.rooms();
			const live = new Set(rooms.map((r) => roomKey(r.room)));
			for (const key of new Set([
				...store.rooms.map((r) => roomKey(r.room)),
				...toolRoom.values(),
				...Object.keys(store.activityByRoom),
			])) {
				if (!live.has(key)) forgetRoom(key);
			}
			store.rooms = [...rooms].sort(
				(a, b) =>
					Number(b.room.kind === 'manager') - Number(a.room.kind === 'manager'),
			);
		},

		watchRooms(intervalMs: number): () => void {
			const timer = setInterval(() => {
				void store.loadRooms().catch(() => undefined);
			}, intervalMs);
			return () => clearInterval(timer);
		},

		async init() {
			await store.loadRooms();
			await store.loadCaps();
			await api.onMessage((message) =>
				store.ingest(
					message.author.kind === 'owner' || message.user_turn
						? message
						: { ...message, fresh: true },
				),
			);
			await api.onActivity((event) => store.ingestActivity(event));
			await api.onReceipt?.((receipt) => store.ingestReceipt(receipt));
			const first = store.rooms[0];
			if (first) await store.select(roomKey(first.room));
		},

		async loadCaps() {
			try {
				store.mention.supported = mentionSupported((await api.caps?.()) ?? []);
			} catch {
				store.mention.supported = false;
			}
			if (!store.mention.supported) store.closeMention();
		},

		async refreshActive() {
			await store.loadRooms().catch(() => undefined);
			await store.loadCaps();
			const key = store.activeKey;
			const room = key && store.roomRef(key);
			if (!key || !room) return;
			try {
				const page = await api.history({ room, limit: PAGE });
				store.messagesByRoom[key] = merge(
					store.messagesByRoom[key] ?? [],
					page,
				);
			} catch (e) {
				store.error = errorText(e);
			}
		},

		async select(key: string) {
			const room = store.roomRef(key);
			if (!room) return;
			store.activeKey = key;
			store.slash.picked = null;
			store.mention.picked = null;
			store.closeMention();
			if (store.control.key !== key) {
				store.control.open = null;
				store.control.error = '';
				store.control.stillRunning = false;
			}
			store.queuedIds = [];
			store.queuedWhy = {};
			postSendPolls = 0;
			stopQueuePoll();
			void store.refreshQueued();
			store.replyTo = null;
			store.draft.attachments = [];
			store.closeSlash();
			store.unreadByRoom[key] = 0;
			try {
				const page = await api.history({ room, limit: PAGE });
				store.messagesByRoom[key] = merge(
					store.messagesByRoom[key] ?? [],
					page,
				);
				store.unreadByRoom[key] = 0;
			} catch (e) {
				store.error = errorText(e);
			}
		},

		async loadOlder(): Promise<boolean | null> {
			const key = store.activeKey;
			const room = key && store.roomRef(key);
			const first = key && store.messagesByRoom[key]?.[0];
			if (!key || !room || !first) return false;
			try {
				const page = await api.history({
					room,
					before: first.id,
					limit: PAGE,
				});
				const next = merge(store.messagesByRoom[key] ?? [], page);
				const grew = next.length > (store.messagesByRoom[key]?.length ?? 0);
				store.messagesByRoom[key] = next;
				return grew;
			} catch (e) {
				store.error = errorText(e);
				return null;
			}
		},

		setReply(message: ChatMessage | null) {
			store.replyTo = message;
		},

		addAttachment(file: FileLike) {
			if (store.draft.attachments.length >= MAX_ATTACHMENTS) {
				store.error = `At most ${MAX_ATTACHMENTS} attachments per message`;
				return;
			}
			if (file.size > MAX_FILE_BYTES) {
				store.error = `${file.name} is larger than 25 MiB`;
				return;
			}
			store.error = null;
			store.draft.attachments.push({
				name: file.name,
				mime: file.type,
				size: file.size,
				sha256: file.sha256,
			});
		},

		removeAttachment(index: number) {
			store.draft.attachments.splice(index, 1);
		},

		async send() {
			const key = store.activeKey;
			const room = key && store.roomRef(key);
			const text = store.draft.text;
			if (!key || !room) return;
			if (!text.trim() && !store.draft.attachments.length) return;
			let body = text;
			let mention: MentionReq | undefined;
			const picked = store.mention.picked;
			const rest = picked ? stripMention(text, picked.handle) : null;
			if (picked && rest !== null) {
				if (!store.mention.supported || !store.originLocal(key)) {
					store.error = 'Mentions are not available in this chat';
					return;
				}
				if (!picked.ok) {
					store.error = picked.reason ?? 'This chat cannot be mentioned';
					return;
				}
				if (rest === '') {
					store.error = 'Write a question after the @name';
					return;
				}
				body = rest;
				mention = { target: picked.room, confirmed_risky: picked.risky };
				if (picked.risky) {
					store.mention.confirm = {
						kind: 'send',
						key,
						room,
						text,
						body,
						target: picked,
						attachments: [...store.draft.attachments],
						replyTo: store.replyTo?.id,
					};
					return;
				}
			}
			await deliver({
				key,
				room,
				text,
				body,
				mention,
				attachments: [...store.draft.attachments],
				replyTo: store.replyTo?.id,
			});
		},

		originLocal(key: string | null): boolean {
			const info = store.rooms.find((r) => roomKey(r.room) === key);
			return !!info && info.local !== false;
		},

		closeMention() {
			store.mention.open = false;
			store.mention.items = [];
			store.mention.index = 0;
		},

		updateMention(text: string, key: string): boolean {
			const query = mentionQuery(text);
			const picked = store.mention.picked;
			if (
				query === null ||
				!store.mention.supported ||
				!store.originLocal(key) ||
				(picked && stripMention(text, picked.handle) !== null)
			) {
				store.closeMention();
				return false;
			}
			const items = mentionCandidates(store.rooms, key, query);
			store.mention.items = items;
			store.mention.index = firstEnabled(items);
			store.mention.open = items.length > 0;
			return true;
		},

		moveMention(step: 1 | -1) {
			const m = store.mention;
			if (!m.items.length) return;
			m.index = nextEnabled(m.items, m.index, step);
		},

		pickMention(
			item: MentionItem | undefined = store.mention.items[store.mention.index],
		) {
			if (!item || !item.ok) return;
			store.draft.text = mentionHandleText(item);
			store.mention.picked = item;
			store.closeMention();
		},

		confirmMention() {
			const pending = store.mention.confirm;
			store.mention.confirm = null;
			if (!pending) return;
			if (pending.kind === 'inject') {
				void store.runInject(pending.message, true);
				return;
			}
			const info = store.roomInfo(pending.target.room);
			const unchanged =
				store.activeKey === pending.key &&
				store.draft.text === pending.text &&
				store.mention.picked?.key === pending.target.key &&
				store.replyTo?.id === pending.replyTo &&
				JSON.stringify(store.draft.attachments) ===
					JSON.stringify(pending.attachments) &&
				store.mention.supported &&
				store.originLocal(pending.key) &&
				info?.mention?.ok === true &&
				info.mention.risky === true;
			if (!unchanged) {
				store.error = 'The message changed. Send it again.';
				return;
			}
			void deliver({
				key: pending.key,
				room: pending.room,
				text: pending.text,
				body: pending.body,
				mention: { target: pending.target.room, confirmed_risky: true },
				attachments: pending.attachments,
				replyTo: pending.replyTo,
			});
		},

		cancelMention() {
			store.mention.confirm = null;
		},

		requestInject(message: ChatMessage) {
			const gate = injectGate(
				store.roomInfo(message.room),
				message,
				store.mention.supported,
			);
			if (!gate.show) return;
			if (gate.refused) {
				store.error = 'Not available for unrestricted sessions yet';
				return;
			}
			if (gate.confirm) {
				store.mention.confirm = { kind: 'inject', message };
				return;
			}
			void store.runInject(message, false);
		},

		async runInject(message: ChatMessage, confirmed: boolean) {
			const id = message.mention?.id;
			const gate = injectGate(
				store.roomInfo(message.room),
				message,
				store.mention.supported,
			);
			if (!id || !api.mentionInject || !gate.show || gate.refused) return;
			if (store.mention.injecting[id]) return;
			store.mention.injecting[id] = true;
			try {
				await api.mentionInject(message.room, id, confirmed);
				store.mention.injected[id] = true;
				store.error = null;
			} catch (e) {
				store.error = errorText(e);
			} finally {
				delete store.mention.injecting[id];
			}
		},

		roomInfo(room: RoomRef): RoomInfo | undefined {
			const key = roomKey(room);
			return store.rooms.find((r) => roomKey(r.room) === key);
		},

		closeSlash() {
			store.slash.open = false;
			store.slash.query = '';
			store.slash.items = [];
			store.slash.index = 0;
		},

		async onInput(text: string) {
			store.draft.text = text;
			const key = store.activeKey;
			const room = key && store.roomRef(key);
			const picked = store.mention.picked;
			if (picked && stripMention(text, picked.handle) === null) {
				store.mention.picked = null;
			}
			if (key && room && store.updateMention(text, key)) {
				store.closeSlash();
				return;
			}
			if (!key || !room || !/^\/\S*$/.test(text)) {
				store.closeSlash();
				return;
			}
			let commands = commandCache.get(key);
			if (!commands) {
				try {
					commands = await api.commands(room);
					commandCache.set(key, commands);
				} catch {
					store.closeSlash();
					return;
				}
			}
			if (store.draft.text !== text || store.activeKey !== key) return;
			const query = text.slice(1);
			const items = commands.filter((c) => slashName(c).startsWith(query));
			store.slash.query = query;
			store.slash.items = items;
			store.slash.index = 0;
			store.slash.open = items.length > 0;
		},

		moveSlash(step: 1 | -1) {
			const n = store.slash.items.length;
			if (!n) return;
			store.slash.index = (store.slash.index + step + n) % n;
		},

		pickSlash() {
			const command = store.slash.items[store.slash.index];
			if (!command) return;
			store.draft.text = `/${slashName(command)} `;
			store.closeSlash();
			store.slash.picked = command;
		},

		openSearch(scopeKey?: string) {
			store.search.open = true;
			store.search.scopeKey = scopeKey ?? null;
		},

		closeSearch() {
			searchSeq++;
			store.search.open = false;
			store.search.text = '';
			store.search.scopeKey = null;
			store.search.hits = [];
		},

		async runSearch(text: string) {
			const seq = ++searchSeq;
			store.search.text = text;
			if (!text.trim()) {
				store.search.hits = [];
				return;
			}
			const scope = store.search.scopeKey;
			const room = scope ? store.roomRef(scope) : null;
			try {
				const hits = await api.search(
					room ? { query: text, room } : { query: text },
				);
				if (seq === searchSeq) store.search.hits = hits;
			} catch (e) {
				if (seq === searchSeq) store.error = errorText(e);
			}
		},

		async setTab(tab: PanelTab) {
			store.panel.tab = tab;
			if (tab === 'audit' && !store.panel.auditLoaded) await store.loadAudit();
		},

		async loadAudit() {
			try {
				store.panel.audit = await api.audit();
				store.panel.auditLoaded = true;
				store.panel.auditError = '';
			} catch (e) {
				store.panel.auditError = errorText(e);
			}
		},

		async selectFromHierarchy(key: string) {
			store.panel.tab = 'session';
			await store.select(key);
		},

		openNewSession() {
			store.newSession.form = {
				...emptyForm(),
				machine: store.machines[0] ?? '',
			};
			store.newSession.error = '';
			store.newSession.open = true;
		},

		closeNewSession() {
			store.newSession.open = false;
			store.newSession.error = '';
		},

		async submitNewSession() {
			const form = store.newSession;
			if (form.busy || !form.form.machine || !form.form.folder.trim()) return;
			form.busy = true;
			form.error = '';
			try {
				await api.spawn({ ...form.form, folder: form.form.folder.trim() });
				store.closeNewSession();
			} catch (e) {
				form.error = errorText(e);
			} finally {
				form.busy = false;
			}
		},

		openSwitch() {
			const info = store.rooms.find((r) => roomKey(r.room) === store.activeKey);
			if (!info || info.room.kind === 'manager') return;
			const known = info.agent as SpawnAgent | undefined;
			const agent = known && known in AGENT_MODELS ? known : 'claude';
			store.switcher.agent = agent;
			store.switcher.model = store.switchModels[0];
			store.switcher.error = '';
			store.switcher.open = true;
			modelTouched = false;
			void loadModels(agent);
		},

		pickSwitchAgent(agent: SpawnAgent) {
			store.switcher.agent = agent;
			store.switcher.model = store.switchModels[0];
			modelTouched = false;
			void loadModels(agent);
		},

		pickSwitchModel(model: string) {
			store.switcher.model = model;
			modelTouched = true;
		},

		closeSwitch() {
			store.switcher.open = false;
			store.switcher.error = '';
		},

		async submitSwitch() {
			const sw = store.switcher;
			const key = store.activeKey;
			if (sw.busy || !key) return;
			sw.busy = true;
			sw.error = '';
			try {
				await api.switchAgent(key, sw.agent, sw.model);
				store.closeSwitch();
			} catch (e) {
				sw.error = errorText(e);
			} finally {
				sw.busy = false;
			}
		},

		controllable(key: string | null): RoomInfo | null {
			const info = store.rooms.find((r) => roomKey(r.room) === key);
			return info && info.room.kind === 'session' && info.local !== false
				? info
				: null;
		},

		openControl(mode: 'force' | 'delete') {
			const info = store.controllable(store.activeKey);
			if (!info || store.control.busy) return;
			if ((info.state === 'exited') !== (mode === 'delete')) return;
			store.control.key = roomKey(info.room);
			store.control.error = '';
			store.control.stillRunning = false;
			store.control.open = mode;
		},

		closeControl() {
			if (store.control.busy) return;
			store.control.open = null;
			store.control.error = '';
		},

		async runControl(
			key: string | null,
			wantExited: boolean,
			reportStill: boolean,
			act: (room: RoomRef) => Promise<boolean>,
		) {
			const info = store.controllable(key);
			const ctl = store.control;
			if (!key || !info || ctl.busy) return;
			if ((info.state === 'exited') !== wantExited) return;
			ctl.key = key;
			ctl.busy = true;
			ctl.error = '';
			ctl.stillRunning = false;
			const current = () => ctl.key === key && store.activeKey === key;
			try {
				const done = await act(info.room);
				if (current()) {
					if (done) ctl.open = null;
					else if (reportStill) ctl.stillRunning = true;
				}
			} catch (e) {
				if (current()) ctl.error = errorText(e);
			} finally {
				ctl.busy = false;
			}
			await store.loadRooms().catch(() => undefined);
		},

		async stopActive() {
			await store.runControl(store.activeKey, false, true, async (room) => {
				return (await api.stop(room, false)).exited;
			});
		},

		async forceStopActive() {
			if (store.control.open !== 'force') return;
			await store.runControl(store.control.key, false, true, async (room) => {
				return (await api.stop(room, true)).exited;
			});
		},

		async deleteActive() {
			if (store.control.open !== 'delete') return;
			await store.runControl(store.control.key, true, false, async (room) => {
				const key = roomKey(room);
				await api.remove(room);
				store.rooms = store.rooms.filter((r) => roomKey(r.room) !== key);
				delete store.messagesByRoom[key];
				store.shownRooms.delete(key);
				if (store.activeKey === key) {
					const first = store.rooms[0];
					store.activeKey = null;
					if (first) await store.select(roomKey(first.room));
				}
				return true;
			});
		},

		async jumpToHit(hit: ChatMessage): Promise<boolean> {
			const key = roomKey(hit.room);
			store.closeSearch();
			if (key !== store.activeKey) await store.select(key);
			for (let i = 0; i <= MAX_JUMP_PAGES; i++) {
				if (store.messagesByRoom[key]?.some((m) => m.id === hit.id)) {
					store.focusId = hit.id;
					return true;
				}
				if (!(await store.loadOlder())) break;
			}
			return false;
		},
	});

	return store;
};

export type ManagerStore = ReturnType<typeof createManagerStore>;

let singleton: ManagerStore | null = null;

export const useManagerStore = (): ManagerStore => {
	singleton ??= createManagerStore({
		rooms: realApi.managerRooms,
		history: realApi.managerHistory,
		send: realApi.managerSend,
		commands: realApi.managerCommands,
		search: realApi.managerSearch,
		queued: realApi.managerQueued,
		queuedWhy: realApi.managerQueuedWhy,
		cancel: realApi.managerCancel,
		agentModels: realApi.managerAgentModels,
		onMessage: realApi.onManagerMessage,
		onActivity: realApi.onManagerActivity,
		onReceipt: realApi.onManagerReceipt,
		audit: realApi.managerAudit,
		spawn: realApi.managerSpawnSession,
		switchAgent: realApi.managerSwitchAgent,
		stop: realApi.managerStop,
		remove: realApi.managerRemove,
		caps: realApi.runnerCaps,
		mentionInject: realApi.managerMentionInject,
	});
	return singleton;
};
