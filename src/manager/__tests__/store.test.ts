import type { Receipt } from '../delivery';
import { createManagerStore, type ManagerApi } from '../store';
import {
	roomKey,
	type ActivityEvent,
	type ChatMessage,
	type RoomInfo,
	type RoomRef,
	type SlashCommand,
} from '../types';

const MGR: RoomRef = { kind: 'manager', machine: 'mac' };
const S1: RoomRef = { kind: 'session', machine: 'mac', session: 's1' };
const MGR_KEY = roomKey(MGR);
const S1_KEY = roomKey(S1);

const info = (room: RoomRef): RoomInfo => ({ room, title: roomKey(room) });

const msg = (
	n: number,
	room: RoomRef = MGR,
	authorKind: 'owner' | 'manager' | 'session' = 'manager',
): ChatMessage => ({
	v: 1,
	id: `m${String(n).padStart(3, '0')}`,
	room,
	author: { kind: authorKind, id: 'a' },
	ts_ms: n,
	kind: 'text',
	body_md: `body ${n}`,
});

const range = (from: number, to: number, room: RoomRef = MGR) =>
	Array.from({ length: to - from + 1 }, (_, i) => msg(from + i, room));

const commands: SlashCommand[] = [
	{ name: 'status', description: 's', group: 'Info', dangerous: false },
	{ name: 'stop', description: 'x', group: 'Control', dangerous: true },
	{ name: 'help', description: 'h', group: 'Info', dangerous: false },
];

const setup = (over: Partial<ManagerApi> = {}) => {
	let push: (m: ChatMessage) => void = () => {};
	let pushActivity: (e: ActivityEvent) => void = () => {};
	const api: ManagerApi = {
		rooms: jest.fn(async () => [info(S1), info(MGR)]),
		history: jest.fn(async ({ room }) => range(1, 3, room)),
		send: jest.fn(async ({ room }) => msg(99, room, 'owner')),
		commands: jest.fn(async () => commands),
		search: jest.fn(async () => []),
		queued: jest.fn(async () => []),
		cancel: jest.fn(async () => true),
		agentModels: jest.fn(async () => {
			throw new Error('no list');
		}),
		audit: jest.fn(async () => []),
		spawn: jest.fn(async () => undefined),
		switchAgent: jest.fn(async () => undefined),
		stop: jest.fn(async () => ({ exited: true })),
		remove: jest.fn(async () => undefined),
		onMessage: jest.fn(async (h) => {
			push = h;
			return () => {};
		}),
		onActivity: jest.fn(async (h) => {
			pushActivity = h;
			return () => {};
		}),
		...over,
	};
	const store = createManagerStore(api);
	return {
		api,
		store,
		emit: (m: ChatMessage) => push(m),
		emitActivity: (e: ActivityEvent) => pushActivity(e),
	};
};

const ids = (list: ChatMessage[]) => list.map((m) => m.id);

describe('refreshActive', () => {
	it('refetches rooms and the active history, merging new messages', async () => {
		const { api, store } = setup();
		await store.init();
		(api.history as jest.Mock).mockResolvedValueOnce(range(1, 5));
		await store.refreshActive();
		expect(api.rooms).toHaveBeenCalledTimes(2);
		expect(ids(store.messagesByRoom[MGR_KEY])).toEqual(ids(range(1, 5)));
	});

	it('does nothing for history without an active room', async () => {
		const { api, store } = setup({ rooms: jest.fn(async () => []) });
		await store.init();
		await store.refreshActive();
		expect(api.history).not.toHaveBeenCalled();
	});
});

describe('init and select', () => {
	it('loads rooms manager-first, selects it and subscribes', async () => {
		const { api, store } = setup();
		await store.init();
		expect(store.rooms.map((r) => roomKey(r.room))).toEqual([MGR_KEY, S1_KEY]);
		expect(store.activeKey).toBe(MGR_KEY);
		expect(store.messagesByRoom[MGR_KEY]).toHaveLength(3);
		expect(api.history).toHaveBeenCalledWith({ room: MGR, limit: 50 });
		expect(api.onMessage).toHaveBeenCalled();
	});

	it('clears reply and attachments on select, keeps text', async () => {
		const { store } = setup();
		await store.init();
		store.setReply(msg(1));
		store.addAttachment({ name: 'a', type: 'x/y', size: 1, sha256: 'h' });
		store.draft.text = 'keep';
		await store.select(S1_KEY);
		expect(store.replyTo).toBeNull();
		expect(store.draft.attachments).toEqual([]);
		expect(store.draft.text).toBe('keep');
	});

	it('ignores unknown room keys', async () => {
		const { store } = setup();
		await store.init();
		await store.select('session:nope:x');
		expect(store.activeKey).toBe(MGR_KEY);
	});
});

describe('incoming messages', () => {
	it('appends to the active room, deduped and ordered', async () => {
		const { store, emit } = setup();
		await store.init();
		emit(msg(5));
		emit(msg(5));
		emit(msg(4));
		expect(ids(store.messagesByRoom[MGR_KEY])).toEqual([
			'm001',
			'm002',
			'm003',
			'm004',
			'm005',
		]);
		expect(store.unreadTotal).toBe(0);
	});

	it('counts unread for other rooms and clears on select', async () => {
		const { store, emit } = setup();
		await store.init();
		emit(msg(10, S1));
		emit(msg(11, S1));
		emit(msg(12, S1, 'owner'));
		expect(store.unreadByRoom[S1_KEY]).toBe(2);
		expect(store.unreadTotal).toBe(2);
		await store.select(S1_KEY);
		expect(store.unreadTotal).toBe(0);
	});

	it('does not double count a message already loaded by select', async () => {
		const { store, emit } = setup({
			history: jest.fn(async ({ room }) => range(1, 3, room)),
		});
		await store.init();
		emit(msg(3, S1));
		await store.select(S1_KEY);
		emit(msg(3, S1));
		expect(store.unreadTotal).toBe(0);
	});

	it('updates the room preview', async () => {
		const { store, emit } = setup();
		await store.init();
		emit(msg(7));
		expect(store.rooms[0].last?.id).toBe('m007');
	});

	it('reloads rooms when a message arrives for an unknown room', async () => {
		const other: RoomRef = { kind: 'session', machine: 'mac', session: 'new' };
		const rooms = jest
			.fn()
			.mockResolvedValueOnce([info(MGR)])
			.mockResolvedValue([info(MGR), info(other)]);
		const { store, emit } = setup({ rooms });
		await store.init();
		emit(msg(1, other));
		await new Promise((r) => setTimeout(r, 0));
		expect(store.rooms).toHaveLength(2);
	});
});

describe('loadOlder', () => {
	it('uses the first message id as cursor and prepends', async () => {
		const history = jest.fn(async ({ before }) =>
			before ? range(1, 2) : range(3, 5),
		);
		const { store } = setup({ history });
		await store.init();
		expect(await store.loadOlder()).toBe(true);
		expect(history).toHaveBeenLastCalledWith({
			room: MGR,
			before: 'm003',
			limit: 50,
		});
		expect(ids(store.messagesByRoom[MGR_KEY])).toEqual([
			'm001',
			'm002',
			'm003',
			'm004',
			'm005',
		]);
	});

	it('returns false on an empty page', async () => {
		const history = jest.fn(async ({ before }) => (before ? [] : range(1, 2)));
		const { store } = setup({ history });
		await store.init();
		expect(await store.loadOlder()).toBe(false);
	});

	it('returns null and sets the error when the page request fails', async () => {
		const history = jest.fn(async ({ before }) => {
			if (before) throw new Error('offline');
			return range(1, 2);
		});
		const { store } = setup({ history });
		await store.init();
		expect(await store.loadOlder()).toBeNull();
		expect(store.error).toBe('offline');
		expect(ids(store.messagesByRoom[MGR_KEY])).toEqual(['m001', 'm002']);
	});
});

