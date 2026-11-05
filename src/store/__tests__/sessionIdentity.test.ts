import * as tokenSync from '@/utils/tokenSync';
import * as workspaceContext from '@/utils/workspaceContext';
import { readFileSync } from 'fs';
import { join } from 'path';
import { ModuleKind, transpileModule } from 'typescript';

const fakeJwt = (sub: unknown) =>
	['h', Buffer.from(JSON.stringify({ sub })).toString('base64url'), 'sig'].join(
		'.',
	);
const tokenFor = (sub: unknown) => ({ token: fakeJwt(sub) });

function fixture() {
	const cache = { clear: jest.fn(), setContext: jest.fn() };
	const disconnectRealtime = jest.fn();
	const deps: Record<string, unknown> = {
		vuex: { createStore: (options: any) => options },
		'@/actions/tmgr/workspaces': { getWorkspaces: jest.fn() },
		'@/utils/openDesktopLink': { DESKTOP_BAR_DISMISS_KEY: 'bar' },
		'@/utils/requestCache': { requestCache: cache },
		'@/utils/workspaceContext': workspaceContext,
		'@/utils/tokenSync': tokenSync,
		'@/composable/usePusher': { disconnectRealtime },
	};
	const source = transpileModule(
		readFileSync(require.resolve('../index.js'), 'utf8'),
		{ compilerOptions: { module: ModuleKind.CommonJS } },
	).outputText;
	const loaded = { exports: {} as any };
	const storage = {
		getItem: () => null,
		setItem: jest.fn(),
		removeItem: jest.fn(),
	};
	new Function('require', 'module', 'exports', 'localStorage', source)(
		(name: string) => deps[name] ?? { default: {} },
		loaded,
		loaded.exports,
		storage,
	);
	const { state, mutations } = loaded.exports.default;
	return { state, mutations, cache, disconnectRealtime };
}

const signedIn = (userId: number, token: unknown, extra: object = {}) => {
	const f = fixture();
	f.state.token = token;
	f.state.user = { id: userId, settings: [{ key: 'current_workspace' }] };
	f.state.userSettingsMap = { current_workspace: { value: 3 } };
	f.state.defaultWorkspaceId = 3;
	f.state.clientWorkspaceId = 3;
	f.state.sessionGeneration = 5;
	Object.assign(f.state, extra);
	f.cache.setContext.mockClear();
	f.cache.clear.mockClear();
	return f;
};

describe('setToken forgets the cloud user', () => {
	it('when the token goes to null', () => {
		const { state, mutations, cache } = signedIn(2, tokenFor('2'));
		mutations.setToken(state, null);
		expect(state.user).toEqual({});
		expect(state.userSettingsMap).toEqual({});
		expect(state.defaultWorkspaceId).toBeNull();
		expect(state.clientWorkspaceId).toBeNull();
		expect(cache.setContext).toHaveBeenLastCalledWith('guest');
		expect(cache.clear).toHaveBeenCalled();
		expect(state.sessionGeneration).toBe(6);
	});

	it('when another account takes over the token, and disconnects realtime', () => {
		const { state, mutations, cache, disconnectRealtime } = signedIn(
			2,
			tokenFor('2'),
		);
		mutations.setToken(state, tokenFor('5'));
		expect(state.user).toEqual({});
		expect(state.userSettingsMap).toEqual({});
		expect(state.defaultWorkspaceId).toBeNull();
		expect(state.clientWorkspaceId).toBeNull();
		expect(cache.setContext).toHaveBeenLastCalledWith('guest');
		expect(cache.clear).toHaveBeenCalled();
		expect(disconnectRealtime).toHaveBeenCalled();
		expect(state.sessionGeneration).toBe(6);
		expect(state.token).toEqual(tokenFor('5'));
	});

	it('keeps the user on a same-subject rotation', () => {
		const { state, mutations, disconnectRealtime } = signedIn(2, tokenFor('2'));
		mutations.setToken(state, { token: fakeJwt('2') + 'x' });
		expect(state.user.id).toBe(2);
		expect(state.sessionGeneration).toBe(5);
		expect(disconnectRealtime).not.toHaveBeenCalled();
	});

	it('keeps the user when the new subject is unreadable', () => {
		const { state, mutations, disconnectRealtime } = signedIn(2, tokenFor('2'));
		mutations.setToken(state, { token: 'opaque' });
		expect(state.user.id).toBe(2);
		expect(state.sessionGeneration).toBe(5);
		expect(disconnectRealtime).not.toHaveBeenCalled();
	});

	it('keeps a local profile user (id <= 0)', () => {
		const { state, mutations } = signedIn(-1, tokenFor('2'));
		mutations.setToken(state, tokenFor('5'));
		expect(state.user.id).toBe(-1);
		mutations.setToken(state, null);
		expect(state.user.id).toBe(-1);
		expect(state.userSettingsMap).toEqual({ current_workspace: { value: 3 } });
	});

	it('is a no-op when the token is already null', () => {
		const { state, mutations } = signedIn(2, null);
		mutations.setToken(state, null);
		expect(state.user.id).toBe(2);
		expect(state.sessionGeneration).toBe(5);
	});

	it('drops a stale user of another account when signing in from no token', () => {
		const { state, mutations } = signedIn(2, null);
		mutations.setToken(state, tokenFor('5'));
		expect(state.user).toEqual({});
		expect(state.sessionGeneration).toBe(6);
	});

	it('keeps the user when signing in from no token as the same account', () => {
		const { state, mutations } = signedIn(2, null);
		mutations.setToken(state, tokenFor('2'));
		expect(state.user.id).toBe(2);
	});
});

describe('setUser ignores a late response for the previous account', () => {
	it('ignores a mismatching positive id', () => {
		const { state, mutations } = signedIn(5, tokenFor('5'));
		mutations.setUser(state, { id: 2, settings: [] });
		expect(state.user.id).toBe(5);
		expect(state.sessionGeneration).toBe(5);
	});

	it('accepts a matching id', () => {
		const { state, mutations } = signedIn(5, tokenFor('5'), { user: {} });
		mutations.setUser(state, { id: 5, settings: [] });
		expect(state.user.id).toBe(5);
	});

	it('accepts any id when the subject is unreadable', () => {
		const { state, mutations } = signedIn(5, { token: 'opaque' });
		mutations.setUser(state, { id: 2, settings: [] });
		expect(state.user.id).toBe(2);
	});

	it('accepts any id when there is no token', () => {
		const { state, mutations } = signedIn(5, null);
		mutations.setUser(state, { id: 2, settings: [] });
		expect(state.user.id).toBe(2);
	});

	it('accepts a local profile payload under a token', () => {
		const { state, mutations } = signedIn(5, tokenFor('5'));
		mutations.setUser(state, { id: -1, settings: [] });
		expect(state.user.id).toBe(-1);
	});
});

describe('source pins', () => {
	const read = (path: string) =>
		readFileSync(join(__dirname, '../../', path), 'utf8');

	it('Workspaces.vue reads user settings with optional chaining', () => {
		expect(read('pages/Settings/Workspaces.vue')).not.toMatch(
			/store\.state\.user\.settings\.find/,
		);
	});

	it('hardLogout can skip the expiry notice', () => {
		const axios = read('plugins/axios.ts');
		expect(axios).toMatch(/hardLogout = async \(\{ expired = true \} = \{\}\)/);
		expect(axios).toMatch(/if \(expired && store\.state\.token\)/);
		expect(axios).toMatch(/export \{[^}]*hardLogout/);
	});
});
