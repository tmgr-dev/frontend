import {
	canSkipLocalUpgrade,
	clearUpgradeDialog,
	installLocalUpgrade,
	LOCAL_UPGRADE_KEY,
	markLocalUpgrade,
	readUpgradeMarker,
	retryLocalUpgrade,
	skipLocalUpgrade,
	upgradeFailureSentence,
	upgradeState,
} from '@/utils/localUpgrade';
import { readFileSync } from 'fs';
import { join } from 'path';

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
		state: {
			user: userId ? { id: userId } : null,
			token: userId ? 'token' : null,
			localProfile: {},
		} as any,
		commit: jest.fn(),
		subscribe: (fn: any) => void listeners.push(fn),
	};
	const signIn = (id: number) => {
		store.state.token = 'token';
		listeners.forEach((fn) => fn({ type: 'setToken', payload: 'token' }));
		store.state.user = { id };
		listeners.forEach((fn) => fn({ type: 'setUser', payload: { id } }));
	};
	const setToken = (token: string | null) => {
		store.state.token = token;
		listeners.forEach((fn) => fn({ type: 'setToken', payload: token }));
	};
	const setUser = (id: number) => {
		store.state.user = { id };
		listeners.forEach((fn) => fn({ type: 'setUser', payload: { id } }));
	};
	const signOut = () => setToken(null);
	return { store, signIn, signOut, setToken, setUser };
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

const firstWinsLink = () => {
	let linkedId: number | null = null;
	return (args: any) => {
		linkedId ??= args.userId;
		return linkedTo(linkedId)(args);
	};
};

const migrateIdsOf = (invoke: jest.Mock) =>
	invoke.mock.calls
		.filter((c) => c[0].endsWith('migrate_local'))
		.map((c) => c[1]?.toUserId);

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
			'persona_cache_migrate_local',
			'persona_llm_migrate_local',
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
	expect(readUpgradeMarker(storage)?.migrate).toBe(true);
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
	expect(readUpgradeMarker(storage)?.migrate).toBe(true);
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

test.each(['local_profile_link', 'persona_cache_migrate_local'])(
	'a user change during %s keeps the marker for the linked account',
	async (during) => {
		const storage = memoryStorage();
		markLocalUpgrade(storage);
		const { store, signIn } = makeStore(7);
		let switched = false;
		const switchUser = () => {
			if (switched) return;
			switched = true;
			signIn(8);
		};
		const link = firstWinsLink();
		const invoke = makeInvoke({
			local_profile_link: (a) => {
				const profile = link(a);
				if (during === 'local_profile_link') switchUser();
				return profile;
			},
			persona_cache_migrate_local: () => {
				if (during === 'persona_cache_migrate_local') switchUser();
				return { moved: 0, conflicts: 0 };
			},
			persona_llm_migrate_local: () => ({ status: 'moved' }),
		});
		installLocalUpgrade(store, invoke, storage);
		await flush();
		expect(migrateIdsOf(invoke)).not.toContain(8);
		expect(invoke).not.toHaveBeenCalledWith('persona_llm_migrate_local', {
			toUserId: 7,
		});
		expect(readUpgradeMarker(storage)?.migrate).toBe(true);
		expect(upgradeState.phase).not.toBe('running');
		expect(upgradeState.key).not.toBe('moved');

		invoke.mockClear();
		signIn(7);
		await flush();
		expect(migrateIdsOf(invoke)).toEqual([7, 7]);
		expect(readUpgradeMarker(storage)?.migrate).toBe(false);
		expect(upgradeState).toMatchObject({ phase: 'done', key: 'moved' });
	},
);

test('a user change during persona_llm_migrate_local settles but shows no key outcome to the new user', async () => {
	const storage = memoryStorage();
	markLocalUpgrade(storage);
	const { store, signIn } = makeStore(7);
	const invoke = makeInvoke({
		local_profile_link: firstWinsLink(),
		persona_cache_migrate_local: () => ({ moved: 1, conflicts: 0 }),
		persona_llm_migrate_local: () => {
			signIn(8);
			return { status: 'moved' };
		},
	});
	installLocalUpgrade(store, invoke, storage);
	await flush();
	expect(migrateIdsOf(invoke)).toEqual([7, 7]);
	expect(readUpgradeMarker(storage)?.migrate).toBe(false);
	expect(upgradeState.key).toBeNull();
	expect(upgradeState.phase).not.toBe('running');
	expect(upgradeState.phase).not.toBe('failed');
});

