import { computed, reactive } from 'vue';
import * as realApi from './api';
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
	type ModelChoice,
	type RoomInfo,
	type RoomRef,
	type SlashCommand,
	type SpawnAgent,
	type SpawnRequest,
} from './types';

export interface ManagerApi {
	rooms: typeof realApi.managerRooms;
	history: typeof realApi.managerHistory;
	send: typeof realApi.managerSend;
	commands: typeof realApi.managerCommands;
	search: typeof realApi.managerSearch;
	queued: typeof realApi.managerQueued;
	cancel: typeof realApi.managerCancel;
	agentModels: typeof realApi.managerAgentModels;
	onMessage: typeof realApi.onManagerMessage;
	onActivity: typeof realApi.onManagerActivity;
	audit: typeof realApi.managerAudit;
	spawn: typeof realApi.managerSpawnSession;
	switchAgent: typeof realApi.managerSwitchAgent;
	stop: typeof realApi.managerStop;
	remove: typeof realApi.managerRemove;
}

export type PanelTab = 'session' | 'hierarchy' | 'audit' | 'encryption';

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
const MAX_TOOL_MESSAGES = 200;

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
	const lastUsed = new Map<string, { used: string[]; at: number }>();

	const toolOrder: string[] = [];
	const toolRoom = new Map<string, string>();

	const dropTools = (id: string) => {
		delete store.toolsByMessage[id];
		toolRoom.delete(id);
	};

	const rememberTools = (id: string, key: string, used: string[]) => {
		store.toolsByMessage[id] = used;
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
		activityByRoom: {} as Record<string, ActivityView>,
		toolsByMessage: {} as Record<string, string[]>,
		replyTo: null as ChatMessage | null,
		draft: { text: '', attachments: [] as Attachment[] },
		slash: {
			open: false,
			query: '',
			items: [] as SlashCommand[],
			index: 0,
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
				message.kind === 'text' &&
				!message.user_turn &&
				message.author.kind === message.room.kind
			) {
				const seen = lastUsed.get(key);
				if (seen && Date.now() - seen.at <= ACTIVITY_TTL_MS) {
					rememberTools(message.id, key, seen.used);
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
				!message.user_turn
			) {
				store.unreadByRoom[key] = (store.unreadByRoom[key] ?? 0) + 1;
			}
			if (key === store.activeKey) void store.refreshQueued();
			if (!info) void store.loadRooms();
		},

		ingestActivity(event: ActivityEvent) {
			const key = roomKey(event.room);
			const cap = (text: string) => text.slice(0, MAX_LABEL);
			const used = event.used.slice(0, MAX_TOOLS).map(cap);
			dropActivity(key);
			if (used.length) lastUsed.set(key, { used, at: Date.now() });
			else if (event.state === 'idle') lastUsed.delete(key);
			if (event.state === 'working') {
				store.activityByRoom[key] = {
					state: event.state,
					current: event.current === null ? null : cap(event.current),
					used,
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
				syncQueuePoll();
				return;
			}
			const ids = await api.queued(room).catch(() => null);
			if (seq !== queueSeq || store.activeKey !== key) return;
			if (ids) store.queuedIds = ids;
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
			await api.onMessage((message) =>
				store.ingest(
					message.author.kind === 'owner' || message.user_turn
						? message
						: { ...message, fresh: true },
				),
			);
			await api.onActivity((event) => store.ingestActivity(event));
			const first = store.rooms[0];
			if (first) await store.select(roomKey(first.room));
		},

		async refreshActive() {
			await store.loadRooms().catch(() => undefined);
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
			if (store.control.key !== key) {
				store.control.open = null;
				store.control.error = '';
				store.control.stillRunning = false;
			}
			store.queuedIds = [];
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
			if (new TextEncoder().encode(text).length > MAX_BODY_BYTES) {
				store.error = 'Message is larger than 64 KiB';
				return;
			}
			try {
				const sent = await api.send({
					room,
					body_md: text,
					reply_to: store.replyTo?.id,
					attachments: [...store.draft.attachments],
				});
				if (store.activeKey === key && store.draft.text === text) {
					store.draft.text = '';
					store.draft.attachments = [];
					store.replyTo = null;
					store.closeSlash();
				}
				store.error = null;
				store.ingest(sent);
				postSendPolls = POST_SEND_POLLS;
				void store.refreshQueued();
			} catch (e) {
				store.error = errorText(e);
			}
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
		cancel: realApi.managerCancel,
		agentModels: realApi.managerAgentModels,
		onMessage: realApi.onManagerMessage,
		onActivity: realApi.onManagerActivity,
		audit: realApi.managerAudit,
		spawn: realApi.managerSpawnSession,
		switchAgent: realApi.managerSwitchAgent,
		stop: realApi.managerStop,
		remove: realApi.managerRemove,
	});
	return singleton;
};
