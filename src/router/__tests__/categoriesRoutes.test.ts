import { readFileSync } from 'fs';
import { join } from 'path';
import { ModuleKind, transpileModule } from 'typescript';
import { createMemoryHistory, createRouter } from 'vue-router';

const loadRoutes = () => {
	const source = transpileModule(
		readFileSync(require.resolve('../routes.js'), 'utf8'),
		{ compilerOptions: { module: ModuleKind.CommonJS, allowJs: true } },
	).outputText;
	const loaded = { exports: {} as any };
	(globalThis as any).window = { app: { $store: { getters: {}, state: {} } } };
	const dependencies: Record<string, unknown> = {
		'@/router/docsRoutes': { default: [] },
		'@/router/moduleGate': {
			gatedPage: (_k: string, _t: string, f: unknown) => f,
		},
		'@/store': { default: { getters: {} } },
		'@/utils/simpleStatus': {
			boardRedirect: () => undefined,
			isSimpleMode: () => false,
		},
	};
	new Function('require', 'module', 'exports', source)(
		(name: string) => dependencies[name],
		loaded,
		loaded.exports,
	);
	return loaded.exports.default as any[];
};

const byName = (name: string) => loadRoutes().find((r) => r.name === name);

describe('category routes land on the category page', () => {
	it('sends the old edit URL to the Settings tab of the category page', () => {
		expect(
			byName('WorkspaceCategoryEdit').redirect({
				params: { workspace_code: 'demo', id: '101' },
				query: { from: 'x' },
			}),
		).toEqual({
			name: 'WorkspaceCategoryChildren',
			params: { workspace_code: 'demo', id: '101' },
			query: { from: 'x', tab: 'settings' },
		});
	});

	it('sends the old status list to the categories root', () => {
		expect(
			byName('WorkspaceCategoryListWithStatus').redirect({
				params: { workspace_code: 'demo', status: 'x' },
				query: {},
			}),
		).toEqual({
			name: 'WorkspaceCategories',
			params: { workspace_code: 'demo' },
			query: {},
		});
	});

	it.each([
		[
			{ category: '101', tab: '1' },
			{
				name: 'WorkspaceCategoryChildren',
				params: { workspace_code: 'demo', id: '101' },
				query: { tab: 'settings' },
			},
		],
		[
			{ create: 'top', x: '1' },
			{
				name: 'WorkspaceCategoryCreate',
				params: { workspace_code: 'demo' },
				query: { x: '1' },
			},
		],
		[
			{ create: '27' },
			{
				name: 'WorkspaceCategoryCreateInCategory',
				params: { workspace_code: 'demo', category_id: '27' },
				query: {},
			},
		],
		[{}, true],
		[{ category: '' }, true],
	])('/demo/categories?%o redirects to %o', (query, target) => {
		expect(
			byName('WorkspaceCategories').beforeEnter({
				params: { workspace_code: 'demo' },
				query,
			}),
		).toEqual(target);
	});

	it('serves every category screen from the category page', () => {
		for (const name of [
			'WorkspaceCategories',
			'WorkspaceCategory',
			'WorkspaceCategoryChildren',
			'WorkspaceCategoryCreate',
			'WorkspaceCategoryCreateInCategory',
			'FallbackCategoriesList',
			'ProjectCategoryChildrenList',
		]) {
			expect(String(byName(name).component)).toContain('CategoryPage');
		}
	});
});

describe('legacy task route', () => {
	const resolve = (path: string) => {
		const router = createRouter({
			history: createMemoryHistory(),
			routes: loadRoutes().map((r) => ({
				...r,
				component: r.component ?? { render: () => null },
			})),
		});
		return router.resolve(path).name;
	};

	it('resolves numeric ids to the task form', () => {
		expect(resolve('/123')).toBe('TasksEdit');
	});

	it.each(['/some-unknown-slug', '/tmgrdev'])(
		'sends the unknown one-segment URL %s to the 404 page',
		(path) => {
			expect(resolve(path)).toBe('NotFound');
		},
	);

	it.each([
		['/login', 'Login'],
		['/routines', 'DailyRoutines'],
		['/archive', 'ArchiveTasksList'],
		['/quick-add', 'QuickAdd'],
		['/settings', 'Settings'],
	])('keeps %s', (path, name) => {
		expect(resolve(path)).toBe(name);
	});
});

describe('route names used by category screens', () => {
	it.each([
		'src/pages/ProjectCategoryList.vue',
		'src/components/general/CategoryBadge.vue',
	])('%s only navigates to routes that exist', (file) => {
		const defined = new Set(loadRoutes().map((r) => r.name));
		const used = [
			...readFileSync(join(__dirname, '../../..', file), 'utf8').matchAll(
				/name: '([A-Z][A-Za-z]+)'/g,
			),
		].map((m) => m[1]);
		expect(used.filter((n) => !defined.has(n))).toEqual([]);
	});
});
