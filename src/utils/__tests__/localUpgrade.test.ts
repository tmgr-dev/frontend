import {
	clearUpgradeDialog,
	installLocalUpgrade,
	LOCAL_UPGRADE_KEY,
	markLocalUpgrade,
	readUpgradeMarker,
	retryLocalUpgrade,
	upgradeState,
} from '@/utils/localUpgrade';

const memoryStorage = () => {
	const data = new Map<string, string>();
	return {
		getItem: (key: string) => data.get(key) ?? null,
		setItem: (key: string, value: string) => void data.set(key, value),
		removeItem: (key: string) => void data.delete(key),
	};
};

const makeStore = (userId?: number) => {
	const listeners: Array<(m: { type: string; payload?: unknown }) => void> = [];
	const store = {
		state: { user: userId ? { id: userId } : null, localProfile: {} } as any,
		commit: jest.fn(),
		subscribe: (fn: any) => void listeners.push(fn),
	};
	const signIn = (id: number) => {
		store.state.user = { id };
		listeners.forEach((fn) => fn({ type: 'setUser', payload: { id } }));
	};
	return { store, signIn };
};

const flush = async () => {
	for (let i = 0; i < 20; i++) await Promise.resolve();
};

type Handlers = Record<string, (args: any) => unknown>;
const makeInvoke = (handlers: Handlers) =>
	jest.fn(async (command: string, args?: Record<string, unknown>) => {
		const handler = handlers[command];
		if (!handler) throw new Error(`unexpected ${command}`);
		return handler(args);
	});

const linkedTo = (id: number | null | undefined) => (args: any) => ({
	id: -1,
	uuid: 'u',
	name: 'Ada',
	created_at: 'x',
	...(id === undefined ? {} : { linked_user_id: id }),
	...(args ? {} : {}),
});

const commandsOf = (invoke: jest.Mock) => invoke.mock.calls.map((c) => c[0]);

test('success: links, migrates both, clears migrate and keeps the dialog flag', async () => {
	const storage = memoryStorage();
	markLocalUpgrade(storage);
	const { store } = makeStore(7);
	const invoke = makeInvoke({
		local_profile_link: linkedTo(7),
		persona_llm_migrate_local: () => ({ status: 'moved' }),
		persona_cache_migrate_local: () => ({ moved: 1, conflicts: 0 }),
	});
	installLocalUpgrade(store, invoke, storage);
	await flush();
	expect(invoke).toHaveBeenCalledWith('local_profile_link', { userId: 7 });
	expect(invoke).toHaveBeenCalledWith('persona_llm_migrate_local', {
		toUserId: 7,
	});
	expect(invoke).toHaveBeenCalledWith('persona_cache_migrate_local', {
		toUserId: 7,
	});
	expect(store.commit).toHaveBeenCalledWith(
		'setLocalProfile',
		expect.objectContaining({ linked_user_id: 7 }),
	);
	expect(readUpgradeMarker(storage)).toEqual({ migrate: false, dialog: true });
	expect(upgradeState).toMatchObject({ phase: 'done', key: 'moved' });
	clearUpgradeDialog(storage);
	expect(storage.getItem(LOCAL_UPGRADE_KEY)).toBeNull();
});

test('conflict counts as done', async () => {
	const storage = memoryStorage();
	markLocalUpgrade(storage);
	const { store } = makeStore(7);
	installLocalUpgrade(
		store,
		makeInvoke({
			local_profile_link: linkedTo(7),
			persona_llm_migrate_local: () => ({ status: 'conflict' }),
			persona_cache_migrate_local: () => ({ moved: 0, conflicts: 2 }),
		}),
		storage,
	);
	await flush();
	expect(readUpgradeMarker(storage)?.migrate).toBe(false);
	expect(upgradeState).toMatchObject({ phase: 'done', key: 'conflict' });
});

test('a failed link keeps the marker, runs no migration and retry recovers', async () => {
	const storage = memoryStorage();
	markLocalUpgrade(storage);
	const { store } = makeStore(7);
	let failLink = true;
	const invoke = makeInvoke({
		local_profile_link: (a) => {
			if (failLink) throw new Error('api-key sk-secret');
			return linkedTo(7)(a);
		},
		persona_llm_migrate_local: () => ({ status: 'none' }),
		persona_cache_migrate_local: () => ({ moved: 0, conflicts: 0 }),
	});
	const errors = jest.spyOn(console, 'error').mockImplementation(() => {});
	installLocalUpgrade(store, invoke, storage);
	await flush();
	expect(upgradeState.phase).toBe('failed');
	expect(readUpgradeMarker(storage)?.migrate).toBe(true);
	expect(commandsOf(invoke)).toEqual(['local_profile_link']);
	expect(JSON.stringify(errors.mock.calls)).not.toContain('sk-secret');
	failLink = false;
	await retryLocalUpgrade();
	expect(upgradeState.phase).toBe('done');
	expect(readUpgradeMarker(storage)?.migrate).toBe(false);
	errors.mockRestore();
});

