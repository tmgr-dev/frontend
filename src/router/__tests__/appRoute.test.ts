import { readFileSync } from 'fs';
import { ModuleKind, transpileModule } from 'typescript';

const loadRoutes = (canUseApp: boolean, workspaceCode?: string) => {
	const source = transpileModule(
		readFileSync(require.resolve('../routes.js'), 'utf8'),
		{ compilerOptions: { module: ModuleKind.CommonJS, allowJs: true } },
	).outputText;
	const loaded = { exports: {} as any };
	const store = {
		getters: { canUseApp },
		state: {
			user: { settings: [{ key: 'current_workspace', value: 7 }] },
			workspaces: workspaceCode ? [{ id: 7, code: workspaceCode }] : [],
		},
	};
	(globalThis as any).window = { app: { $store: store } };
	const dependencies: Record<string, unknown> = {
		'@/router/moduleGate': { gatedPage: (_k: string, _t: string, f: unknown) => f },
		'@/store': { default: { getters: {} } },
		'@/utils/simpleStatus': { boardRedirect: () => undefined, isSimpleMode: () => false },
	};
	new Function('require', 'module', 'exports', source)(
		(name: string) => dependencies[name],
		loaded,
		loaded.exports,
	);
	return loaded.exports.default as any[];
};

const find = (routes: any[], path: string) => routes.find((r) => r.path === path);

describe('app home route', () => {
	it('keeps the Dashboard record on /app with the old root meta', () => {
		const app = find(loadRoutes(true), '/app');
		expect(app.name).toBe('Dashboard');
		expect(app.meta).toMatchObject({
			title: 'Dashboard',
			navbarHidden: true,
			requiresAuth: true,
		});
		expect(typeof app.component).toBe('function');
		expect(typeof app.beforeEnter).toBe('function');
	});

	it('redirects /app to the current workspace dashboard keeping the query', () => {
		const app = find(loadRoutes(true, 'tmgrdev'), '/app');
		expect(app.beforeEnter({ query: { a: '1' } })).toEqual({
			path: '/tmgrdev/dashboard',
			query: { a: '1' },
		});
	});

	it('does not redirect /app when the app cannot be used', () => {
		const app = find(loadRoutes(false, 'tmgrdev'), '/app');
		expect(app.beforeEnter({ query: {} })).toBeUndefined();
	});

	it('redirects / to /app preserving query and hash', () => {
		const root = find(loadRoutes(true), '/');
		expect(root.name).toBeUndefined();
		expect(
			root.redirect({ query: { a: '1' }, hash: '#x', params: {} }),
		).toEqual({ path: '/app', query: { a: '1' }, hash: '#x' });
	});
});

describe('privacy route', () => {
	it('is open to guests and to signed-in users', () => {
		const privacy = find(loadRoutes(true), '/privacy');
		expect(privacy.name).toBe('Privacy');
		expect(privacy.meta).toMatchObject({
			allowedGuests: true,
			notOnlyForLoggedUsers: true,
		});
		expect(typeof privacy.component).toBe('function');
	});
});