test.each(['local_profile_link', 'persona_cache_migrate_local'])(
	'a logout during %s stops the migration; the next sign-in finishes it',
	async (during) => {
		const storage = memoryStorage();
		markLocalUpgrade(storage);
		const { store, signIn, signOut } = makeStore(7);
		let out = false;
		const firstWins = firstWinsLink();
		const invoke = makeInvoke({
			local_profile_link: (a) => {
				const profile = firstWins(a);
				if (during === 'local_profile_link' && !out) {
					out = true;
					signOut();
				}
				return profile;
			},
			persona_cache_migrate_local: () => {
				if (during === 'persona_cache_migrate_local' && !out) {
					out = true;
					signOut();
				}
				return { moved: 0, conflicts: 0 };
			},
			persona_llm_migrate_local: () => ({ status: 'moved' }),
		});
		installLocalUpgrade(store, invoke, storage);
		await flush();
		expect(invoke).not.toHaveBeenCalledWith(
			'persona_llm_migrate_local',
			expect.anything(),
		);
		expect(readUpgradeMarker(storage)?.migrate).toBe(true);
		expect(upgradeState.phase).toBe('idle');
		expect(upgradeState.key).toBeNull();

		invoke.mockClear();
		signIn(7);
		await flush();
		expect(commandsOf(invoke)).toEqual([
			'local_profile_link',
			'persona_cache_migrate_local',
			'persona_llm_migrate_local',
		]);
		expect(readUpgradeMarker(storage)?.migrate).toBe(false);
		expect(upgradeState).toMatchObject({ phase: 'done', key: 'moved' });
	},
);

test('signed out with a user still in the store: no calls', async () => {
	const storage = memoryStorage();
	markLocalUpgrade(storage);
	const { store } = makeStore(7);
	store.state.token = null;
	const invoke = makeInvoke({});
	installLocalUpgrade(store, invoke, storage);
	await flush();
	expect(invoke).not.toHaveBeenCalled();
	expect(readUpgradeMarker(storage)?.migrate).toBe(true);
});

test.each([
	['local_profile_link', 'link'],
	['persona_cache_migrate_local', 'move'],
	['persona_llm_migrate_local', 'move'],
])('a failing %s records the %s step', async (failing, step) => {
	const storage = memoryStorage();
	markLocalUpgrade(storage);
	const { store } = makeStore(7);
	const handlers: Handlers = {
		local_profile_link: firstWinsLink(),
		persona_cache_migrate_local: () => ({ moved: 0, conflicts: 0 }),
		persona_llm_migrate_local: () => ({ status: 'none' }),
	};
	handlers[failing] = () => {
		throw new Error('boom');
	};
	const errors = jest.spyOn(console, 'error').mockImplementation(() => {});
	installLocalUpgrade(store, makeInvoke(handlers), storage);
	await flush();
	expect(upgradeState).toMatchObject({ phase: 'failed', failed: step });
	errors.mockRestore();
});

test('a failure after a user change is not shown to the new user', async () => {
	const storage = memoryStorage();
	markLocalUpgrade(storage);
	const { store, signIn } = makeStore(7);
	const invoke = makeInvoke({
		local_profile_link: firstWinsLink(),
		persona_cache_migrate_local: () => {
			signIn(8);
			throw new Error('boom');
		},
		persona_llm_migrate_local: () => ({ status: 'moved' }),
	});
	const errors = jest.spyOn(console, 'error').mockImplementation(() => {});
	installLocalUpgrade(store, invoke, storage);
	await flush();
	expect(upgradeState.phase).not.toBe('failed');
	expect(upgradeState.failed).toBeNull();
	expect(readUpgradeMarker(storage)?.migrate).toBe(true);
	errors.mockRestore();
});

test('failure copy never claims a key', () => {
	for (const step of ['link', 'move', null] as const) {
		expect(upgradeFailureSentence(step)).not.toMatch(/key/i);
	}
	expect(upgradeFailureSentence('link')).not.toMatch(/try again next time/);
});

