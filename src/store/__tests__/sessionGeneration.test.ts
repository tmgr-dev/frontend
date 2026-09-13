import { readFileSync } from 'fs';
import { ModuleKind, transpileModule } from 'typescript';

const deferred = () => {
	let resolve!: (value: any) => void;
	const promise = new Promise<any>((done) => {
		resolve = done;
	});
	return { promise, resolve };
};
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
function fixture() {
	const getWorkspaces = jest.fn();
	const cache = { clear: jest.fn(), setContext: jest.fn() };
	const config = evaluate('../index.js', {
		vuex: { createStore: (options: any) => options },
		'@/actions/tmgr/workspaces': { getWorkspaces },
		'@/utils/requestCache': { requestCache: cache },
		'@/composable/usePusher': { disconnectRealtime: jest.fn() },
	}).default;
	return { config, getWorkspaces };
}

test('session identity advances on login/logout/user replacement but survives refresh', () => {
	const { config } = fixture();
	const { state, mutations } = config;
	expect(state.sessionGeneration).toBe(0);
	mutations.setToken(state, { token: 'first' });
	expect(state.sessionGeneration).toBe(1);
	mutations.setToken(state, { token: 'rotated' });
	expect(state.sessionGeneration).toBe(1);
	mutations.setUser(state, { id: 1, settings: [] });
	expect(state.sessionGeneration).toBe(2);
	mutations.setUser(state, { id: 1, settings: [] });
	expect(state.sessionGeneration).toBe(2);
	mutations.setToken(state, null);
	expect(state.sessionGeneration).toBe(3);
	mutations.setToken(state, null);
	expect(state.sessionGeneration).toBe(3);
	mutations.setToken(state, { token: 'new-login' });
	expect(state.sessionGeneration).toBe(4);
	mutations.setUser(state, { id: 2, settings: [] });
	expect(state.sessionGeneration).toBe(5);
});

test.each(['refresh', 'session'])(
	'workspace list ownership handles %s',
	async (change) => {
		const { config, getWorkspaces } = fixture();
		const state = config.state;
		state.token = { token: 'old' };
		state.user = { id: 1 };
		state.sessionGeneration = 1;
		const response = deferred(),
			commit = jest.fn();
		getWorkspaces.mockReturnValueOnce(response.promise);
		const pending = config.actions.loadWorkspaces({ state, commit });
		if (change === 'refresh')
			config.mutations.setToken(state, { token: 'new' });
		else {
			config.mutations.setToken(state, null);
			config.mutations.setToken(state, { token: 'new' });
		}
		response.resolve([{ id: 3 }]);
		await pending;
		if (change === 'refresh')
			expect(commit).toHaveBeenCalledWith('setWorkspaces', [{ id: 3 }]);
		else expect(commit).not.toHaveBeenCalled();
	},
);

test.each(['refresh', 'session'])(
	'workspace status ownership handles %s',
	async (change) => {
		const state = {
			user: { id: 1 },
			token: { token: 'old' },
			sessionGeneration: 1,
		};
		const store = {
			state,
			getters: { currentWorkspaceId: 2 },
			commit: jest.fn(),
		};
		const response = deferred();
		const actions = evaluate('../../actions/tmgr/workspaces.ts', {
			'@/plugins/axios': { default: { get: () => response.promise } },
			'@/store': { default: store },
			'@/utils/requestCache': {
				requestCache: {
					getOrFetch: (_: string, fetch: () => unknown) => fetch(),
				},
			},
		});
		const pending = actions.getWorkspaceStatuses();
		state.token = { token: 'new' };
		if (change === 'session') state.sessionGeneration++;
		response.resolve({ data: { data: [{ id: 9 }] } });
		await pending;
		if (change === 'refresh')
			expect(store.commit).toHaveBeenCalledWith('setWorkspaceStatuses', [
				{ id: 9 },
			]);
		else expect(store.commit).not.toHaveBeenCalled();
	},
);