describe('send', () => {
	it('builds params, clears the composer and appends the result', async () => {
		const { api, store } = setup();
		await store.init();
		store.setReply(msg(2));
		store.addAttachment({
			name: 'a.txt',
			type: 'text/plain',
			size: 5,
			sha256: 'h',
		});
		store.draft.text = 'hello';
		await store.send();
		expect(api.send).toHaveBeenCalledWith({
			id: expect.stringMatching(/^m_[0-9a-f]{32}$/),
			ts_ms: expect.any(Number),
			room: MGR,
			body_md: 'hello',
			reply_to: 'm002',
			attachments: [
				{ name: 'a.txt', mime: 'text/plain', size: 5, sha256: 'h' },
			],
		});
		expect(store.draft.text).toBe('');
		expect(store.draft.attachments).toEqual([]);
		expect(store.replyTo).toBeNull();
		expect(store.messagesByRoom[MGR_KEY].slice(-1)[0]?.id).toBe('m099');
	});

	it('ignores empty drafts', async () => {
		const { api, store } = setup();
		await store.init();
		store.draft.text = '   ';
		await store.send();
		expect(api.send).not.toHaveBeenCalled();
	});

	it('rejects bodies over 64 KiB', async () => {
		const { api, store } = setup();
		await store.init();
		store.draft.text = 'x'.repeat(64 * 1024 + 1);
		await store.send();
		expect(api.send).not.toHaveBeenCalled();
		expect(store.error).toMatch(/64 KiB/);
	});

	it('does not duplicate a message already pushed by an event', async () => {
		const { store, emit } = setup();
		await store.init();
		emit(msg(99, MGR, 'owner'));
		store.draft.text = 'x';
		await store.send();
		expect(
			store.messagesByRoom[MGR_KEY].filter((m) => m.id === 'm099'),
		).toHaveLength(1);
	});

	it('keeps the composer and sets error when sending fails', async () => {
		const { store } = setup({
			send: jest.fn(async () => {
				throw new Error('offline');
			}),
		});
		await store.init();
		store.draft.text = 'hello';
		await store.send();
		expect(store.draft.text).toBe('hello');
		expect(store.error).toBe('offline');
	});
});

describe('attachments', () => {
	const file = (name: string, size = 1) => ({
		name,
		type: 'x/y',
		size,
		sha256: name,
	});

	it('adds and removes by index', () => {
		const { store } = setup();
		store.addAttachment(file('a'));
		store.addAttachment(file('b'));
		store.removeAttachment(0);
		expect(store.draft.attachments.map((a) => a.name)).toEqual(['b']);
	});

	it('rejects files over 25 MiB but accepts exactly 25 MiB', () => {
		const { store } = setup();
		store.addAttachment(file('big', 25 * 1024 * 1024 + 1));
		expect(store.draft.attachments).toEqual([]);
		expect(store.error).toMatch(/25 MiB/);
		store.addAttachment(file('edge', 25 * 1024 * 1024));
		expect(store.draft.attachments).toHaveLength(1);
	});

	it('rejects the 17th attachment', () => {
		const { store } = setup();
		for (let i = 0; i < 16; i++) store.addAttachment(file(`f${i}`));
		expect(store.draft.attachments).toHaveLength(16);
		store.addAttachment(file('extra'));
		expect(store.draft.attachments).toHaveLength(16);
		expect(store.error).toMatch(/16/);
	});
});

describe('slash menu', () => {
	it('opens on a leading slash and lists commands', async () => {
		const { store } = setup();
		await store.init();
		await store.onInput('/');
		expect(store.slash.open).toBe(true);
		expect(store.slash.items.map((c) => c.name)).toEqual([
			'status',
			'stop',
			'help',
		]);
	});

	it('filters by prefix keeping order', async () => {
		const { store } = setup();
		await store.init();
		await store.onInput('/st');
		expect(store.slash.items.map((c) => c.name)).toEqual(['status', 'stop']);
		await store.onInput('/sta');
		expect(store.slash.items.map((c) => c.name)).toEqual(['status']);
	});

	it('closes on a space, plain text or no match', async () => {
		const { store } = setup();
		await store.init();
		await store.onInput('/st');
		await store.onInput('/status now');
		expect(store.slash.open).toBe(false);
		await store.onInput('/st');
		await store.onInput('hello');
		expect(store.slash.open).toBe(false);
		await store.onInput('/zzz');
		expect(store.slash.open).toBe(false);
	});

	it('fetches commands once per room', async () => {
		const { api, store } = setup();
		await store.init();
		await store.onInput('/');
		await store.onInput('/s');
		expect(api.commands).toHaveBeenCalledTimes(1);
		await store.select(S1_KEY);
		await store.onInput('/');
		expect(api.commands).toHaveBeenCalledTimes(2);
		expect(api.commands).toHaveBeenLastCalledWith(S1);
	});

	it('moves with wraparound and picks', async () => {
		const { store } = setup();
		await store.init();
		await store.onInput('/');
		store.moveSlash(-1);
		expect(store.slash.index).toBe(2);
		store.moveSlash(1);
		store.moveSlash(1);
		store.pickSlash();
		expect(store.draft.text).toBe('/stop ');
		expect(store.slash.open).toBe(false);
	});

	it('accepts command names with a leading slash', async () => {
		const { store } = setup({
			commands: jest.fn(async () => [
				{ name: '/deploy', description: '', group: 'g', dangerous: false },
			]),
		});
		await store.init();
		await store.onInput('/de');
		store.pickSlash();
		expect(store.draft.text).toBe('/deploy ');
	});

	it('survives a command fetch failure', async () => {
		const { store } = setup({
			commands: jest.fn(async () => {
				throw new Error('nope');
			}),
		});
		await store.init();
		await store.onInput('/');
		expect(store.slash.open).toBe(false);
		expect(store.draft.text).toBe('/');
	});
});