test('skip clears the marker and the dialog without any command', async () => {
	const storage = memoryStorage();
	markLocalUpgrade(storage);
	const { store } = makeStore(7);
	const invoke = makeInvoke({
		local_profile_link: () => {
			throw new Error('unreadable profile link');
		},
	});
	const errors = jest.spyOn(console, 'error').mockImplementation(() => {});
	installLocalUpgrade(store, invoke, storage);
	await flush();
	expect(upgradeState.failed).toBe('link');
	invoke.mockClear();
	skipLocalUpgrade(storage);
	await retryLocalUpgrade();
	expect(invoke).not.toHaveBeenCalled();
	expect(storage.getItem(LOCAL_UPGRADE_KEY)).toBeNull();
	expect(upgradeState).toMatchObject({
		phase: 'idle',
		dialog: false,
		failed: null,
	});
	errors.mockRestore();
});

test('a finished outcome for 7 is not shown after switching to 8', async () => {
	const storage = memoryStorage();
	markLocalUpgrade(storage);
	const { store, signIn } = makeStore(7);
	const invoke = makeInvoke({
		local_profile_link: firstWinsLink(),
		persona_cache_migrate_local: () => ({ moved: 1, conflicts: 0 }),
		persona_llm_migrate_local: () => ({ status: 'moved' }),
	});
	installLocalUpgrade(store, invoke, storage);
	await flush();
	expect(upgradeState).toMatchObject({ phase: 'done', key: 'moved' });
	invoke.mockClear();
	signIn(8);
	await flush();
	expect(invoke).not.toHaveBeenCalled();
	expect(upgradeState.key).toBeNull();
	expect(upgradeState.phase).not.toBe('running');
});

test('a logout during persona_llm_migrate_local settles but shows nothing', async () => {
	const storage = memoryStorage();
	markLocalUpgrade(storage);
	const { store, signOut } = makeStore(7);
	installLocalUpgrade(
		store,
		makeInvoke({
			local_profile_link: firstWinsLink(),
			persona_cache_migrate_local: () => ({ moved: 0, conflicts: 0 }),
			persona_llm_migrate_local: () => {
				signOut();
				return { status: 'moved' };
			},
		}),
		storage,
	);
	await flush();
	expect(readUpgradeMarker(storage)?.migrate).toBe(false);
	expect(upgradeState).toMatchObject({
		phase: 'idle',
		key: null,
		failed: null,
	});
});

describe.each([
	['local_profile_link'],
	['persona_cache_migrate_local'],
	['persona_llm_migrate_local'],
])('a throwing %s after the session changed', (failing) => {
	const run = async (change: 'switch' | 'logout') => {
		const storage = memoryStorage();
		markLocalUpgrade(storage);
		const { store, signIn, signOut } = makeStore(7);
		const link = firstWinsLink();
		const handlers: Handlers = {
			local_profile_link: link,
			persona_cache_migrate_local: () => ({ moved: 0, conflicts: 0 }),
			persona_llm_migrate_local: () => ({ status: 'moved' }),
		};
		const pass = handlers[failing];
		let thrown = false;
		handlers[failing] = (a) => {
			if (thrown) return pass(a);
			thrown = true;
			if (failing === 'local_profile_link') link(a);
			if (change === 'switch') signIn(8);
			else signOut();
			throw new Error('boom');
		};
		const invoke = makeInvoke(handlers);
		const errors = jest.spyOn(console, 'error').mockImplementation(() => {});
		installLocalUpgrade(store, invoke, storage);
		await flush();
		errors.mockRestore();
		return { storage, invoke };
	};

	it('a switch to 8 shows no failure and runs no migrate for 8', async () => {
		const { storage, invoke } = await run('switch');
		expect(migrateIdsOf(invoke)).not.toContain(8);
		expect(upgradeState).toMatchObject({ phase: 'done', key: 'other' });
		expect(upgradeState.failed).toBeNull();
		expect(readUpgradeMarker(storage)?.migrate).toBe(true);
	});

	it('a logout leaves phase idle with no failure and keeps the marker', async () => {
		const { storage } = await run('logout');
		expect(upgradeState).toMatchObject({
			phase: 'idle',
			key: null,
			failed: null,
		});
		expect(readUpgradeMarker(storage)?.migrate).toBe(true);
	});
});

