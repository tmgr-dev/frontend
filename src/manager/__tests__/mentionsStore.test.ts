import { createManagerStore, type ManagerApi } from '../store';
import {
	roomKey,
	type ChatMessage,
	type MentionRef,
	type RoomInfo,
	type RoomRef,
	type SendParams,
} from '../types';

const MGR: RoomRef = { kind: 'manager', machine: 'mac' };
const A: RoomRef = { kind: 'session', machine: 'mac', session: 's_a' };
const B: RoomRef = { kind: 'session', machine: 'mac', session: 's_b' };
const RISKY: RoomRef = { kind: 'session', machine: 'mac', session: 's_r' };
const FAR: RoomRef = { kind: 'session', machine: 'far', session: 's_f' };

const base = (room: RoomRef, title: string, extra: Partial<RoomInfo> = {}) =>
	({ room, title, agent: 'claude', ...extra }) as RoomInfo;

const rooms = (): RoomInfo[] => [
	base(MGR, 'Manager', { agent: undefined, mention: { ok: true } }),
	base(A, 'alpha', { mention: { ok: true } }),
	base(B, 'build-fix', { mention: { ok: true } }),
	base(RISKY, 'bypass one', { mention: { ok: true, risky: true } }),
	base(FAR, 'far one', { local: false }),
];

const sentRow = (p: SendParams): ChatMessage => ({
	v: 1,
	id: 'm_sent',
	room: p.room,
	author: { kind: 'owner', id: 'owner' },
	ts_ms: 1,
	kind: 'text',
	body_md: p.body_md,
});

const setup = (caps: string[], list: RoomInfo[] = rooms()) => {
	const send = jest.fn(async (p: SendParams) => sentRow(p));
	const mentionInject = jest.fn(async () => undefined);
	const api: ManagerApi = {
		rooms: jest.fn(async () => list),
		history: jest.fn(async () => []),
		send,
		commands: jest.fn(async () => []),
		search: jest.fn(async () => []),
		queued: jest.fn(async () => []),
		cancel: jest.fn(async () => true),
		agentModels: jest.fn(async () => {
			throw new Error('none');
		}),
		audit: jest.fn(async () => []),
		spawn: jest.fn(async () => undefined),
		switchAgent: jest.fn(async () => undefined),
		stop: jest.fn(async () => ({ exited: true })),
		remove: jest.fn(async () => undefined),
		onMessage: jest.fn(async () => () => {}),
		onActivity: jest.fn(async () => () => {}),
		caps: jest.fn(async () => caps),
		mentionInject,
	};
	return { store: createManagerStore(api), send, mentionInject };
};

const CAP = ['chat.v1', 'chat.mention.v1'];

const ready = async (caps = CAP, key = roomKey(MGR)) => {
	const ctx = setup(caps);
	await ctx.store.init();
	await ctx.store.select(key);
	return ctx;
};

