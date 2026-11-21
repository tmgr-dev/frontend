/** @jest-environment jsdom */
const invoke = jest.fn();
jest.mock('@tauri-apps/api/core', () => ({
	invoke: (...args: unknown[]) => invoke(...args),
}));

import {
	clearRelayAuth,
	installRelayAuthSync,
	syncRelayAuth,
} from '../relayAuth';

const API = 'https://api.example/api/';
const SECRET = `a.${btoa(JSON.stringify({ sub: '7' }))}.b`;
const envelope = (token = SECRET) => JSON.stringify({ token });

const asDesktop = (on: boolean) => {
	if (on) (window as any).__TAURI_INTERNALS__ = {};
	else delete (window as any).__TAURI_INTERNALS__;
};

beforeEach(() => {
	invoke.mockReset();
	invoke.mockResolvedValue(undefined);
	localStorage.clear();
	asDesktop(true);
});
afterEach(() => asDesktop(false));

describe('syncRelayAuth', () => {
	it('passes exactly token, userId and apiBase', async () => {
		localStorage.setItem('token', envelope());
		await syncRelayAuth(API, 7);
		expect(invoke).toHaveBeenCalledTimes(1);
		expect(invoke).toHaveBeenCalledWith('manager_set_relay_auth', {
			token: SECRET,
			userId: 7,
			apiBase: API,
		});
	});

	it('takes the user id from the token subject when not given', async () => {
		const payload = btoa(JSON.stringify({ sub: '7' }));
		const token = `h.${payload}.s`;
		localStorage.setItem('token', envelope(token));
		await syncRelayAuth(API);
		expect(invoke.mock.calls[0][1]).toEqual({
			token,
			userId: 7,
			apiBase: API,
		});
	});

	it('does nothing outside the desktop shell', async () => {
		asDesktop(false);
		localStorage.setItem('token', envelope());
		await syncRelayAuth(API, 7);
		expect(invoke).not.toHaveBeenCalled();
	});

	it('does nothing without an api base', async () => {
		localStorage.setItem('token', envelope());
		await syncRelayAuth(undefined, 7);
		expect(invoke).not.toHaveBeenCalled();
	});

	it.each(['/api/', 'api', 'ftp://x/api/', ''])(
		'does not set for an unresolvable api base %p',
		async (base) => {
			localStorage.setItem('token', envelope());
			await syncRelayAuth(base, 7);
			expect(invoke).not.toHaveBeenCalled();
		},
	);

	it('does nothing without a token (local or signed-out)', async () => {
		await syncRelayAuth(API, 7);
		expect(invoke).not.toHaveBeenCalled();
	});

	it('does nothing without a user id', async () => {
		localStorage.setItem('token', envelope('opaque'));
		await syncRelayAuth(API);
		expect(invoke).not.toHaveBeenCalled();
	});

	it('swallows a missing command and never leaks the token', async () => {
		localStorage.setItem('token', envelope());
		invoke.mockRejectedValue(new Error(`command not found ${SECRET}`));
		const spies = (['log', 'warn', 'error', 'info'] as const).map((m) =>
			jest.spyOn(console, m).mockImplementation(() => undefined),
		);
		await expect(syncRelayAuth(API, 7)).resolves.toBeUndefined();
		const out = spies
			.flatMap((s) => s.mock.calls)
			.flat()
			.join(' ');
		expect(out).not.toContain(SECRET);
		spies.forEach((s) => s.mockRestore());
	});
});

describe('clearRelayAuth', () => {
	it('invokes the clear command without args', async () => {
		await clearRelayAuth();
		expect(invoke).toHaveBeenCalledWith('manager_clear_relay_auth');
	});

	it('does nothing outside the desktop shell and swallows errors', async () => {
		asDesktop(false);
		await clearRelayAuth();
		expect(invoke).not.toHaveBeenCalled();
		asDesktop(true);
		invoke.mockRejectedValue(new Error('x'));
		await expect(clearRelayAuth()).resolves.toBeUndefined();
	});
});

