import * as workspaceContext from '@/utils/workspaceContext';
import { readFileSync } from 'fs';
import { ModuleKind, transpileModule } from 'typescript';

const memoryStorage = () => {
	const map = new Map<string, string>();
	return {
		getItem: (key: string) => (map.has(key) ? map.get(key)! : null),
		setItem: (key: string, value: string) => void map.set(key, value),
		removeItem: (key: string) => void map.delete(key),
	};
};

function evaluate(
	path: string,
	dependencies: Record<string, unknown>,
	storages: { localStorage?: any; sessionStorage?: any } = {},
) {
	const source = transpileModule(readFileSync(require.resolve(path), 'utf8'), {
		compilerOptions: { module: ModuleKind.CommonJS },
	}).outputText;
	const loaded = { exports: {} as any };
	new Function(
		'require',
		'module',
		'exports',
		'localStorage',
		'sessionStorage',
		source,
	)(
		(name: string) => dependencies[name] ?? { default: {} },
		loaded,
		loaded.exports,
		storages.localStorage ?? memoryStorage(),
		storages.sessionStorage ?? memoryStorage(),
	);
	return loaded.exports;
}

function fixture(
	extraDeps: Record<string, unknown> = {},
	storages: { localStorage?: any; sessionStorage?: any } = {},
) {
	const getWorkspaces = jest.fn();
	const cache = {
		clear: jest.fn(),
		setContext: jest.fn(),
		invalidate: jest.fn(),
		clearInFlight: jest.fn(),
	};
	const config = evaluate(
		'../index.js',
		{
			vuex: { createStore: (options: any) => options },
			'@/actions/tmgr/workspaces': { getWorkspaces },
			'@/utils/requestCache': { requestCache: cache },
			'@/utils/workspaceContext': workspaceContext,
			'@/composable/usePusher': { disconnectRealtime: jest.fn() },
			...extraDeps,
		},
		storages,
	).default;
	return { config, getWorkspaces, cache };
}

const userWith = (settingsExtra: any[] = []) => ({
	id: 1,
	settings: [{ id: 5, key: 'current_workspace', value: 1 }, ...settingsExtra],
});

describe('setUser: per-tab workspace overlay', () => {
	test('with no stored preference, the tab adopts the server default', () => {
		const { config } = fixture();
		const { state, mutations } = config;
		mutations.setUser(state, userWith());
		expect(state.defaultWorkspaceId).toBe(1);
		expect(state.clientWorkspaceId).toBe(1);
		expect(state.userSettingsMap['current_workspace'].value).toBe(1);
	});

	test('sessionStorage wins over the server default', () => {
		const sessionStorage = memoryStorage();
		sessionStorage.setItem('tmgr:tabWorkspaceId', '2');
		const { config } = fixture({}, { sessionStorage });
		const { state, mutations } = config;
		state.workspaces = [{ id: 1 }, { id: 2 }];
		mutations.setUser(state, userWith());
		expect(state.defaultWorkspaceId).toBe(1);
		expect(state.clientWorkspaceId).toBe(2);
		expect(state.userSettingsMap['current_workspace'].value).toBe(2);
	});

	test('localStorage is used when sessionStorage is empty', () => {
		const localStorage = memoryStorage();
		localStorage.setItem('tmgr:lastWorkspaceId', '2');
		const { config } = fixture({}, { localStorage });
		const { state, mutations } = config;
		state.workspaces = [{ id: 1 }, { id: 2 }];
		mutations.setUser(state, userWith());
		expect(state.clientWorkspaceId).toBe(2);
	});

	test('a resolved client workspace is sticky across repeated setUser calls', () => {
		const { config } = fixture();
		const { state, mutations } = config;
		mutations.setUser(state, userWith());
		expect(state.clientWorkspaceId).toBe(1);
		// The server now reports a different default (another tab or device changed it);
		// this tab must not move.
		mutations.setUser(state, {
			id: 1,
			settings: [{ id: 5, key: 'current_workspace', value: 3 }],
		});
		expect(state.clientWorkspaceId).toBe(1);
		expect(state.defaultWorkspaceId).toBe(3);
		expect(state.userSettingsMap['current_workspace'].value).toBe(1);
	});

	test('a different user id (a cross-tab token swap that skips logout) resets the tab workspace', () => {
		const sessionStorage = memoryStorage();
		sessionStorage.setItem('tmgr:tabWorkspaceId', '1');
		const { config } = fixture({}, { sessionStorage });
		const { state, mutations } = config;
		mutations.setUser(state, userWith());
		expect(state.clientWorkspaceId).toBe(1);

		sessionStorage.setItem('tmgr:tabWorkspaceId', '9');
		mutations.setUser(state, {
			id: 2,
			settings: [{ id: 5, key: 'current_workspace', value: 9 }],
		});
		expect(state.clientWorkspaceId).toBe(9);
		expect(state.defaultWorkspaceId).toBe(9);
	});

	test('a negative (local) current_workspace value never becomes the default or an unresolved client id', () => {
		const { config } = fixture();
		const { state, mutations } = config;
		mutations.setUser(state, {
			id: 1,
			settings: [{ id: 5, key: 'current_workspace', value: -3 }],
		});
		expect(state.defaultWorkspaceId).toBeNull();
		expect(state.clientWorkspaceId).toBeNull();
	});

	test('readers off state.user.settings (not just userSettingsMap) see the overlay', () => {
		const { config } = fixture();
		const { state, mutations } = config;
		mutations.setUser(state, userWith());
		const setting = state.user.settings.find(
			(s: any) => s.key === 'current_workspace',
		);
		expect(setting.value).toBe(1);
	});
});

