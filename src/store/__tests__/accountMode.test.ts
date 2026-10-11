import { readFileSync } from 'fs';
import { ModuleKind, transpileModule } from 'typescript';
import * as workspaceContext from '@/utils/workspaceContext';

function evaluate(path: string, dependencies: Record<string, unknown>) {
	const source = transpileModule(readFileSync(require.resolve(path), 'utf8'), {
		compilerOptions: { module: ModuleKind.CommonJS },
	}).outputText;
	const loaded = { exports: {} as any };
	new Function('require', 'module', 'exports', 'localStorage', source)(
		(name: string) => dependencies[name] ?? { default: {} },
		loaded,
		loaded.exports,
		{ getItem: () => null, setItem: jest.fn(), removeItem: jest.fn() },
	);
	return loaded.exports;
}

const PROFILE = { id: -1, uuid: 'u-1', name: 'Ada', created_at: '2026-10-11T10:00:00Z' };

function fixture(desktop = true) {
	const mocks = {
		loadLocalProfile: jest.fn(),
		createLocalProfile: jest.fn(async () => PROFILE),
		listLocalWorkspaces: jest.fn(async () => [] as any[]),
		createLocalWorkspace: jest.fn(async (name: string) => ({ id: -5, name, code: 'my-workspace' })),
		setActiveLocalWorkspace: jest.fn(),
		push: jest.fn(async () => {}),
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
		'@/router': { default: { push: mocks.push } },
	}).default;
	const getters = (state: any) => {
		const bound: any = {};
		for (const [key, fn] of Object.entries<any>(config.getters)) {
			Object.defineProperty(bound, key, { get: () => fn(state, bound), enumerable: true });
		}
		return bound;
	};
	return { config, mocks, getters };
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
