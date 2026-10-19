import { readFileSync } from 'fs';
import { ModuleKind, transpileModule } from 'typescript';

const loadRoutes = () => {
	const source = transpileModule(
		readFileSync(require.resolve('../routes.js'), 'utf8'),
		{ compilerOptions: { module: ModuleKind.CommonJS, allowJs: true } },
	).outputText;
	const loaded = { exports: {} as any };
	(globalThis as any).window = { app: { $store: { getters: {}, state: {} } } };
	const dependencies: Record<string, unknown> = {
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

	it('sends the fallback create route to the fallback list in create mode', () => {
		expect(
			byName('FallbackCategoryCreate').redirect({ params: {}, query: {} }),
		).toEqual({
			name: 'FallbackCategoriesList',
			query: { create: 'top' },
		});
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