describe('search', () => {
	it('searches globally and by scope', async () => {
		const hits = [msg(2, S1)];
		const search = jest.fn(async () => hits);
		const { store } = setup({ search });
		await store.init();
		store.openSearch();
		await store.runSearch('foo');
		expect(search).toHaveBeenLastCalledWith({ query: 'foo' });
		expect(store.search.hits).toEqual(hits);
		store.openSearch(S1_KEY);
		await store.runSearch('foo');
		expect(search).toHaveBeenLastCalledWith({ query: 'foo', room: S1 });
	});

	it('clears hits on empty text without calling the api', async () => {
		const search = jest.fn(async () => [msg(2, S1)]);
		const { store } = setup({ search });
		await store.init();
		await store.runSearch('foo');
		await store.runSearch('  ');
		expect(store.search.hits).toEqual([]);
		expect(search).toHaveBeenCalledTimes(1);
	});

	it('closeSearch resets state', async () => {
		const { store } = setup();
		await store.init();
		store.openSearch(S1_KEY);
		await store.runSearch('foo');
		store.closeSearch();
		expect(store.search).toMatchObject({
			open: false,
			text: '',
			scopeKey: null,
			hits: [],
		});
	});

	it('ignores stale responses', async () => {
		let resolveFirst: (v: ChatMessage[]) => void = () => {};
		const search = jest
			.fn()
			.mockImplementationOnce(() => new Promise((r) => (resolveFirst = r)))
			.mockImplementationOnce(async () => [msg(1)]);
		const { store } = setup({ search });
		await store.init();
		const first = store.runSearch('a');
		await store.runSearch('ab');
		resolveFirst([]);
		await first;
		expect(store.search.hits).toHaveLength(1);
	});

	it('jumpToHit selects the room and pages back until the message is loaded', async () => {
		const history = jest.fn(async ({ room, before }) => {
			if (roomKey(room) !== S1_KEY) return range(1, 3, room);
			if (!before) return range(51, 100, room);
			if (before === 'm051') return range(1, 50, room);
			return [];
		});
		const { store } = setup({ history });
		await store.init();
		store.openSearch();
		expect(await store.jumpToHit(msg(7, S1))).toBe(true);
		expect(store.activeKey).toBe(S1_KEY);
		expect(store.focusId).toBe('m007');
		expect(store.search.open).toBe(false);
	});

	it('jumpToHit gives up when history is exhausted', async () => {
		const history = jest.fn(async ({ room, before }) =>
			before ? [] : range(1, 3, room),
		);
		const { store } = setup({ history });
		await store.init();
		expect(await store.jumpToHit(msg(500))).toBe(false);
		expect(store.focusId).toBeNull();
	});
});

describe('detail panel and dialogs', () => {
	it('loads the audit log once when the audit tab opens', async () => {
		const { api, store } = setup();
		await store.setTab('audit');
		await store.setTab('session');
		await store.setTab('audit');
		expect(store.panel.tab).toBe('audit');
		expect(api.audit).toHaveBeenCalledTimes(1);
		expect(store.panel.audit).toEqual([]);
	});

	it('keeps the audit tab usable when loading fails', async () => {
		const { store } = setup({
			audit: jest.fn(async () => {
				throw new Error('no audit');
			}),
		});
		await store.setTab('audit');
		expect(store.panel.auditError).toBe('no audit');
		expect(store.panel.auditLoaded).toBe(false);
	});

	it('selecting from the hierarchy opens the session tab and the room', async () => {
		const { store } = setup();
		await store.init();
		await store.setTab('hierarchy');
		await store.selectFromHierarchy(S1_KEY);
		expect(store.panel.tab).toBe('session');
		expect(store.activeKey).toBe(S1_KEY);
	});

	it('new-session dialog opens with the first machine and closes', async () => {
		const { store } = setup();
		await store.init();
		store.openNewSession();
		expect(store.newSession.open).toBe(true);
		expect(store.newSession.form.machine).toBe('mac');
		store.closeNewSession();
		expect(store.newSession.open).toBe(false);
	});

	it('new-session stub error stays in the dialog and is never success', async () => {
		const spawn = jest.fn(async () => {
			throw new Error(
				'Starting sessions from the Manager window is not available yet',
			);
		});
		const { store } = setup({ spawn });
		await store.init();
		store.openNewSession();
		await store.submitNewSession();
		expect(spawn).not.toHaveBeenCalled();
		store.newSession.form.folder = ' ~/app ';
		await store.submitNewSession();
		expect(spawn).toHaveBeenCalledWith(
			expect.objectContaining({ folder: '~/app', machine: 'mac' }),
		);
		expect(store.newSession.open).toBe(true);
		expect(store.newSession.error).toContain('not available yet');
		expect(store.newSession.busy).toBe(false);
	});

	it('closes the new-session dialog after a real success', async () => {
		const { store } = setup();
		await store.init();
		store.openNewSession();
		store.newSession.form.folder = '~/app';
		await store.submitNewSession();
		expect(store.newSession.open).toBe(false);
	});

	it('switch dialog opens only for session rooms and preselects the agent', async () => {
		const { store } = setup({
			rooms: jest.fn(async () => [{ ...info(S1), agent: 'codex' }, info(MGR)]),
		});
		await store.init();
		await store.select(MGR_KEY);
		store.openSwitch();
		expect(store.switcher.open).toBe(false);
		await store.select(S1_KEY);
		store.openSwitch();
		expect(store.switcher.open).toBe(true);
		expect(store.switcher.agent).toBe('codex');
		store.pickSwitchAgent('opencode');
		expect(store.switcher.model).toBe('default');
		store.closeSwitch();
		expect(store.switcher.open).toBe(false);
	});

	const deferred = <T>() => {
		let resolve!: (v: T) => void;
		let reject!: (e: unknown) => void;
		const promise = new Promise<T>((res, rej) => {
			resolve = res;
			reject = rej;
		});
		return { promise, resolve, reject };
	};
	const flush = () => new Promise((r) => setTimeout(r, 0));
	type Choices = { value: string; label: string }[];
	const codexSetup = (agentModels: ManagerApi['agentModels']) =>
		setup({
			rooms: jest.fn(async () => [{ ...info(S1), agent: 'codex' }, info(MGR)]),
			agentModels,
		});

	it('switch dialog loads the agent model list once and caches it', async () => {
		const agentModels = jest.fn(async (_agent: string) => [
			{ value: 'default', label: 'default' },
			{ value: 'gemini-3.8-flash-medium', label: 'Gemini 3.8 Flash (Medium)' },
		]);
		const { store } = codexSetup(agentModels);
		await store.init();
		await store.select(S1_KEY);
		store.openSwitch();
		store.pickSwitchAgent('agy');
		await flush();
		expect(store.switchModels).toEqual(['default', 'gemini-3.8-flash-medium']);
		expect(store.switchLabels['gemini-3.8-flash-medium']).toBe(
			'Gemini 3.8 Flash (Medium)',
		);
		expect(store.switcher.model).toBe('default');
		store.closeSwitch();
		store.pickSwitchAgent('agy');
		await flush();
		expect(agentModels.mock.calls.filter((c) => c[0] === 'agy')).toHaveLength(
			1,
		);
	});

	it('switch dialog keeps the static list when the query fails and never asks for claude', async () => {
		const agentModels = jest.fn(async (_agent: string): Promise<Choices> => {
			throw new Error('agy not found');
		});
		const { store } = setup({ agentModels });
		await store.init();
		await store.select(S1_KEY);
		store.openSwitch();
		store.pickSwitchAgent('opencode');
		await flush();
		expect(store.switchModels).toEqual(['default']);
		expect(store.switchLabels).toEqual({});
		store.pickSwitchAgent('claude');
		expect(agentModels).not.toHaveBeenCalledWith('claude');
		expect(store.switchModels[0]).toBe('opus');
	});

	it('a failed query is retried on the next open', async () => {
		const first = deferred<Choices>();
		const second = deferred<Choices>();
		const agentModels = jest
			.fn()
			.mockReturnValueOnce(first.promise)
			.mockReturnValueOnce(second.promise);
		const { store } = codexSetup(agentModels);
		await store.init();
		await store.select(S1_KEY);
		store.openSwitch();
		first.reject(new Error('boom'));
		await flush();
		store.closeSwitch();
		store.openSwitch();
		expect(agentModels).toHaveBeenCalledTimes(2);
		second.resolve([{ value: 'gpt-7', label: 'GPT 7' }]);
		await flush();
		expect(store.switcher.model).toBe('gpt-7');
	});

	it('a loaded list replaces an unavailable untouched model', async () => {
		const d = deferred<Choices>();
		const { store } = codexSetup(jest.fn(() => d.promise));
		await store.init();
		await store.select(S1_KEY);
		store.openSwitch();
		d.resolve([{ value: 'gpt-7', label: 'GPT 7' }]);
		await flush();
		expect(store.switcher.model).toBe('gpt-7');
	});

	it('a chip picked before the list arrives is kept', async () => {
		const d = deferred<Choices>();
		const { store } = codexSetup(jest.fn(() => d.promise));
		await store.init();
		await store.select(S1_KEY);
		store.openSwitch();
		store.pickSwitchModel('gpt-6-luna');
		d.resolve([{ value: 'gpt-7', label: 'GPT 7' }]);
		await flush();
		expect(store.switcher.model).toBe('gpt-6-luna');
		expect(store.switchModels).toEqual(['gpt-7']);
	});

	it('a list arriving while a switch is busy does not change the model', async () => {
		const d = deferred<Choices>();
		const { store } = codexSetup(jest.fn(() => d.promise));
		await store.init();
		await store.select(S1_KEY);
		store.openSwitch();
		store.switcher.busy = true;
		d.resolve([{ value: 'gpt-7', label: 'GPT 7' }]);
		await flush();
		expect(store.switcher.model).toBe('gpt-6.1-terra');
	});

	it('a list arriving after the dialog closed does not touch the model', async () => {
		const d = deferred<Choices>();
		const { store } = codexSetup(jest.fn(() => d.promise));
		await store.init();
		await store.select(S1_KEY);
		store.openSwitch();
		store.closeSwitch();
		d.resolve([{ value: 'gpt-7', label: 'GPT 7' }]);
		await flush();
		expect(store.switcher.model).toBe('gpt-6.1-terra');
	});

	it('lists resolving out of order only affect their own agent', async () => {
		const agy = deferred<Choices>();
		const opencode = deferred<Choices>();
		const agentModels = jest.fn((agent: string) =>
			agent === 'agy' ? agy.promise : opencode.promise,
		);
		const { store } = codexSetup(agentModels);
		await store.init();
		await store.select(S1_KEY);
		store.openSwitch();
		store.pickSwitchAgent('agy');
		store.pickSwitchAgent('opencode');
		agy.resolve([{ value: 'agy-m', label: 'Agy M' }]);
		await flush();
		expect(store.switcher.agent).toBe('opencode');
		expect(store.switcher.model).toBe('default');
		opencode.resolve([{ value: 'p/m', label: 'p/m' }]);
		await flush();
		expect(store.switcher.model).toBe('p/m');
		store.pickSwitchAgent('agy');
		expect(store.switchModels).toEqual(['agy-m']);
		expect(store.switcher.model).toBe('agy-m');
	});

	it('switch stub error surfaces in the dialog', async () => {
		const switchAgent = jest.fn(async () => {
			throw new Error('Use the tmgr CLI.');
		});
		const { store } = setup({ switchAgent });
		await store.init();
		await store.select(S1_KEY);
		store.openSwitch();
		await store.submitSwitch();
		expect(switchAgent).toHaveBeenCalledWith(S1_KEY, 'claude', 'opus');
		expect(store.switcher.open).toBe(true);
		expect(store.switcher.error).toBe('Use the tmgr CLI.');
	});
});