describe('setWorkspaces: re-validates a provisionally trusted client id', () => {
	test('falls back to the default when the stored id is not in the loaded list', () => {
		const sessionStorage = memoryStorage();
		sessionStorage.setItem('tmgr:tabWorkspaceId', '99');
		const { config, cache } = fixture({}, { sessionStorage });
		const { state, mutations } = config;
		mutations.setUser(state, userWith());
		expect(state.clientWorkspaceId).toBe(99);
		cache.setContext.mockClear();
		mutations.setWorkspaces(state, [{ id: 1 }, { id: 2 }]);
		expect(state.clientWorkspaceId).toBe(1);
		expect(state.userSettingsMap['current_workspace'].value).toBe(1);
		expect(sessionStorage.getItem('tmgr:tabWorkspaceId')).toBe('1');
		expect(cache.setContext).toHaveBeenLastCalledWith('1:1');
		expect(cache.invalidate).toHaveBeenCalled();
	});

	test('keeps a client id already present in the list', () => {
		const { config } = fixture();
		const { state, mutations } = config;
		mutations.setUser(state, userWith());
		mutations.setWorkspaces(state, [{ id: 1 }, { id: 2 }]);
		expect(state.clientWorkspaceId).toBe(1);
	});
});

describe('updateUserWorkspaceSetting: the local switch', () => {
	test('sets clientWorkspaceId and persists it, without touching defaultWorkspaceId', () => {
		const sessionStorage = memoryStorage();
		const localStorage = memoryStorage();
		const { config } = fixture({}, { sessionStorage, localStorage });
		const { state, mutations } = config;
		mutations.setUser(state, userWith());
		mutations.updateUserWorkspaceSetting(state, { workspaceId: 2 });
		expect(state.clientWorkspaceId).toBe(2);
		expect(state.defaultWorkspaceId).toBe(1);
		expect(state.userSettingsMap['current_workspace'].value).toBe(2);
		expect(sessionStorage.getItem('tmgr:tabWorkspaceId')).toBe('2');
		expect(localStorage.getItem('tmgr:lastWorkspaceId')).toBe('2');
	});

	test('invalidates workspace-scoped cache only when the workspace actually changes', () => {
		const { config, cache } = fixture();
		const { state, mutations } = config;
		mutations.setUser(state, userWith());
		cache.invalidate.mockClear();
		mutations.updateUserWorkspaceSetting(state, { workspaceId: 1 });
		expect(cache.invalidate).not.toHaveBeenCalled();
		mutations.updateUserWorkspaceSetting(state, { workspaceId: 2 });
		expect(cache.invalidate).toHaveBeenCalled();
	});
});

describe('logout: clears the tab and the remembered workspace', () => {
	test('resets defaultWorkspaceId, clientWorkspaceId, and both storages', () => {
		const sessionStorage = memoryStorage();
		const localStorage = memoryStorage();
		localStorage.setItem('theme', 'default');
		const { config } = fixture(
			{
				'@/local/personaCache': { clearPersonaLlmForLogout: jest.fn() },
				'@/utils/desktop': { isDesktopApp: () => false },
			},
			{ sessionStorage, localStorage },
		);
		const { state, mutations, actions } = config;
		mutations.setUser(state, userWith());
		mutations.updateUserWorkspaceSetting(state, { workspaceId: 2 });
		actions.logout({ commit: jest.fn(), state });
		expect(state.defaultWorkspaceId).toBeNull();
		expect(state.clientWorkspaceId).toBeNull();
		expect(sessionStorage.getItem('tmgr:tabWorkspaceId')).toBeNull();
		expect(localStorage.getItem('tmgr:lastWorkspaceId')).toBeNull();
	});
});