describe('composer mentions', () => {
	it('opens the menu on a leading @ and lists only local targets', async () => {
		const { store } = await ready();
		await store.onInput('@');
		expect(store.mention.open).toBe(true);
		expect(store.mention.items.map((i) => i.sessionId).sort()).toEqual([
			's_a',
			's_b',
			's_r',
		]);
		await store.onInput('@build');
		expect(store.mention.items.map((i) => i.sessionId)).toEqual(['s_b']);
		await store.onInput('hello @build');
		expect(store.mention.open).toBe(false);
	});

	it('stays hidden when the runner has no chat.mention.v1', async () => {
		const { store } = await ready(['chat.v1']);
		await store.onInput('@');
		expect(store.mention.supported).toBe(false);
		expect(store.mention.open).toBe(false);
	});

	it('stays hidden in a relay-only origin room', async () => {
		const { store } = await ready(CAP, roomKey(FAR));
		await store.onInput('@');
		expect(store.mention.open).toBe(false);
	});

	it('sends the picked target as mention with the @name removed from the body', async () => {
		const { store, send } = await ready();
		await store.onInput('@build');
		store.pickMention();
		expect(store.draft.text).toBe('@build-fix ');
		await store.onInput('@build-fix what is the status?');
		await store.send();
		expect(send).toHaveBeenCalledTimes(1);
		expect(send.mock.calls[0][0]).toMatchObject({
			room: MGR,
			body_md: 'what is the status?',
			mention: { target: B, confirmed_risky: false },
		});
		expect(store.mention.picked).toBeNull();
		expect(store.draft.text).toBe('');
	});

	it('drops the pick when the @name is edited away, and sends plain text', async () => {
		const { store, send } = await ready();
		await store.onInput('@build');
		store.pickMention();
		await store.onInput('build-fix hi');
		expect(store.mention.picked).toBeNull();
		await store.send();
		expect(send.mock.calls[0][0].mention).toBeUndefined();
		expect(send.mock.calls[0][0].body_md).toBe('build-fix hi');
	});

	it('does not pick a disabled row', async () => {
		const list = rooms();
		list[2] = base(B, 'build-fix', {
			mention: { ok: false, reason: 'Exited' },
		});
		const ctx = setup(CAP, list);
		await ctx.store.init();
		await ctx.store.onInput('@build');
		ctx.store.pickMention();
		expect(ctx.store.mention.picked).toBeNull();
		expect(ctx.store.draft.text).toBe('@build');
	});

	it('needs a question after the name', async () => {
		const { store, send } = await ready();
		await store.onInput('@build');
		store.pickMention();
		await store.onInput('@build-fix ');
		await store.send();
		expect(send).not.toHaveBeenCalled();
		expect(store.error).toMatch(/question/);
	});

	it('asks before mentioning a risky target and sends confirmed_risky after OK', async () => {
		const { store, send } = await ready();
		await store.onInput('@bypass');
		store.pickMention();
		await store.onInput('@bypass-one run it');
		await store.send();
		expect(send).not.toHaveBeenCalled();
		expect(store.mention.confirm).toMatchObject({
			kind: 'send',
			body: 'run it',
		});
		store.cancelMention();
		expect(send).not.toHaveBeenCalled();
		await store.send();
		store.confirmMention();
		await new Promise((r) => setTimeout(r, 0));
		expect(send).toHaveBeenCalledTimes(1);
		expect(send.mock.calls[0][0]).toMatchObject({
			body_md: 'run it',
			mention: { target: RISKY, confirmed_risky: true },
		});
	});

	const riskyDraft = async () => {
		const ctx = await ready();
		await ctx.store.onInput('@bypass');
		ctx.store.pickMention();
		await ctx.store.onInput('@bypass-one run it');
		await ctx.store.send();
		return ctx;
	};
	const flush = () => new Promise((r) => setTimeout(r, 0));

	it('sends exactly the snapshot taken when the dialog opened', async () => {
		const { store, send } = await riskyDraft();
		store.confirmMention();
		await flush();
		expect(send).toHaveBeenCalledTimes(1);
		expect(send.mock.calls[0][0]).toMatchObject({
			room: MGR,
			body_md: 'run it',
			mention: { target: RISKY, confirmed_risky: true },
		});
	});

	it('aborts when the draft changed behind the dialog', async () => {
		const { store, send } = await riskyDraft();
		store.draft.text = '@bypass-one something else';
		store.confirmMention();
		await flush();
		expect(send).not.toHaveBeenCalled();
		expect(store.error).toMatch(/changed/);
	});

	it('aborts when the chat was switched behind the dialog', async () => {
		const { store, send } = await riskyDraft();
		await store.select(roomKey(A));
		store.draft.text = '@bypass-one run it';
		store.confirmMention();
		await flush();
		expect(send).not.toHaveBeenCalled();
	});

	it('aborts when the target is no longer a confirmed-risky target', async () => {
		const { store, send } = await riskyDraft();
		const info = store.rooms.find((r) => roomKey(r.room) === roomKey(RISKY))!;
		info.mention = { ok: false, reason: 'Exited' };
		store.confirmMention();
		await flush();
		expect(send).not.toHaveBeenCalled();
	});

	it('refuses to send a mention once the capability is gone', async () => {
		const { store, send } = await ready();
		await store.onInput('@build');
		store.pickMention();
		await store.onInput('@build-fix hi');
		store.mention.supported = false;
		await store.send();
		expect(send).not.toHaveBeenCalled();
		expect(store.error).toMatch(/not available/);
	});
});