describe('select by room key', () => {
	it('loads that room history and activates it', async () => {
		const { api, store } = setup();
		await store.init();
		await store.select(S1_KEY);
		expect(api.history).toHaveBeenLastCalledWith({ room: S1, limit: 50 });
		expect(store.activeKey).toBe(S1_KEY);
	});
});

describe('watchRooms', () => {
	beforeEach(() => {
		jest.useFakeTimers();
	});

	afterEach(() => {
		jest.useRealTimers();
	});

	it('picks up a session that appears after the first poll', async () => {
		const rooms = jest
			.fn()
			.mockResolvedValueOnce([info(MGR)])
			.mockResolvedValueOnce([info(MGR), info(S1)]);
		const { store } = setup({ rooms });
		await store.init();
		const stop = store.watchRooms(5000);
		await jest.advanceTimersByTimeAsync(5000);
		expect(store.rooms.map((r) => roomKey(r.room))).toEqual([MGR_KEY, S1_KEY]);
		expect(store.activeKey).toBe(MGR_KEY);
		stop();
	});

	it('keeps the previous rooms and sets no error when a poll fails', async () => {
		const rooms = jest
			.fn()
			.mockResolvedValueOnce([info(MGR), info(S1)])
			.mockRejectedValue(new Error('offline'));
		const { api, store } = setup({ rooms });
		await store.init();
		const stop = store.watchRooms(5000);
		await jest.advanceTimersByTimeAsync(10000);
		expect(api.rooms).toHaveBeenCalledTimes(3);
		expect(store.rooms.map((r) => roomKey(r.room))).toEqual([MGR_KEY, S1_KEY]);
		expect(store.error).toBeNull();
		stop();
	});

	it('stops polling after stop()', async () => {
		const { api, store } = setup();
		await store.init();
		const stop = store.watchRooms(5000);
		stop();
		await jest.advanceTimersByTimeAsync(20000);
		expect(api.rooms).toHaveBeenCalledTimes(1);
	});
});

