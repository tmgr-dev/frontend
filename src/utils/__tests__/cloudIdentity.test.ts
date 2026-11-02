import {
	installCloudIdentitySync,
	resyncCloudIdentity,
} from '../cloudIdentity';

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

const setup = (
	initial: { token?: string | null; user?: unknown } = {},
	invoke = jest.fn().mockResolvedValue(undefined),
) => {
	const listeners: Array<(m: { type: string; payload?: unknown }) => void> = [];
	const state: { token: { token: string } | null; user: unknown } = {
		token: initial.token ? { token: initial.token } : null,
		user: initial.user ?? null,
	};
	const store = {
		state,
		subscribe: (fn: (m: { type: string; payload?: unknown }) => void) => {
			listeners.push(fn);
		},
	};
	installCloudIdentitySync(store, invoke);
	const commit = (type: string, payload?: unknown) => {
		if (type === 'setToken')
			state.token = payload ? { token: payload as string } : null;
		if (type === 'setUser') state.user = payload;
		listeners.forEach((fn) => fn({ type, payload }));
	};
	return { invoke, commit };
};

describe('installCloudIdentitySync', () => {
	it('clears once at startup without a token', async () => {
		const { invoke } = setup();
		await flush();
		expect(invoke.mock.calls).toEqual([['local_identity_clear', undefined]]);
	});

	it('sets at startup when signed in with a user', async () => {
		const { invoke } = setup({ token: 't', user: { id: 4, name: 'Ann' } });
		await flush();
		expect(invoke.mock.calls).toEqual([
			['local_identity_set', { id: 4, name: 'Ann' }],
		]);
	});

	it('sets on sign-in and sends only id and name', async () => {
		const { invoke, commit } = setup();
		commit('setToken', 't');
		commit('setUser', { id: 5, name: 'Bob', email: 'b@x.y', token: 'secret' });
		await flush();
		const set = invoke.mock.calls.filter((c) => c[0] === 'local_identity_set');
		expect(set).toEqual([['local_identity_set', { id: 5, name: 'Bob' }]]);
		expect(Object.keys(set[0][1])).toEqual(['id', 'name']);
	});

	it('updates on account switch', async () => {
		const { invoke, commit } = setup({
			token: 't',
			user: { id: 1, name: 'A' },
		});
		commit('setUser', { id: 2, name: 'B' });
		await flush();
		expect(invoke.mock.calls.map((c) => c[1])).toEqual([
			{ id: 1, name: 'A' },
			{ id: 2, name: 'B' },
		]);
	});

	it('clears on logout and on session expiry, keeping order', async () => {
		const { invoke, commit } = setup({
			token: 't',
			user: { id: 1, name: 'A' },
		});
		commit('setToken', null);
		commit('setToken', 't2');
		commit('setUser', { id: 3, name: 'C' });
		commit('setToken', null);
		await flush();
		expect(invoke.mock.calls.map((c) => c[0])).toEqual([
			'local_identity_set',
			'local_identity_clear',
			'local_identity_set',
			'local_identity_clear',
		]);
	});

	it('does not set a user while there is no token', async () => {
		const { invoke, commit } = setup();
		commit('setUser', { id: 9, name: 'Z' });
		await flush();
		expect(invoke.mock.calls.map((c) => c[0])).toEqual([
			'local_identity_clear',
		]);
	});

	it('does not write on token rotation', async () => {
		const { invoke, commit } = setup({
			token: 't',
			user: { id: 1, name: 'A' },
		});
		commit('setToken', 't2');
		await flush();
		expect(invoke).toHaveBeenCalledTimes(1);
	});

	it('writes the same user once', async () => {
		const { invoke, commit } = setup({
			token: 't',
			user: { id: 1, name: 'A' },
		});
		commit('setUser', { id: 1, name: 'A', extra: 1 });
		await flush();
		expect(invoke).toHaveBeenCalledTimes(1);
	});

	it('ignores users without a positive id', async () => {
		const { invoke, commit } = setup({ token: 't' });
		commit('setUser', { id: 0, name: 'A' });
		commit('setUser', { name: 'A' });
		await flush();
		expect(invoke).not.toHaveBeenCalled();
	});

	it('logs invoke failures and never throws', async () => {
		const spy = jest
			.spyOn(console, 'error')
			.mockImplementation(() => undefined);
		const invoke = jest.fn().mockRejectedValue(new Error('boom'));
		const { commit } = setup(
			{ token: 't', user: { id: 1, name: 'A' } },
			invoke,
		);
		expect(() => commit('setUser', { id: 2, name: 'B' })).not.toThrow();
		await flush();
		expect(invoke).toHaveBeenCalledTimes(2);
		expect(spy).toHaveBeenCalled();
		spy.mockRestore();
	});

	describe('resyncCloudIdentity', () => {
		let spy: jest.SpyInstance;
		beforeEach(() => {
			spy = jest.spyOn(console, 'error').mockImplementation(() => undefined);
		});
		afterEach(() => spy.mockRestore());

		it('resends a set that was rejected', async () => {
			const invoke = jest
				.fn()
				.mockRejectedValueOnce(new Error('boom'))
				.mockResolvedValue(undefined);
			setup({ token: 't', user: { id: 1, name: 'A' } }, invoke);
			await flush();
			await resyncCloudIdentity();
			expect(invoke.mock.calls).toEqual([
				['local_identity_set', { id: 1, name: 'A' }],
				['local_identity_set', { id: 1, name: 'A' }],
			]);
		});

		it('resends a clear that was rejected', async () => {
			const invoke = jest
				.fn()
				.mockRejectedValueOnce(new Error('boom'))
				.mockResolvedValue(undefined);
			setup({}, invoke);
			await flush();
			await resyncCloudIdentity();
			expect(invoke.mock.calls).toEqual([
				['local_identity_clear', undefined],
				['local_identity_clear', undefined],
			]);
		});

		it('sends nothing when nothing failed', async () => {
			const { invoke } = setup({ token: 't', user: { id: 1, name: 'A' } });
			await flush();
			await resyncCloudIdentity();
			expect(invoke).toHaveBeenCalledTimes(1);
		});

		it('sends only the new account after a switch', async () => {
			const invoke = jest
				.fn()
				.mockRejectedValueOnce(new Error('boom'))
				.mockResolvedValue(undefined);
			const { commit } = setup(
				{ token: 't', user: { id: 1, name: 'A' } },
				invoke,
			);
			await flush();
			commit('setUser', { id: 2, name: 'B' });
			await resyncCloudIdentity();
			expect(invoke.mock.calls.map((c) => c[1])).toEqual([
				{ id: 1, name: 'A' },
				{ id: 2, name: 'B' },
			]);
		});

		it('resolves when sync is not installed', async () => {
			let fresh: typeof import('../cloudIdentity') | undefined;
			jest.isolateModules(() => {
				fresh = require('../cloudIdentity');
			});
			await expect(fresh!.resyncCloudIdentity()).resolves.toBeUndefined();
		});
	});
});
