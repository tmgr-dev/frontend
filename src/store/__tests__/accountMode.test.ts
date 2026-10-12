import { readFileSync } from 'fs';
import { ModuleKind, transpileModule } from 'typescript';
import * as firstRun from '@/onboarding/localFirstRun';
import * as workspaceContext from '@/utils/workspaceContext';

function evaluate(path: string, dependencies: Record<string, unknown>, storage?: unknown) {
	const source = transpileModule(readFileSync(require.resolve(path), 'utf8'), {
		compilerOptions: { module: ModuleKind.CommonJS },
	}).outputText;
	const loaded = { exports: {} as any };
	new Function('require', 'module', 'exports', 'localStorage', source)(
		(name: string) => dependencies[name] ?? { default: {} },
		loaded,
		loaded.exports,
		storage ?? { getItem: () => null, setItem: jest.fn(), removeItem: jest.fn() },
	);
	return loaded.exports;
}

const PROFILE = { id: -1, uuid: 'u-1', name: 'Ada', created_at: '2026-10-11T10:00:00Z' };

function fixture(desktop = true) {
	const data = new Map<string, string>();
	const storage = {
		getItem: (key: string) => data.get(key) ?? null,
		setItem: (key: string, value: string) => void data.set(key, value),
		removeItem: (key: string) => void data.delete(key),
	};
	const mocks = {
		loadLocalProfile: jest.fn(),
		createLocalProfile: jest.fn(async () => PROFILE),
		listLocalWorkspaces: jest.fn(async () => [] as any[]),
		createLocalWorkspace: jest.fn(async (name: string) => ({ id: -5, name, code: 'my-workspace' })),
		setActiveLocalWorkspace: jest.fn(),
		push: jest.fn(async () => {}),
		invoke: jest.fn(async (_command: string) => [] as any[]),
	};
	const config = evaluate('../index.js', {
		vuex: { createStore: (options: any) => options },
		'@/actions/tmgr/workspaces': { getWorkspaces: jest.fn() },
		'@/utils/requestCache': { requestCache: { clear: jest.fn(), setContext: jest.fn(), invalidate: jest.fn() } },
		'@/utils/workspaceContext': workspaceContext,
		'@/composable/usePusher': { disconnectRealtime: jest.fn() },
		'@/utils/desktop': { isDesktopApp: () => desktop },
		'@/utils/localProfile': {
			loadLocalProfile: mocks.loadLocalProfile,
			createLocalProfile: mocks.createLocalProfile,
		},
		'@/local/runtime': {
			listLocalWorkspaces: mocks.listLocalWorkspaces,
			createLocalWorkspace: mocks.createLocalWorkspace,
			setActiveLocalWorkspace: mocks.setActiveLocalWorkspace,
		},
		'@tauri-apps/api/core': { invoke: mocks.invoke },
		'@/router': { default: { push: mocks.push } },
		'@/onboarding/localFirstRun': firstRun,
	}, storage).default;
	const getters = (state: any) => {
		const bound: any = {};
		for (const [key, fn] of Object.entries<any>(config.getters)) {
			Object.defineProperty(bound, key, { get: () => fn(state, bound), enumerable: true });
		}
		return bound;
	};
	return { config, mocks, getters, data };
}

describe('account mode', () => {
	it('is none with neither a token nor a profile', () => {
		const { config, getters } = fixture();
		const g = getters(config.state);
		expect(g.accountMode).toBe('none');
		expect(g.canUseApp).toBe(false);
		expect(g.isLocalOnly).toBe(false);
	});

	it('is local on desktop with a profile and no token', () => {
		const { config, getters } = fixture();
		config.mutations.setLocalProfile(config.state, PROFILE);
		const g = getters(config.state);
		expect(g.accountMode).toBe('local');
		expect(g.canUseApp).toBe(true);
		expect(g.isLocalOnly).toBe(true);
		expect(g.isLoggedIn).toBe(false);
	});

	it('flips to cloud by itself when a token appears', () => {
		const { config, getters } = fixture();
		config.mutations.setLocalProfile(config.state, PROFILE);
		config.mutations.setToken(config.state, { token: 'abc' });
		const g = getters(config.state);
		expect(g.accountMode).toBe('cloud');
		expect(g.isLocalOnly).toBe(false);
		expect(g.isLoggedIn).toBe(true);
	});

	it('ignores a profile on the web', () => {
		const { config, getters } = fixture(false);
		config.mutations.setLocalProfile(config.state, PROFILE);
		expect(getters(config.state).accountMode).toBe('none');
	});
});

