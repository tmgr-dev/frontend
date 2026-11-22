import { createManagerStore, type ManagerApi } from '../store';
import {
	roomKey,
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
	const api: ManagerApi = {
		rooms: jest.fn(async () => [info(S1), info(MGR)]),
		history: jest.fn(async ({ room }) => range(1, 3, room)),
		send: jest.fn(async ({ room }) => msg(99, room, 'owner')),
		commands: jest.fn(async () => commands),
		search: jest.fn(async () => []),
		queued: jest.fn(async () => []),
		cancel: jest.fn(async () => true),
		audit: jest.fn(async () => []),
		spawn: jest.fn(async () => undefined),
		switchAgent: jest.fn(async () => undefined),
		onMessage: jest.fn(async (h) => {
			push = h;
			return () => {};
		}),
		...over,
	};
	const store = createManagerStore(api);
	return { api, store, emit: (m: ChatMessage) => push(m) };
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