describe('queued messages', () => {
	beforeEach(() => jest.useFakeTimers());
	afterEach(() => jest.useRealTimers());

	const flush = async () => {
		for (let i = 0; i < 5; i++) await Promise.resolve();
	};

	it('refreshes the queue when a room opens, after a send and after an ingest', async () => {
		const queued = jest.fn(async () => ['m050']);
		const { api, store, emit } = setup({ queued });
		await store.init();
		await flush();
		expect(api.queued).toHaveBeenCalledWith(MGR);
		expect(store.queuedIds).toEqual(['m050']);
		const before = queued.mock.calls.length;
		emit(msg(77, MGR));
		await flush();
		expect(queued.mock.calls.length).toBe(before + 1);
		queued.mockResolvedValue([]);
		store.draft.text = 'hi';
		await store.send();
		await flush();
		expect(store.queuedIds).toEqual([]);
	});

	it('loads why the queued messages wait and survives an older runner', async () => {
		const queuedWhy = jest.fn(async () => ({ m050: 'turn' }));
		const { store } = setup({ queued: async () => ['m050'], queuedWhy });
		await store.init();
		await flush();
		expect(queuedWhy).toHaveBeenCalledWith(MGR);
		expect(store.queuedWhy).toEqual({ m050: 'turn' });
		const failing = setup({
			queued: async () => ['m050'],
			queuedWhy: async () => {
				throw new Error('unknown method');
			},
		});
		await failing.store.init();
		await flush();
		expect(failing.store.queuedIds).toEqual(['m050']);
		expect(failing.store.queuedWhy).toEqual({});
	});

	it('polls every 2 seconds only while messages are queued', async () => {
		const queued = jest.fn(async () => ['m050']);
		const { store } = setup({ queued });
		await store.init();
		await flush();
		const opened = queued.mock.calls.length;
		await jest.advanceTimersByTimeAsync(2000);
		expect(queued.mock.calls.length).toBe(opened + 1);
		queued.mockResolvedValue([]);
		await jest.advanceTimersByTimeAsync(2000);
		const drained = queued.mock.calls.length;
		await jest.advanceTimersByTimeAsync(10_000);
		expect(queued.mock.calls.length).toBe(drained);
	});

	it('polls a few times after a send and then stops when nothing is queued', async () => {
		const queued = jest.fn(async () => []);
		const { store } = setup({ queued });
		await store.init();
		await flush();
		store.draft.text = 'hi';
		await store.send();
		await flush();
		const sent = queued.mock.calls.length;
		await jest.advanceTimersByTimeAsync(2000 * 4);
		expect(queued.mock.calls.length).toBeGreaterThan(sent);
		const settled = queued.mock.calls.length;
		await jest.advanceTimersByTimeAsync(10_000);
		expect(queued.mock.calls.length).toBe(settled);
	});

	it('stops polling on room switch and ignores a late response for the old room', async () => {
		let resolveManager: (ids: string[]) => void = () => {};
		const queued = jest.fn((room: RoomRef) =>
			room.kind === 'manager'
				? new Promise<string[]>((resolve) => {
						resolveManager = resolve;
					})
				: Promise.resolve([]),
		);
		const { store } = setup({ queued });
		await store.init();
		await store.select(S1_KEY);
		await flush();
		resolveManager(['m050']);
		await flush();
		expect(store.queuedIds).toEqual([]);
		const switched = queued.mock.calls.length;
		await jest.advanceTimersByTimeAsync(10_000);
		expect(queued.mock.calls.length).toBe(switched);
		expect(queued.mock.calls.slice(switched)).toEqual([]);
	});

	it('keeps the newest queue when responses arrive out of order', async () => {
		const pending: Array<(ids: string[]) => void> = [];
		const queued = jest.fn(
			() =>
				new Promise<string[]>((resolve) => {
					pending.push(resolve);
				}),
		);
		const { store, emit } = setup({ queued });
		await store.init();
		emit(msg(77, MGR));
		await flush();
		expect(pending).toHaveLength(2);
		pending[1]([]);
		await flush();
		pending[0](['m050']);
		await flush();
		expect(store.queuedIds).toEqual([]);
	});

	it('cancelQueued cancels the message and refreshes the queue', async () => {
		let ids = ['m050'];
		const cancel = jest.fn(async () => {
			ids = [];
			return true;
		});
		const { api, store } = setup({
			queued: jest.fn(async () => ids),
			cancel,
		});
		await store.init();
		await flush();
		await store.cancelQueued('m050');
		expect(api.cancel).toHaveBeenCalledWith(MGR, 'm050');
		expect(store.queuedIds).toEqual([]);
		expect(store.error).toBeNull();
	});

	it('cancelQueued reports a failed cancel as the store error', async () => {
		const { store } = setup({
			queued: jest.fn(async () => ['m050']),
			cancel: jest.fn(async () => {
				throw new Error('runner is down');
			}),
		});
		await store.init();
		await flush();
		await store.cancelQueued('m050');
		expect(store.error).toBe('runner is down');
	});
});

describe('live tool activity', () => {
	beforeEach(() => jest.useFakeTimers());
	afterEach(() => jest.useRealTimers());

	const act = (
		room: RoomRef,
		state: 'working' | 'idle',
		current: string | null,
		used: string[],
	): ActivityEvent => ({ v: 1, room, state, current, used });

	it('subscribes to activity on init and keeps the latest event per room', async () => {
		const { api, store, emitActivity } = setup();
		await store.init();
		expect(api.onActivity).toHaveBeenCalled();
		emitActivity(act(S1, 'working', 'Read: a.ts', ['Read: a.ts']));
		emitActivity(
			act(S1, 'working', 'Bash: cargo test', [
				'Read: a.ts',
				'Bash: cargo test',
			]),
		);
		expect(store.activityByRoom[S1_KEY]).toEqual({
			state: 'working',
			current: 'Bash: cargo test',
			used: ['Read: a.ts', 'Bash: cargo test'],
			calls: [],
		});
		expect(store.activityByRoom[MGR_KEY]).toBeUndefined();
	});

	it('drops the live entry on idle', async () => {
		const { store, emitActivity } = setup();
		await store.init();
		emitActivity(act(S1, 'working', 'Read: a.ts', ['Read: a.ts']));
		emitActivity(act(S1, 'idle', null, ['Read: a.ts']));
		expect(store.activityByRoom[S1_KEY]).toBeUndefined();
	});

	it('expires a stale live entry after 60 s without an update', async () => {
		const { store, emitActivity } = setup();
		await store.init();
		emitActivity(act(S1, 'working', 'Read: a.ts', ['Read: a.ts']));
		jest.advanceTimersByTime(59_000);
		expect(store.activityByRoom[S1_KEY]).toBeDefined();
		emitActivity(
			act(S1, 'working', 'Read: b.ts', ['Read: a.ts', 'Read: b.ts']),
		);
		jest.advanceTimersByTime(59_000);
		expect(store.activityByRoom[S1_KEY]).toBeDefined();
		jest.advanceTimersByTime(2_000);
		expect(store.activityByRoom[S1_KEY]).toBeUndefined();
	});

	it('snapshots the used tools onto the next agent message and clears the live entry', async () => {
		const { store, emit, emitActivity } = setup();
		await store.init();
		emitActivity(act(S1, 'working', 'Bash: ls', ['Read: a.ts', 'Bash: ls']));
		emit(msg(10, S1, 'session'));
		expect(store.toolsByMessage.m010).toEqual(['Read: a.ts', 'Bash: ls']);
		expect(store.activityByRoom[S1_KEY]).toBeUndefined();
	});

	it('keeps capped call details live and snapshots them onto the next agent message', async () => {
		const { store, emit, emitActivity } = setup();
		await store.init();
		const long = 'x'.repeat(500);
		emitActivity({
			...act(S1, 'working', 'Bash: ls', ['Bash: ls']),
			calls: [
				{
					n: 1,
					name: 'Bash',
					summary: long,
					result: 'y'.repeat(3000),
					running: true,
				},
			],
		});
		const live = store.activityByRoom[S1_KEY].calls ?? [];
		expect(live).toHaveLength(1);
		expect(live[0].summary).toHaveLength(200);
		expect(live[0].running).toBe(true);
		emit(msg(10, S1, 'session'));
		expect(store.callsByMessage.m010[0].name).toBe('Bash');
		expect(store.callsByMessage.m010[0].result).toHaveLength(1608);
	});

	it('treats an event without calls as an empty list', async () => {
		const { store, emit, emitActivity } = setup();
		await store.init();
		emitActivity(act(S1, 'working', 'Bash: ls', ['Bash: ls']));
		expect(store.activityByRoom[S1_KEY].calls).toEqual([]);
		emit(msg(10, S1, 'session'));
		expect(store.callsByMessage).toEqual({});
	});

	it('still snapshots when idle arrives before the reply', async () => {
		const { store, emit, emitActivity } = setup();
		await store.init();
		emitActivity(act(S1, 'working', 'Bash: ls', ['Bash: ls']));
		emitActivity(act(S1, 'idle', null, ['Bash: ls']));
		emit(msg(10, S1, 'session'));
		expect(store.toolsByMessage.m010).toEqual(['Bash: ls']);
	});

	it('does not snapshot for owner messages, empty tool lists, other rooms, or stale activity', async () => {
		const { store, emit, emitActivity } = setup();
		await store.init();
		emitActivity(act(S1, 'working', 'Bash: ls', ['Bash: ls']));
		emit(msg(10, S1, 'owner'));
		emit(msg(11, MGR, 'manager'));
		expect(store.toolsByMessage).toEqual({});
		emitActivity(act(MGR, 'working', null, []));
		emit(msg(12, MGR, 'manager'));
		expect(store.toolsByMessage).toEqual({});
		jest.advanceTimersByTime(61_000);
		emit(msg(13, S1, 'session'));
		expect(store.toolsByMessage).toEqual({});
	});

	it('does not snapshot twice for a replayed message and caps the list at 50', async () => {
		const { store, emit, emitActivity } = setup();
		await store.init();
		const many = Array.from({ length: 60 }, (_, i) => `Read: ${i}`);
		emitActivity(act(S1, 'working', 'Read: 59', many));
		emit(msg(10, S1, 'session'));
		expect(store.toolsByMessage.m010).toHaveLength(50);
		emitActivity(act(S1, 'working', 'Read: z', ['Read: z']));
		emit(msg(10, S1, 'session'));
		expect(store.toolsByMessage.m010).toHaveLength(50);
		expect(store.activityByRoom[S1_KEY]).toBeDefined();
	});

	it('attaches tools only to the room own agent message', async () => {
		const { store, emit, emitActivity } = setup();
		await store.init();
		emitActivity(act(S1, 'working', 'Bash: ls', ['Bash: ls']));
		emit(msg(10, S1, 'manager'));
		expect(store.toolsByMessage).toEqual({});
		emit(msg(11, S1, 'session'));
		expect(store.toolsByMessage.m011).toEqual(['Bash: ls']);
		emitActivity(act(MGR, 'working', 'Read: a', ['Read: a']));
		emit(msg(12, MGR, 'session'));
		expect(store.toolsByMessage.m012).toBeUndefined();
		emit(msg(13, MGR, 'manager'));
		expect(store.toolsByMessage.m013).toEqual(['Read: a']);
	});

	it('caps labels at 80 characters and the list at 50 on receipt', async () => {
		const { store, emitActivity } = setup();
		await store.init();
		const long = 'x'.repeat(200);
		emitActivity(act(S1, 'working', long, Array(60).fill(long)));
		const live = store.activityByRoom[S1_KEY];
		expect(live.used).toHaveLength(50);
		expect(live.used[0]).toHaveLength(80);
		expect(live.current).toHaveLength(80);
	});

	it('keeps only the newest 200 message tool lists', async () => {
		const { store, emit, emitActivity } = setup();
		await store.init();
		for (let i = 0; i < 205; i++) {
			emitActivity(act(S1, 'working', 't', ['t']));
			emit(msg(i, S1, 'session'));
		}
		const keys = Object.keys(store.toolsByMessage);
		expect(keys).toHaveLength(200);
		expect(store.toolsByMessage.m000).toBeUndefined();
		expect(store.toolsByMessage.m204).toBeDefined();
	});

	it('drops tool lists and activity of a room that is removed', async () => {
		const rooms = jest.fn(async () => [info(S1), info(MGR)]);
		const { store, emit, emitActivity } = setup({ rooms });
		await store.init();
		emitActivity(act(S1, 'working', 't', ['t']));
		emit(msg(10, S1, 'session'));
		emitActivity(act(S1, 'working', 'u', ['u']));
		rooms.mockResolvedValue([info(MGR)]);
		await store.loadRooms();
		expect(store.toolsByMessage).toEqual({});
		expect(store.activityByRoom[S1_KEY]).toBeUndefined();
	});

	it('keeps the used list of a finished turn from the idle event for the reply', async () => {
		const { store, emit, emitActivity } = setup();
		await store.init();
		emitActivity(act(S1, 'idle', null, ['Read: a.ts']));
		expect(store.activityByRoom[S1_KEY]).toBeUndefined();
		emit(msg(10, S1, 'session'));
		expect(store.toolsByMessage.m010).toEqual(['Read: a.ts']);
	});

	it('clears everything for the room on an empty idle', async () => {
		const { store, emit, emitActivity } = setup();
		await store.init();
		emitActivity(act(S1, 'working', 't', ['t']));
		emitActivity(act(S1, 'idle', null, []));
		expect(store.activityByRoom[S1_KEY]).toBeUndefined();
		emit(msg(10, S1, 'session'));
		expect(store.toolsByMessage).toEqual({});
	});

	it('expiry also clears the remembered tools', async () => {
		const { store, emit, emitActivity } = setup();
		await store.init();
		emitActivity(act(S1, 'idle', null, ['t']));
		jest.advanceTimersByTime(61_000);
		jest.setSystemTime(Date.now() - 61_000);
		emit(msg(10, S1, 'session'));
		expect(store.toolsByMessage).toEqual({});
	});
});