describe('startWithoutAccount', () => {
	const run = async (fx: ReturnType<typeof fixture>, payload: any) => {
		const commit = jest.fn((name: string, value: any) => fx.config.mutations[name](fx.config.state, value));
		await fx.config.actions.startWithoutAccount({ commit, state: fx.config.state }, payload);
		return commit;
	};

	it('creates the profile and a first workspace, then opens the app', async () => {
		const fx = fixture();
		const commit = await run(fx, { name: 'Ada' });

		expect(fx.mocks.createLocalProfile).toHaveBeenCalledWith('Ada');
		expect(commit).toHaveBeenCalledWith('setLocalProfile', PROFILE);
		expect(fx.mocks.createLocalWorkspace).toHaveBeenCalledWith('My workspace');
		expect(fx.mocks.setActiveLocalWorkspace).toHaveBeenCalledWith(expect.objectContaining({ id: -5 }));
		expect(fx.mocks.push).toHaveBeenCalledWith('/app');
	});

	it('keeps the workspaces that already exist', async () => {
		const fx = fixture();
		fx.mocks.listLocalWorkspaces.mockResolvedValue([{ id: -9, name: 'Old', code: 'old' }]);
		await run(fx, { name: 'Ada' });

		expect(fx.mocks.createLocalWorkspace).not.toHaveBeenCalled();
		expect(fx.mocks.setActiveLocalWorkspace).toHaveBeenCalledWith(expect.objectContaining({ id: -9 }));
	});
});

describe('startWithoutAccount fresh-install marker', () => {
	const MARKER = 'tmgr:onboarding:local-fresh';
	const start = async (fx: ReturnType<typeof fixture>) => {
		const commit = jest.fn((name: string, value: any) => fx.config.mutations[name](fx.config.state, value));
		await fx.config.actions.startWithoutAccount({ commit, state: fx.config.state }, { name: 'Ada' });
	};

	it('writes the marker on a truly fresh install', async () => {
		const fx = fixture();
		await start(fx);
		expect(JSON.parse(fx.data.get(MARKER)!)).toMatchObject({ code: 'my-workspace' });
	});

	it.each([
		['a profile in the app state', (fx: ReturnType<typeof fixture>) => (fx.config.state.localProfile = PROFILE)],
		['a profile on disk', (fx: ReturnType<typeof fixture>) => fx.mocks.loadLocalProfile.mockResolvedValue(PROFILE)],
		['an unreadable profile', (fx: ReturnType<typeof fixture>) => fx.mocks.loadLocalProfile.mockRejectedValue(new Error('x'))],
		[
			'an existing local workspace',
			(fx: ReturnType<typeof fixture>) => fx.mocks.invoke.mockResolvedValue([{ id: -9, code: 'old' }]),
		],
		[
			'a failing workspace listing',
			(fx: ReturnType<typeof fixture>) => fx.mocks.invoke.mockRejectedValue(new Error('ipc down')),
		],
		[
			'a listing that is not an array',
			(fx: ReturnType<typeof fixture>) => fx.mocks.invoke.mockResolvedValue(null as any),
		],
		[
			'a starter_seeded_at in the onboarding state',
			(fx: ReturnType<typeof fixture>) =>
				fx.data.set('tmgr:onboarding:v1', JSON.stringify({ starter_seeded_at: '2026-10-12T10:00:00Z' })),
		],
	])('never writes the marker for %s', async (_name, arrange) => {
		const fx = fixture();
		arrange(fx);
		await start(fx);
		expect(fx.data.has(MARKER)).toBe(false);
	});
});

describe('startWithoutAccount stale marker', () => {
	const MARKER = 'tmgr:onboarding:local-fresh';
	const stale = JSON.stringify({ code: 'old', at: '2026-10-01T10:00:00.000Z' });
	const start = async (fx: ReturnType<typeof fixture>) => {
		const commit = jest.fn((name: string, value: any) => fx.config.mutations[name](fx.config.state, value));
		await fx.config.actions.startWithoutAccount({ commit, state: fx.config.state }, { name: 'Ada' });
	};

	it('clears a stale marker when the install is not fresh', async () => {
		const fx = fixture();
		fx.data.set(MARKER, stale);
		fx.mocks.invoke.mockResolvedValue([{ id: -9, code: 'old' }]);
		await start(fx);
		expect(fx.data.has(MARKER)).toBe(false);
	});

	it('replaces the marker with the new workspace on a fresh install', async () => {
		const fx = fixture();
		fx.data.set(MARKER, stale);
		await start(fx);
		expect(JSON.parse(fx.data.get(MARKER)!).code).toBe('my-workspace');
	});

	it('decides freshness from the backend list, not the cached runtime list', async () => {
		const fx = fixture();
		fx.mocks.listLocalWorkspaces.mockResolvedValue([{ id: -9, code: 'old' }]);
		await start(fx);
		expect(fx.mocks.invoke).toHaveBeenCalledWith('local_workspaces_list');
		expect(JSON.parse(fx.data.get(MARKER)!).code).toBe('old');
	});
});

describe('refreshLocalProfile', () => {
	it('loads the profile into the store', async () => {
		const fx = fixture();
		fx.mocks.loadLocalProfile.mockResolvedValue(PROFILE);
		const commit = jest.fn();
		await fx.config.actions.refreshLocalProfile({ commit });
		expect(commit).toHaveBeenCalledWith('setLocalProfile', PROFILE);
	});

	it('treats a failing read as no profile', async () => {
		const fx = fixture();
		fx.mocks.loadLocalProfile.mockRejectedValue(new Error('boom'));
		const commit = jest.fn();
		await fx.config.actions.refreshLocalProfile({ commit });
		expect(commit).toHaveBeenCalledWith('setLocalProfile', null);
	});
});
