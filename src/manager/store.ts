import { computed, reactive } from 'vue';
import * as realApi from './api';
import {
	MAX_ATTACHMENTS,
	MAX_BODY_BYTES,
	roomKey,
	type Attachment,
	type AuditEntry,
	type ChatMessage,
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
	onMessage: typeof realApi.onManagerMessage;
	audit: typeof realApi.managerAudit;
	spawn: typeof realApi.managerSpawnSession;
	switchAgent: typeof realApi.managerSwitchAgent;
}

export type PanelTab = 'session' | 'hierarchy' | 'audit' | 'encryption';

export const AGENT_MODELS: Record<SpawnAgent, string[]> = {
	claude: ['opus', 'sonnet', 'haiku'],
	codex: ['gpt-5', 'gpt-5-mini'],
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

	const unread = reactive({} as Record<string, number>);

	const store = reactive({
		rooms: [] as RoomInfo[],
		activeKey: null as string | null,
		messagesByRoom: {} as Record<string, ChatMessage[]>,
		unreadByRoom: unread,
		unreadTotal: computed(() =>
			Object.values(unread).reduce((a, b) => a + b, 0),
		),
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
		switcher: {
			open: false,
			busy: false,
			error: '',
			agent: 'claude' as SpawnAgent,
			model: AGENT_MODELS.claude[0],
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
			const info = store.rooms.find((r) => roomKey(r.room) === key);
			if (info && (!info.last || byTime(info.last, message) <= 0)) {
				info.last = message;
			}
			if (key !== store.activeKey && message.author.kind !== 'owner') {
				store.unreadByRoom[key] = (store.unreadByRoom[key] ?? 0) + 1;
			}
			if (!info) void store.loadRooms();
		},

		async loadRooms() {
			const rooms = await api.rooms();
			store.rooms = [...rooms].sort(
				(a, b) =>
					Number(b.room.kind === 'manager') - Number(a.room.kind === 'manager'),
			);
		},

		async init() {
			await store.loadRooms();
			await api.onMessage((message) => store.ingest(message));
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

		async loadOlder(): Promise<boolean> {
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
				return false;
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
			store.switcher.model = AGENT_MODELS[agent][0];
			store.switcher.error = '';
			store.switcher.open = true;
		},

		pickSwitchAgent(agent: SpawnAgent) {
			store.switcher.agent = agent;
			store.switcher.model = AGENT_MODELS[agent][0];
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
		onMessage: realApi.onManagerMessage,
		audit: realApi.managerAudit,
		spawn: realApi.managerSpawnSession,
		switchAgent: realApi.managerSwitchAgent,
	});
	return singleton;
};