describe('chat control', () => {
	const running = { ...info(S1), state: 'working' };
	const exited = { ...info(S1), state: 'exited' };
	const withRooms = (first: RoomInfo, extra: Partial<ManagerApi> = {}) =>
		setup({ rooms: jest.fn(async () => [first, info(MGR)]), ...extra });

	it('stopActive asks for a graceful stop and refreshes the rooms', async () => {
		const { api, store } = withRooms(running);
		await store.init();
		await store.select(S1_KEY);
		const before = (api.rooms as jest.Mock).mock.calls.length;
		await store.stopActive();
		expect(api.stop).toHaveBeenCalledWith(S1, false);
		expect((api.rooms as jest.Mock).mock.calls.length).toBe(before + 1);
		expect(store.control.busy).toBe(false);
		expect(store.control.error).toBe('');
	});

	it('stopActive shows busy while the call is in flight and ignores a second click', async () => {
		let release: (v: { exited: boolean }) => void = () => {};
		const stop = jest.fn(
			() => new Promise<{ exited: boolean }>((r) => (release = r)),
		);
		const { store } = withRooms(running, { stop });
		await store.init();
		await store.select(S1_KEY);
		const first = store.stopActive();
		expect(store.control.busy).toBe(true);
		await store.stopActive();
		expect(stop).toHaveBeenCalledTimes(1);
		release({ exited: true });
		await first;
		expect(store.control.busy).toBe(false);
	});

	it('a stop that did not exit points to Force stop', async () => {
		const { store } = withRooms(running, {
			stop: jest.fn(async () => ({ exited: false })),
		});
		await store.init();
		await store.select(S1_KEY);
		await store.stopActive();
		expect(store.control.stillRunning).toBe(true);
	});

	it('stop errors surface and clear busy', async () => {
		const { store } = withRooms(running, {
			stop: jest.fn(async () => {
				throw new Error('The runner is busy');
			}),
		});
		await store.init();
		await store.select(S1_KEY);
		await store.stopActive();
		expect(store.control.error).toBe('The runner is busy');
		expect(store.control.busy).toBe(false);
	});

	it('force stop calls the api only after the dialog is confirmed', async () => {
		const { api, store } = withRooms(running);
		await store.init();
		await store.select(S1_KEY);
		store.openControl('force');
		expect(store.control.open).toBe('force');
		expect(api.stop).not.toHaveBeenCalled();
		await store.forceStopActive();
		expect(api.stop).toHaveBeenCalledWith(S1, true);
		expect(store.control.open).toBeNull();
	});

	it('cancelling the dialog never calls the api', async () => {
		const { api, store } = withRooms(exited);
		await store.init();
		await store.select(S1_KEY);
		store.openControl('delete');
		store.closeControl();
		expect(store.control.open).toBeNull();
		expect(api.remove).not.toHaveBeenCalled();
		expect(api.stop).not.toHaveBeenCalled();
	});

	it('changing the active room closes the dialog and nothing is called', async () => {
		const { api, store } = withRooms(exited);
		await store.init();
		await store.select(S1_KEY);
		store.openControl('delete');
		await store.select(MGR_KEY);
		expect(store.control.open).toBeNull();
		await store.deleteActive();
		await store.forceStopActive();
		expect(api.remove).not.toHaveBeenCalled();
		expect(api.stop).not.toHaveBeenCalled();
	});

	it('confirm acts on the room the dialog was opened for', async () => {
		const other: RoomInfo = {
			...info({ ...S1, session: 's2' }),
			state: 'working',
		};
		const stop = jest.fn(async () => ({ exited: true }));
		const { store } = setup({
			rooms: jest.fn(async () => [running, other, info(MGR)]),
			stop,
		});
		await store.init();
		await store.select(S1_KEY);
		store.openControl('force');
		store.activeKey = roomKey(other.room);
		await store.forceStopActive();
		expect(stop).toHaveBeenCalledWith(S1, true);
	});

	it('a stop result for a room that is no longer active is ignored', async () => {
		let release: (v: { exited: boolean }) => void = () => {};
		const stop = jest.fn(
			() => new Promise<{ exited: boolean }>((r) => (release = r)),
		);
		const { store } = withRooms(running, { stop });
		await store.init();
		await store.select(S1_KEY);
		const pending = store.stopActive();
		await store.select(MGR_KEY);
		release({ exited: false });
		await pending;
		expect(store.control.stillRunning).toBe(false);
		expect(store.control.busy).toBe(false);
	});

	it('a delete error for a room that is no longer active is ignored', async () => {
		let fail: (e: Error) => void = () => {};
		const remove = jest.fn(() => new Promise<void>((_, rej) => (fail = rej)));
		const { store } = withRooms(exited, { remove });
		await store.init();
		await store.select(S1_KEY);
		store.openControl('delete');
		const pending = store.deleteActive();
		await store.select(MGR_KEY);
		fail(new Error('late'));
		await pending;
		expect(store.control.error).toBe('');
		expect(store.control.busy).toBe(false);
	});

	it('a deleted room leaves the list even when the refresh fails', async () => {
		let gone = false;
		const rooms = jest.fn(async () => {
			if (gone) throw new Error('down');
			return [exited, info(MGR)];
		});
		const { store } = setup({
			rooms,
			remove: jest.fn(async () => {
				gone = true;
			}),
		});
		await store.init();
		await store.select(S1_KEY);
		store.openControl('delete');
		await store.deleteActive();
		expect(store.rooms.map((r) => roomKey(r.room))).toEqual([MGR_KEY]);
		expect(store.activeKey).toBe(MGR_KEY);
	});

	it('failed force stop keeps the dialog open with the error', async () => {
		const { store } = withRooms(running, {
			stop: jest.fn(async () => {
				throw new Error('boom');
			}),
		});
		await store.init();
		await store.select(S1_KEY);
		store.openControl('force');
		await store.forceStopActive();
		expect(store.control.open).toBe('force');
		expect(store.control.error).toBe('boom');
	});

	it('deleteActive removes the room and selects another one', async () => {
		let gone = false;
		const remove = jest.fn(async () => {
			gone = true;
		});
		const { store } = setup({
			rooms: jest.fn(async () => (gone ? [info(MGR)] : [exited, info(MGR)])),
			remove,
		});
		await store.init();
		await store.select(S1_KEY);
		store.openControl('delete');
		await store.deleteActive();
		expect(remove).toHaveBeenCalledWith(S1);
		expect(store.control.open).toBeNull();
		expect(store.messagesByRoom[S1_KEY]).toBeUndefined();
		expect(store.activeKey).toBe(MGR_KEY);
	});

	it('delete error keeps the dialog open', async () => {
		const { store } = withRooms(exited, {
			remove: jest.fn(async () => {
				throw new Error('Stop the chat first');
			}),
		});
		await store.init();
		await store.select(S1_KEY);
		store.openControl('delete');
		await store.deleteActive();
		expect(store.control.open).toBe('delete');
		expect(store.control.error).toBe('Stop the chat first');
		expect(store.control.busy).toBe(false);
	});

	it('control actions are refused for the manager room, remote rooms and wrong states', async () => {
		const remote = { ...running, local: false };
		const a = withRooms(running);
		await a.store.init();
		await a.store.select(MGR_KEY);
		await a.store.stopActive();
		a.store.openControl('force');
		await a.store.deleteActive();
		expect(a.store.control.open).toBeNull();
		const b = withRooms(remote);
		await b.store.init();
		await b.store.select(S1_KEY);
		await b.store.stopActive();
		await b.store.forceStopActive();
		const c = withRooms(running);
		await c.store.init();
		await c.store.select(S1_KEY);
		await c.store.deleteActive();
		const d = withRooms(exited);
		await d.store.init();
		await d.store.select(S1_KEY);
		await d.store.stopActive();
		for (const t of [a, b, c, d]) {
			expect(t.api.stop).not.toHaveBeenCalled();
			expect(t.api.remove).not.toHaveBeenCalled();
		}
	});
});