const reply = (room: RoomRef): ChatMessage => {
	const mention: MentionRef = {
		id: 'm_q',
		target: B,
		target_title: 'build-fix',
		state: 'answered',
		hop: 1,
	};
	return {
		v: 1,
		id: 'm_r',
		room,
		author: { kind: 'session', id: 's_b' },
		ts_ms: 5,
		kind: 'text',
		body_md: 'all green',
		mention,
	};
};

describe('Add to context', () => {
	it('injects straight into a plain local session origin', async () => {
		const { store, mentionInject } = await ready();
		store.requestInject(reply(A));
		await new Promise((r) => setTimeout(r, 0));
		expect(mentionInject).toHaveBeenCalledWith(A, 'm_q', false);
		expect(store.mention.injected.m_q).toBe(true);
	});

	it('confirms first for a manager origin, then passes confirmed_risky', async () => {
		const { store, mentionInject } = await ready();
		store.requestInject(reply(MGR));
		expect(mentionInject).not.toHaveBeenCalled();
		expect(store.mention.confirm).toMatchObject({ kind: 'inject' });
		store.confirmMention();
		await new Promise((r) => setTimeout(r, 0));
		expect(mentionInject).toHaveBeenCalledWith(MGR, 'm_q', true);
	});

	it('confirms first for a risky session origin', async () => {
		const { store, mentionInject } = await ready();
		store.requestInject(reply(RISKY));
		expect(store.mention.confirm).toMatchObject({ kind: 'inject' });
		expect(mentionInject).not.toHaveBeenCalled();
	});

	it('is not callable without the capability', async () => {
		const ctx = setup(['chat.v1']);
		await ctx.store.init();
		ctx.store.requestInject(reply(A));
		await ctx.store.runInject(reply(A), true);
		expect(ctx.mentionInject).not.toHaveBeenCalled();
		expect(ctx.store.mention.confirm).toBeNull();
	});

	it('is refused for an unrestricted origin', async () => {
		const list = rooms();
		list[1] = base(A, 'alpha', { unrestricted: true, mention: { ok: true } });
		const ctx = setup(CAP, list);
		await ctx.store.init();
		ctx.store.requestInject(reply(A));
		await ctx.store.runInject(reply(A), true);
		expect(ctx.mentionInject).not.toHaveBeenCalled();
		expect(ctx.store.error).toMatch(/unrestricted/);
	});

	it('never runs for a room that is not on this machine', async () => {
		const { store, mentionInject } = await ready();
		store.requestInject(reply(FAR));
		await store.runInject(reply(FAR), true);
		expect(mentionInject).not.toHaveBeenCalled();
		expect(store.mention.confirm).toBeNull();
	});

	it('shows a runner refusal as an error and allows a retry', async () => {
		const { store, mentionInject } = await ready();
		mentionInject.mockRejectedValueOnce(new Error('not answered yet'));
		await store.runInject(reply(A), false);
		expect(store.error).toBe('not answered yet');
		expect(store.mention.injected.m_q).toBeUndefined();
		expect(store.mention.injecting.m_q).toBeUndefined();
	});
});

describe('state rows', () => {
	it('do not count as unread', async () => {
		const ctx = setup(CAP);
		let push: (m: ChatMessage) => void = () => {};
		const store = createManagerStore({
			rooms: async () => rooms(),
			history: async () => [],
			send: ctx.send,
			commands: async () => [],
			search: async () => [],
			queued: async () => [],
			cancel: async () => true,
			agentModels: async () => [],
			audit: async () => [],
			spawn: async () => undefined,
			switchAgent: async () => undefined,
			stop: async () => ({ exited: true }),
			remove: async () => undefined,
			onMessage: async (h) => {
				push = h;
				return () => {};
			},
			onActivity: async () => () => {},
		});
		await store.init();
		await store.select(roomKey(A));
		const state: ChatMessage = {
			...reply(MGR),
			id: 'm_state',
			author: { kind: 'system', id: 'runner' },
			kind: 'system',
			reply_to: 'm_q',
			mention: { ...reply(MGR).mention!, state: 'answering' },
		};
		push(state);
		push({ ...reply(MGR), id: 'm_real' });
		expect(store.unreadByRoom[roomKey(MGR)]).toBe(1);
	});
});