describe('invoke ordering', () => {
	it('a clear is never overtaken by an earlier slower set', async () => {
		localStorage.setItem('token', envelope());
		const order: string[] = [];
		let release: () => void = () => undefined;
		invoke.mockImplementationOnce(async (cmd: string) => {
			await new Promise<void>((r) => (release = r));
			order.push(cmd);
		});
		invoke.mockImplementation(async (cmd: string) => void order.push(cmd));
		const set = syncRelayAuth(API, 7);
		const clear = clearRelayAuth();
		const set2 = syncRelayAuth(API, 7);
		await new Promise((r) => setTimeout(r, 0));
		expect(order).toEqual([]);
		release();
		await Promise.all([set, clear, set2]);
		expect(order).toEqual([
			'manager_set_relay_auth',
			'manager_clear_relay_auth',
			'manager_set_relay_auth',
		]);
	});

	it('keeps serving later jobs after a failed one', async () => {
		localStorage.setItem('token', envelope());
		invoke.mockRejectedValueOnce(new Error('x'));
		await syncRelayAuth(API, 7);
		await clearRelayAuth();
		expect(invoke).toHaveBeenLastCalledWith('manager_clear_relay_auth');
	});
});

describe('installRelayAuthSync', () => {
	const setup = (initial: { token?: string; user?: number } = {}) => {
		const listeners: Array<(m: { type: string; payload?: unknown }) => void> =
			[];
		const state: any = {
			token: initial.token ? { token: initial.token } : null,
			user: initial.user ? { id: initial.user } : null,
		};
		if (initial.token) localStorage.setItem('token', envelope(initial.token));
		installRelayAuthSync(
			{
				state,
				subscribe: (fn) => void listeners.push(fn),
			},
			API,
		);
		const commit = (type: string, payload?: any) => {
			if (type === 'setToken') {
				state.token = payload;
				if (payload) localStorage.setItem('token', JSON.stringify(payload));
				else localStorage.removeItem('token');
			}
			if (type === 'setUser') state.user = payload;
			listeners.forEach((fn) => fn({ type, payload }));
		};
		return { commit };
	};
	const flush = () => new Promise((r) => setTimeout(r, 0));

	it('syncs at startup when signed in', async () => {
		setup({ token: SECRET, user: 7 });
		await flush();
		expect(invoke).toHaveBeenCalledWith(
			'manager_set_relay_auth',
			expect.objectContaining({ token: SECRET, userId: 7 }),
		);
	});

	it('syncs after login once the user is known and after refresh', async () => {
		const { commit } = setup();
		await flush();
		commit('setToken', { token: 'a' });
		await flush();
		expect(invoke).not.toHaveBeenCalled();
		commit('setUser', { id: 7 });
		await flush();
		expect(invoke).toHaveBeenCalledTimes(1);
		commit('setToken', { token: 'b' });
		await flush();
		expect(invoke).toHaveBeenCalledTimes(2);
		expect(invoke.mock.calls[1][1]).toMatchObject({ token: 'b', userId: 7 });
	});

	it('clears on logout', async () => {
		const { commit } = setup({ token: SECRET, user: 7 });
		await flush();
		invoke.mockClear();
		commit('setToken', null);
		await flush();
		expect(invoke).toHaveBeenCalledWith('manager_clear_relay_auth');
	});

	it('clears on account switch, then syncs the new user', async () => {
		const { commit } = setup({ token: SECRET, user: 7 });
		await flush();
		invoke.mockClear();
		commit('setUser', {});
		commit('setToken', { token: 'other' });
		await flush();
		expect(invoke.mock.calls).toEqual([['manager_clear_relay_auth']]);
		commit('setUser', { id: 8 });
		await flush();
		expect(invoke).toHaveBeenLastCalledWith(
			'manager_set_relay_auth',
			expect.objectContaining({ token: 'other', userId: 8 }),
		);
	});
});

describe('manager window', () => {
	it('does not push relay auth itself', () => {
		const src = require('fs').readFileSync(
			require('path').join(__dirname, '../App.vue'),
			'utf8',
		);
		expect(src).not.toMatch(/relayAuth|syncRelayAuth/);
	});
});