describe('delivery', () => {
	const REMOTE: RoomInfo = { ...info(S1), local: false };
	const echo = jest.fn(
		async (p: { id?: string; room: RoomRef; body_md: string }) => ({
			...msg(1, p.room, 'owner'),
			id: p.id as string,
			body_md: p.body_md,
		}),
	);
	const remoteSetup = (over: Partial<ManagerApi> = {}) => {
		let receipt: (r: Receipt) => void = () => {};
		const t = setup({
			rooms: jest.fn(async () => [REMOTE, info(MGR)]),
			send: echo,
			onReceipt: jest.fn(async (h) => {
				receipt = h;
				return () => {};
			}),
			...over,
		});
		return { ...t, receipt: (r: Receipt) => receipt(r) };
	};
	const sendRemote = async (t: ReturnType<typeof remoteSetup>, text = 'hi') => {
		await t.store.init();
		await t.store.select(S1_KEY);
		t.store.draft.text = text;
		return t.store.send();
	};
	const own = (t: ReturnType<typeof remoteSetup>) =>
		t.store.messagesByRoom[S1_KEY].filter((m) => m.author.kind === 'owner');

	beforeEach(() => {
		echo.mockClear();
		jest.useFakeTimers();
	});
	afterEach(() => jest.useRealTimers());

	it('shows the optimistic row as sending before api.send resolves and clears the draft', async () => {
		let release: (m: ChatMessage) => void = () => {};
		const t = remoteSetup({
			send: jest.fn(
				(p) =>
					new Promise<ChatMessage>((res) => {
						release = () =>
							res({ ...msg(1, p.room, 'owner'), id: p.id as string });
					}),
			),
		});
		await t.store.init();
		await t.store.select(S1_KEY);
		t.store.draft.text = 'hi';
		const pending = t.store.send();
		const row = own(t)[0];
		expect(row.id).toMatch(/^m_[0-9a-f]{32}$/);
		expect(row.body_md).toBe('hi');
		expect(t.store.delivery[row.id].state).toBe('sending');
		expect(t.store.draft.text).toBe('');
		release(row);
		await pending;
		expect(own(t)).toHaveLength(1);
		expect(t.store.delivery[row.id].state).toBe('sent');
	});

	it('passes the generated id to api.send and keeps it for the stored message', async () => {
		const t = remoteSetup();
		await sendRemote(t);
		const id = (t.api.send as jest.Mock).mock.calls[0][0].id;
		expect(own(t).map((m) => m.id)).toEqual([id]);
	});

	it('sends ts_ms with every id, on the first send and on retry', async () => {
		const t = remoteSetup({
			send: jest.fn(async () => {
				throw new Error('relay down');
			}),
		});
		await sendRemote(t, 'hello');
		const first = own(t)[0];
		await t.store.retry(first.id);
		const calls = (t.api.send as jest.Mock).mock.calls.map((c) => c[0]);
		expect(calls).toHaveLength(2);
		for (const params of calls) {
			expect(typeof params.id).toBe('string');
			expect(typeof params.ts_ms).toBe('number');
		}
	});

	it('keeps the text, marks the row failed and offers retry when send fails', async () => {
		const t = remoteSetup({
			send: jest.fn(async () => {
				throw new Error('relay down');
			}),
		});
		await sendRemote(t, 'hello');
		const row = own(t)[0];
		expect(t.store.draft.text).toBe('hello');
		expect(t.store.delivery[row.id]).toMatchObject({
			state: 'failed',
			reason: 'relay down',
		});
	});

	it('does not overwrite a new draft when a send fails', async () => {
		let fail: (e: Error) => void = () => {};
		const t = remoteSetup({
			send: jest.fn(
				() =>
					new Promise<ChatMessage>((_, rej) => {
						fail = rej;
					}),
			),
		});
		await t.store.init();
		await t.store.select(S1_KEY);
		t.store.draft.text = 'one';
		const pending = t.store.send();
		t.store.draft.text = 'two';
		fail(new Error('x'));
		await pending;
		expect(t.store.draft.text).toBe('two');
	});

	it('retry of a message the relay rejected reuses its id and ts_ms', async () => {
		let fail = true;
		const t = remoteSetup({
			send: jest.fn(
				async (p: { id?: string; room: RoomRef; body_md: string }) => {
					if (fail) throw new Error('relay down');
					return {
						...msg(1, p.room, 'owner'),
						id: p.id as string,
						body_md: p.body_md,
					};
				},
			),
		});
		await sendRemote(t, 'again');
		const first = own(t)[0];
		fail = false;
		await t.store.retry(first.id);
		const calls = (t.api.send as jest.Mock).mock.calls.map((c) => c[0]);
		expect(calls).toHaveLength(2);
		expect(calls[1].id).toBe(first.id);
		expect(calls[1].ts_ms).toBe(first.ts_ms);
		expect(own(t)).toHaveLength(1);
		expect(t.store.delivery[first.id].state).toBe('sent');
	});

	it('a send that never answers fails after 30 s with the timeout reason, and retry reuses the id', async () => {
		let hang = true;
		const t = remoteSetup({
			send: jest.fn((p: { id?: string; room: RoomRef; body_md: string }) =>
				hang
					? new Promise<ChatMessage>(() => {})
					: Promise.resolve({
							...msg(1, p.room, 'owner'),
							id: p.id as string,
							body_md: p.body_md,
						}),
			),
		});
		await t.store.init();
		await t.store.select(S1_KEY);
		t.store.draft.text = 'slow';
		const pending = t.store.send();
		const id = own(t)[0].id;
		expect(t.store.delivery[id].state).toBe('sending');
		jest.advanceTimersByTime(30_000);
		await pending;
		expect(t.store.delivery[id]).toMatchObject({
			state: 'failed',
			reason: 'send_timeout',
		});
		hang = false;
		await t.store.retry(id);
		const ids = (t.api.send as jest.Mock).mock.calls.map((c) => c[0].id);
		expect(ids).toEqual([id, id]);
		expect(t.store.delivery[id].state).toBe('sent');
	});

	it('retry of a message the relay accepted but nobody confirmed still sends a new message', async () => {
		const t = remoteSetup();
		await sendRemote(t, 'unsure');
		const first = own(t)[0].id;
		jest.advanceTimersByTime(20_000);
		expect(t.store.delivery[first].state).toBe('unconfirmed');
		await t.store.retry(first);
		const calls = (t.api.send as jest.Mock).mock.calls;
		expect(calls[1][0].id).not.toBe(first);
	});

	it('retry sends the same text as a new message with a new id and marks the old row retried', async () => {
		const t = remoteSetup();
		await sendRemote(t, 'again');
		const first = own(t)[0].id;
		t.receipt({ id: first, state: 'failed', reason: 'refused' });
		await t.store.retry(first);
		const calls = (t.api.send as jest.Mock).mock.calls;
		expect(calls).toHaveLength(2);
		expect(calls[1][0].body_md).toBe('again');
		expect(calls[1][0].id).not.toBe(first);
		expect(t.store.delivery[first].retried).toBe(true);
		expect(own(t)).toHaveLength(2);
	});

	it('applies delivered and queued receipts by id, last event wins', async () => {
		const t = remoteSetup();
		await sendRemote(t);
		await t.store.init();
		const id = own(t)[0].id;
		t.receipt({ id, state: 'queued' });
		expect(t.store.delivery[id].state).toBe('queued');
		t.receipt({ id, state: 'delivered' });
		expect(t.store.delivery[id].state).toBe('delivered');
		t.receipt({ id, state: 'failed', reason: 'expired' });
		expect(t.store.delivery[id]).toMatchObject({
			state: 'failed',
			reason: 'expired',
		});
	});

	it('treats a system note replying to an own message as a failure receipt', async () => {
		const t = remoteSetup();
		await sendRemote(t);
		const id = own(t)[0].id;
		t.emit({
			...msg(50, S1, 'owner'),
			id: 'm050',
			author: { kind: 'system', id: 'runner' },
			kind: 'text',
			reply_to: id,
			body_md: 'Not delivered: session is busy',
		});
		expect(t.store.delivery[id]).toMatchObject({
			state: 'failed',
			reason: 'session is busy',
		});
	});

	it('ignores system notes that do not start with Not delivered', async () => {
		const t = remoteSetup();
		await sendRemote(t);
		const id = own(t)[0].id;
		t.emit({
			...msg(50, S1),
			author: { kind: 'system', id: 'runner' },
			reply_to: id,
			body_md: 'Typed into the session',
		});
		expect(t.store.delivery[id].state).toBe('sent');
	});

	it('does not duplicate the row when the poll delivers the same id', async () => {
		const t = remoteSetup();
		await sendRemote(t);
		const row = own(t)[0];
		t.emit({ ...row, ts_ms: row.ts_ms + 5 });
		expect(own(t)).toHaveLength(1);
	});

	it('flags a remote message as not confirmed after 20 s without a receipt', async () => {
		const t = remoteSetup();
		await sendRemote(t);
		const id = own(t)[0].id;
		jest.advanceTimersByTime(19_999);
		expect(t.store.delivery[id].state).toBe('sent');
		jest.advanceTimersByTime(1);
		expect(t.store.delivery[id].state).toBe('unconfirmed');
	});

	it('does not time out a delivered message', async () => {
		const t = remoteSetup();
		await sendRemote(t);
		const id = own(t)[0].id;
		t.receipt({ id, state: 'delivered' });
		jest.advanceTimersByTime(60_000);
		expect(t.store.delivery[id].state).toBe('delivered');
	});

	it('local rooms drop the status once the send is accepted', async () => {
		const t = setup({ send: echo });
		await t.store.init();
		t.store.draft.text = 'x';
		await t.store.send();
		expect(t.store.delivery).toEqual({});
	});
});