test('a switch to 8 during a retry after a link failure: nothing for 8, 7 finishes later', async () => {
	const storage = memoryStorage();
	markLocalUpgrade(storage);
	const { store, signIn } = makeStore(7);
	const link = firstWinsLink();
	let calls = 0;
	const invoke = makeInvoke({
		local_profile_link: (a) => {
			calls++;
			if (calls === 1) throw new Error('unreadable profile link');
			const profile = link(a);
			if (calls === 2) signIn(8);
			return profile;
		},
		persona_cache_migrate_local: () => ({ moved: 1, conflicts: 0 }),
		persona_llm_migrate_local: () => ({ status: 'moved' }),
	});
	const errors = jest.spyOn(console, 'error').mockImplementation(() => {});
	installLocalUpgrade(store, invoke, storage);
	await flush();
	expect(upgradeState.failed).toBe('link');
	await retryLocalUpgrade();
	await flush();
	expect(migrateIdsOf(invoke)).toEqual([]);
	expect(upgradeState.phase).not.toBe('running');
	expect(upgradeState.failed).toBeNull();
	expect(readUpgradeMarker(storage)?.migrate).toBe(true);
	signIn(7);
	await flush();
	expect(migrateIdsOf(invoke)).toEqual([7, 7]);
	expect(readUpgradeMarker(storage)?.migrate).toBe(false);
	errors.mockRestore();
});

test('a new token without its user yet: nothing runs for the previous user', async () => {
	const storage = memoryStorage();
	markLocalUpgrade(storage);
	const { store, signIn, setToken } = makeStore(7);
	const invoke = makeInvoke({
		local_profile_link: linkedTo(7),
		persona_cache_migrate_local: () => ({ moved: 1, conflicts: 0 }),
		persona_llm_migrate_local: () => ({ status: 'moved' }),
	});
	setToken(null);
	installLocalUpgrade(store, invoke, storage);
	setToken('token-8');
	await flush();
	await retryLocalUpgrade();
	expect(invoke).not.toHaveBeenCalled();
	signIn(8);
	await flush();
	expect(migrateIdsOf(invoke)).toEqual([]);
	expect(readUpgradeMarker(storage)?.migrate).toBe(true);
	expect(upgradeState.key).toBe('other');
});

test('a token swap during local_profile_link stops the old attempt before any migration', async () => {
	const storage = memoryStorage();
	markLocalUpgrade(storage);
	const { store, setToken } = makeStore(7);
	const invoke = makeInvoke({
		local_profile_link: (a) => {
			setToken('token-8');
			return linkedTo(7)(a);
		},
		persona_cache_migrate_local: () => ({ moved: 1, conflicts: 0 }),
		persona_llm_migrate_local: () => ({ status: 'moved' }),
	});
	installLocalUpgrade(store, invoke, storage);
	await flush();
	expect(commandsOf(invoke)).toEqual(['local_profile_link']);
	expect(readUpgradeMarker(storage)?.migrate).toBe(true);
	expect(upgradeState).toMatchObject({ phase: 'idle', key: null });
});

describe.each([['local_profile_link'], ['persona_cache_migrate_local']])(
	'a token refresh for the same user during %s',
	(during) => {
		const run = () => {
			const storage = memoryStorage();
			markLocalUpgrade(storage);
			const { store, setToken, setUser } = makeStore(7);
			let refreshed = false;
			const refresh = () => {
				if (refreshed) return;
				refreshed = true;
				setToken('token-refreshed');
			};
			const invoke = makeInvoke({
				local_profile_link: (a) => {
					const profile = linkedTo(7)(a);
					if (during === 'local_profile_link') refresh();
					return profile;
				},
				persona_cache_migrate_local: () => {
					if (during === 'persona_cache_migrate_local') refresh();
					return { moved: 1, conflicts: 0 };
				},
				persona_llm_migrate_local: () => ({ status: 'moved' }),
			});
			installLocalUpgrade(store, invoke, storage);
			return { storage, setUser, invoke };
		};

		it('stops quietly and keeps the marker', async () => {
			const { storage, invoke } = run();
			await flush();
			expect(invoke).not.toHaveBeenCalledWith(
				'persona_llm_migrate_local',
				expect.anything(),
			);
			expect(readUpgradeMarker(storage)?.migrate).toBe(true);
			expect(upgradeState).toMatchObject({
				phase: 'idle',
				key: null,
				failed: null,
			});
		});

		it('the next setUser for the same user finishes it', async () => {
			const { storage, setUser, invoke } = run();
			await flush();
			invoke.mockClear();
			setUser(7);
			await flush();
			expect(migrateIdsOf(invoke)).toEqual([7, 7]);
			expect(readUpgradeMarker(storage)?.migrate).toBe(false);
			expect(upgradeState).toMatchObject({ phase: 'done', key: 'moved' });
		});

		it('the next app start finishes it', async () => {
			const { storage } = run();
			await flush();
			const next = makeStore();
			next.store.state.token = 'token-refreshed';
			const invoke = makeInvoke({
				local_profile_link: linkedTo(7),
				persona_cache_migrate_local: () => ({ moved: 0, conflicts: 0 }),
				persona_llm_migrate_local: () => ({ status: 'moved' }),
			});
			installLocalUpgrade(next.store, invoke, storage);
			await flush();
			expect(invoke).not.toHaveBeenCalled();
			next.setUser(7);
			await flush();
			expect(migrateIdsOf(invoke)).toEqual([7, 7]);
			expect(readUpgradeMarker(storage)?.migrate).toBe(false);
		});
	},
);

