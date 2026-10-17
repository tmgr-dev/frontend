import * as moduleSurfaces from '@/utils/moduleSurfaces';
import { readFileSync } from 'fs';
import { ModuleKind, transpileModule } from 'typescript';

type Guard = (to: any, from: any, next: jest.Mock) => Promise<unknown>;

const loadGuard = (store: any, desktop: boolean) => {
	let guard!: Guard;
	const source = transpileModule(
		readFileSync(require.resolve('../index.js'), 'utf8')
			.replaceAll('import.meta.env.MODE', "'test'")
			.replaceAll('import.meta.env.VUE_ROUTER_BASE', 'undefined'),
		{ compilerOptions: { module: ModuleKind.CommonJS } },
	).outputText;
	const loaded = { exports: {} as any };
	const dependencies: Record<string, unknown> = {
		'@/actions/tmgr/user': { getUser: jest.fn() },
		'@/actions/tmgr/workspaces': { getWorkspaces: jest.fn() },
		'@/utils/desktop': { isDesktopApp: () => desktop },
		'@/utils/emailVerification': { saveReturnPath: jest.fn() },
		'@/utils/localWorkspaceSync': { syncActiveLocalWorkspace: jest.fn() },
		'@/utils/memberPageScroll': { scrollBehavior: jest.fn() },
		'@/utils/moduleSurfaces': moduleSurfaces,
		'vue-router': {
			createRouter: () => ({
				beforeEach: (fn: Guard) => {
					guard = fn;
				},
			}),
			createWebHistory: () => ({}),
		},
		'../store': { default: store },
		'./routes': { default: [] },
	};
	new Function('require', 'module', 'exports', source)(
		(name: string) => dependencies[name],
		loaded,
		loaded.exports,
	);
	return guard;
};

const route = (
	name: string,
	path: string,
	meta: Record<string, unknown> = {},
	params = {},
) => ({
	name,
	path,
	params,
	fullPath: path,
	matched: [{ meta }],
});

const localStore = (overrides: Record<string, unknown> = {}) => ({
	getters: {
		canUseApp: true,
		isLoggedIn: false,
		accountMode: 'local',
		'featureToggles/getUserFeatureValue': () => 'board',
		'featureToggles/isFeatureEnabled': () => true,
		...overrides,
	},
	state: {
		user: { id: -1, settings: [{ key: 'current_workspace', value: -5 }] },
		workspaces: [{ id: -5, code: 'local-my-workspace' }],
		featureToggles: { userLoaded: true, workspaceLoaded: true },
	},
	commit: jest.fn(),
	dispatch: jest.fn(),
});

describe('router guard without a cloud account', () => {
	const privacyMeta = { allowedGuests: true, notOnlyForLoggedUsers: true };

	it.each([
		[
			'a guest on the web',
			localStore({ canUseApp: false, accountMode: 'none' }),
			false,
		],
		[
			'a signed-in user',
			localStore({ isLoggedIn: true, accountMode: 'cloud' }),
			false,
		],
		['a local-mode desktop session', localStore(), true],
		[
			'a desktop session with no identity',
			localStore({ canUseApp: false, accountMode: 'none' }),
			true,
		],
	])('opens the privacy page for %s', async (_label, store, desktop) => {
		const next = jest.fn();

		await loadGuard(store, desktop)(
			route('Privacy', '/privacy', privacyMeta),
			{},
			next,
		);

		expect(next).toHaveBeenCalledWith();
	});

	it('sends a desktop session with no identity to the welcome screen', async () => {
		const next = jest.fn();
		const store = localStore({ canUseApp: false, accountMode: 'none' });

		await loadGuard(store, true)(route('Tasks', '/tasks'), {}, next);

		expect(next).toHaveBeenCalledWith({ name: 'Welcome' });
	});

	it('keeps sending the web to login', async () => {
		const next = jest.fn();
		const store = localStore({ canUseApp: false, accountMode: 'none' });

		await loadGuard(store, false)(route('Tasks', '/tasks'), {}, next);

		expect(next).toHaveBeenCalledWith({ name: 'Login' });
	});

	it('lets the welcome screen through while there is nothing to open', async () => {
		const next = jest.fn();
		const store = localStore({ canUseApp: false, accountMode: 'none' });

		await loadGuard(store, true)(
			route('Welcome', '/welcome', { allowedGuests: true }),
			{},
			next,
		);

		expect(next).toHaveBeenCalledWith();
	});

	it('keeps the welcome screen off the web', async () => {
		const next = jest.fn();
		const store = localStore({ canUseApp: false, accountMode: 'none' });

		await loadGuard(store, false)(
			route('Welcome', '/welcome', { allowedGuests: true }),
			{},
			next,
		);

		expect(next).toHaveBeenCalledWith({ name: 'Login' });
	});

	it('leaves the welcome screen once the app can be used', async () => {
		const next = jest.fn();

		await loadGuard(localStore(), true)(
			route('Welcome', '/welcome', { allowedGuests: true }),
			{},
			next,
		);

		expect(next).toHaveBeenCalledWith('/app');
	});

	it('opens the active local workspace and its landing page at the root', async () => {
		const next = jest.fn();

		await loadGuard(localStore(), true)(route('Dashboard', '/app'), {}, next);

		expect(next).toHaveBeenCalledWith('/local-my-workspace/board');
	});

	it('keeps login and register reachable', async () => {
		const next = jest.fn();

		await loadGuard(localStore(), true)(
			route('Login', '/login', { allowedGuests: true }),
			{},
			next,
		);

		expect(next).toHaveBeenCalledWith();
	});

	it('still sends a signed-in user away from login', async () => {
		const next = jest.fn();
		const store = localStore({ isLoggedIn: true, accountMode: 'cloud' });

		await loadGuard(store, true)(
			route('Login', '/login', { allowedGuests: true }),
			{},
			next,
		);

		expect(next).toHaveBeenCalledWith({ name: 'CurrentTasksList' });
	});
});
