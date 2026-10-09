import featureToggles from '../modules/featureToggles';

jest.mock('@/actions/tmgr/featureToggles', () => ({}));
jest.mock('@/actions/tmgr/modules', () => ({}));

const store = featureToggles as any;
const storage = new Map<string, string>();

const root = (id: number | null = 5) => ({ currentWorkspaceId: id });
const state = (patch: Record<string, unknown> = {}) => ({
	...store.state(),
	...patch,
});
const enabled = (s: any, key: string, id: number | null = 5) =>
	store.getters.isFeatureEnabled(s, {}, {}, root(id))(key);

beforeEach(() => {
	storage.clear();
	(global as any).localStorage = {
		getItem: (k: string) => storage.get(k) ?? null,
		setItem: (k: string, v: string) => void storage.set(k, v),
	};
});

afterEach(() => {
	delete (global as any).localStorage;
});

describe('isFeatureEnabled', () => {
	it('shows everything while loading with nothing cached', () => {
		expect(enabled(state(), 'pages')).toBe(true);
	});

	it('uses the cached payload while loading', () => {
		storage.set(
			'tmgr:modules:5',
			JSON.stringify({ pages: { key: 'pages', enabled: false } }),
		);
		expect(enabled(state(), 'pages')).toBe(false);
		expect(enabled(state(), 'board')).toBe(true);
	});

	it('uses the loaded map once loaded, ignoring the cache', () => {
		storage.set(
			'tmgr:modules:5',
			JSON.stringify({ pages: { key: 'pages', enabled: false } }),
		);
		const s = state({
			workspaceLoaded: true,
			workspaceToggles: { pages: { enabled: true } },
		});
		expect(enabled(s, 'pages')).toBe(true);
	});

	it('hides a loaded module on enabled false', () => {
		const s = state({
			workspaceLoaded: true,
			workspaceToggles: { pages: { enabled: false } },
		});
		expect(enabled(s, 'pages')).toBe(false);
	});

	it('hides a loaded module the user hid', () => {
		const s = state({
			workspaceLoaded: true,
			workspaceToggles: { board: { enabled: true, core: true, hidden: true } },
		});
		expect(enabled(s, 'board')).toBe(false);
	});

	it('shows an unknown key', () => {
		const s = state({
			workspaceLoaded: true,
			workspaceToggles: { pages: { enabled: false } },
		});
		expect(enabled(s, 'something.new')).toBe(true);
	});

	it('reads user-scoped keys from the user toggles', () => {
		const s = state({
			userLoaded: true,
			userToggles: { 'ui.tooltips': { value: false } },
		});
		expect(enabled(s, 'ui.tooltips')).toBe(false);
	});

	it('ignores toggles loaded for another workspace', () => {
		storage.set(
			'tmgr:modules:6',
			JSON.stringify({ pages: { key: 'pages', enabled: false } }),
		);
		const s = state({
			workspaceLoaded: true,
			loadedWorkspaceId: 5,
			workspaceToggles: { pages: { enabled: true } },
		});
		expect(enabled(s, 'pages', 6)).toBe(false);
	});
});

describe('setWorkspaceModules', () => {
	it('stores the map, the meta and the cache', () => {
		const s = state();
		store.mutations.setWorkspaceModules(s, {
			workspaceId: 5,
			payload: {
				configured: false,
				canManage: true,
				enforcement: 'report',
				packs: [],
				presets: [],
				modules: [{ key: 'pages', enabled: false }],
			},
		});
		expect(s.workspaceLoaded).toBe(true);
		expect(s.modulesMeta.configured).toBe(false);
		expect(s.workspaceToggles.pages.enabled).toBe(false);
		expect(JSON.parse(storage.get('tmgr:modules:5')!)).toEqual({
			pages: { key: 'pages', enabled: false },
		});
	});

	it('does not touch the cache for a legacy map', () => {
		const s = state();
		store.mutations.setWorkspaceToggles(s, { pages: { enabled: false } });
		expect(s.workspaceLoaded).toBe(true);
		expect(storage.size).toBe(0);
	});
});

describe('canManageModules', () => {
	const can = (s: any, rootState: any = {}, rootGetters: any = {}) =>
		store.getters.canManageModules(s, {}, rootState, rootGetters);

	it('trusts the server flag when modules were served', () => {
		expect(can(state({ modulesMeta: { canManage: true } }))).toBe(true);
		expect(
			can(
				state({ modulesMeta: { canManage: false } }),
				{ user: { id: 1 } },
				{ currentWorkspace: { user_id: 1 } },
			),
		).toBe(false);
	});

	it('falls back to workspace ownership for an old backend', () => {
		expect(
			can(state(), { user: { id: 1 } }, { currentWorkspace: { user_id: 1 } }),
		).toBe(true);
		expect(
			can(state(), { user: { id: 2 } }, { currentWorkspace: { user_id: 1 } }),
		).toBe(false);
		expect(can(state(), { user: { id: 2 } }, {})).toBe(false);
	});
});