test('a token refresh during persona_llm_migrate_local settles without a key line', async () => {
	const storage = memoryStorage();
	markLocalUpgrade(storage);
	const { store, setToken, setUser } = makeStore(7);
	const invoke = makeInvoke({
		local_profile_link: linkedTo(7),
		persona_cache_migrate_local: () => ({ moved: 1, conflicts: 0 }),
		persona_llm_migrate_local: () => {
			setToken('token-refreshed');
			return { status: 'moved' };
		},
	});
	installLocalUpgrade(store, invoke, storage);
	await flush();
	expect(readUpgradeMarker(storage)?.migrate).toBe(false);
	expect(upgradeState).toMatchObject({ phase: 'idle', key: null });
	invoke.mockClear();
	setUser(7);
	await flush();
	expect(invoke).not.toHaveBeenCalled();
});

test('boot: a refresh between setUser and the link call runs nothing until the next setUser', async () => {
	const storage = memoryStorage();
	markLocalUpgrade(storage);
	const { store, setToken, setUser } = makeStore();
	store.state.token = 'token';
	const invoke = makeInvoke({
		local_profile_link: linkedTo(7),
		persona_cache_migrate_local: () => ({ moved: 1, conflicts: 0 }),
		persona_llm_migrate_local: () => ({ status: 'moved' }),
	});
	installLocalUpgrade(store, invoke, storage);
	await flush();
	setUser(7);
	setToken('token-refreshed');
	await flush();
	expect(invoke).not.toHaveBeenCalled();
	expect(readUpgradeMarker(storage)?.migrate).toBe(true);
	setUser(7);
	await flush();
	expect(migrateIdsOf(invoke)).toEqual([7, 7]);
	expect(upgradeState).toMatchObject({ phase: 'done', key: 'moved' });
});

test.each(['persona_cache_migrate_local', 'persona_llm_migrate_local'])(
	'after a failing %s there is no skip and skip keeps the retry marker',
	async (failing) => {
		const storage = memoryStorage();
		markLocalUpgrade(storage);
		const { store } = makeStore(7);
		const handlers: Handlers = {
			local_profile_link: firstWinsLink(),
			persona_cache_migrate_local: () => ({ moved: 1, conflicts: 0 }),
			persona_llm_migrate_local: () => ({ status: 'moved' }),
		};
		handlers[failing] = () => {
			throw new Error('boom');
		};
		const errors = jest.spyOn(console, 'error').mockImplementation(() => {});
		installLocalUpgrade(store, makeInvoke(handlers), storage);
		await flush();
		expect(upgradeState.failed).toBe('move');
		expect(canSkipLocalUpgrade(upgradeState.failed)).toBe(false);
		skipLocalUpgrade(storage);
		expect(readUpgradeMarker(storage)).toEqual({ migrate: true, dialog: true });
		expect(upgradeState).toMatchObject({ phase: 'failed', failed: 'move' });
		errors.mockRestore();
	},
);

test('skip is offered only for a link failure', () => {
	expect(canSkipLocalUpgrade('link')).toBe(true);
	expect(canSkipLocalUpgrade('move')).toBe(false);
	expect(canSkipLocalUpgrade(null)).toBe(false);
	const host = readFileSync(
		join(__dirname, '../../components/desktop/LocalUpgradeHost.vue'),
		'utf8',
	);
	const skipButton = host.slice(
		host.lastIndexOf(
			'<Button',
			host.indexOf('data-testid="local-upgrade-skip"'),
		),
		host.indexOf('data-testid="local-upgrade-skip"'),
	);
	expect(skipButton).toContain('v-if="canSkip"');
	expect(host).toContain('canSkipLocalUpgrade(upgradeState.failed)');
});