test.each(['persona_llm_migrate_local', 'persona_cache_migrate_local'])(
	'a failing %s keeps the marker; the re-run repeats both and clears it',
	async (failing) => {
		const storage = memoryStorage();
		markLocalUpgrade(storage);
		const { store } = makeStore(7);
		let fail = true;
		const pass = (name: string, value: unknown) => () => {
			if (name === failing && fail) throw new Error('boom');
			return value;
		};
		const invoke = makeInvoke({
			local_profile_link: linkedTo(7),
			persona_llm_migrate_local: pass('persona_llm_migrate_local', {
				status: 'moved',
			}),
			persona_cache_migrate_local: pass('persona_cache_migrate_local', {
				moved: 1,
				conflicts: 0,
			}),
		});
		const errors = jest.spyOn(console, 'error').mockImplementation(() => {});
		installLocalUpgrade(store, invoke, storage);
		await flush();
		expect(upgradeState.phase).toBe('failed');
		expect(readUpgradeMarker(storage)?.migrate).toBe(true);
		fail = false;
		invoke.mockClear();
		await retryLocalUpgrade();
		expect(commandsOf(invoke)).toEqual([
			'local_profile_link',
			'persona_llm_migrate_local',
			'persona_cache_migrate_local',
		]);
		expect(readUpgradeMarker(storage)?.migrate).toBe(false);
		expect(upgradeState.phase).toBe('done');
		errors.mockRestore();
	},
);

test('account B: A linked, B signs in, neither migrate command runs for B', async () => {
	const storage = memoryStorage();
	markLocalUpgrade(storage);
	const { store, signIn } = makeStore();
	const invoke = makeInvoke({
		local_profile_link: linkedTo(7),
		persona_llm_migrate_local: () => ({ status: 'moved' }),
		persona_cache_migrate_local: () => ({ moved: 1, conflicts: 0 }),
	});
	installLocalUpgrade(store, invoke, storage);
	signIn(7);
	await flush();
	invoke.mockClear();
	markLocalUpgrade(storage);
	signIn(8);
	await flush();
	expect(commandsOf(invoke)).toEqual(['local_profile_link']);
	expect(invoke).not.toHaveBeenCalledWith(
		'persona_llm_migrate_local',
		expect.anything(),
	);
	expect(invoke).not.toHaveBeenCalledWith(
		'persona_cache_migrate_local',
		expect.anything(),
	);
	expect(readUpgradeMarker(storage)?.migrate).toBe(false);
	expect(upgradeState.key).toBe('other');
});

test.each([
	['absent', undefined],
	['null', null],
	['another id', 99],
])('linked_user_id %s fails closed', async (_label, linked) => {
	const storage = memoryStorage();
	markLocalUpgrade(storage);
	const { store } = makeStore(7);
	const invoke = makeInvoke({
		local_profile_link: linkedTo(linked),
		persona_llm_migrate_local: () => ({ status: 'moved' }),
		persona_cache_migrate_local: () => ({ moved: 1, conflicts: 0 }),
	});
	installLocalUpgrade(store, invoke, storage);
	await flush();
	expect(commandsOf(invoke)).toEqual(['local_profile_link']);
	expect(readUpgradeMarker(storage)?.migrate).toBe(false);
	expect(upgradeState.key).toBe('other');
});

test('no marker: no calls', async () => {
	const { store } = makeStore(7);
	const invoke = makeInvoke({});
	installLocalUpgrade(store, invoke, memoryStorage());
	await flush();
	expect(invoke).not.toHaveBeenCalled();
});

test.each([-1, 0])('user id %s: no calls and the marker stays', async (id) => {
	const storage = memoryStorage();
	markLocalUpgrade(storage);
	const { store } = makeStore();
	store.state.user = { id };
	const invoke = makeInvoke({});
	installLocalUpgrade(store, invoke, storage);
	await flush();
	expect(invoke).not.toHaveBeenCalled();
	expect(readUpgradeMarker(storage)).toEqual({ migrate: true, dialog: true });
});

test('the user loads later: the upgrade waits and then runs once', async () => {
	const storage = memoryStorage();
	markLocalUpgrade(storage);
	const { store, signIn } = makeStore();
	const invoke = makeInvoke({
		local_profile_link: linkedTo(5),
		persona_llm_migrate_local: () => ({ status: 'none' }),
		persona_cache_migrate_local: () => ({ moved: 0, conflicts: 0 }),
	});
	installLocalUpgrade(store, invoke, storage);
	await flush();
	expect(invoke).not.toHaveBeenCalled();
	signIn(5);
	signIn(5);
	await flush();
	expect(
		commandsOf(invoke).filter((c) => c === 'local_profile_link'),
	).toHaveLength(1);
});

test('the installer makes no HTTP calls', async () => {
	const storage = memoryStorage();
	markLocalUpgrade(storage);
	const { store } = makeStore(7);
	const fetchSpy = jest.fn();
	const original = (globalThis as any).fetch;
	(globalThis as any).fetch = fetchSpy;
	installLocalUpgrade(
		store,
		makeInvoke({
			local_profile_link: linkedTo(7),
			persona_llm_migrate_local: () => ({ status: 'moved' }),
			persona_cache_migrate_local: () => ({ moved: 1, conflicts: 0 }),
		}),
		storage,
	);
	await flush();
	(globalThis as any).fetch = original;
	expect(fetchSpy).not.toHaveBeenCalled();
});
