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

describe('old category form routes open the settings pane', () => {
	it.each([
		['WorkspaceCategoryCreate', { workspace_code: 'demo' }, { create: 'top' }],
		[
			'WorkspaceCategoryEdit',
			{ workspace_code: 'demo', id: '101' },
			{ category: '101' },
		],
		[
			'WorkspaceCategoryCreateInCategory',
			{ workspace_code: 'demo', category_id: '27' },
			{ create: '27' },
		],
		[
			'WorkspaceCategoryListWithStatus',
			{ workspace_code: 'demo', status: 'x' },
			{},
		],
	])('%s keeps its name and redirects into the pane', (name, params, query) => {
		const route = byName(name);
		expect(route.redirect({ params, query: { tab: '1' } })).toEqual({
			name: 'WorkspaceCategories',
			params: { workspace_code: 'demo' },
			query: { tab: '1', ...query },
		});
	});

	it('sends the fallback create route to the fallback list in create mode', () => {
		expect(
			byName('FallbackCategoryCreate').redirect({ params: {}, query: {} }),
		).toEqual({
			name: 'FallbackCategoriesList',
			query: { create: 'top' },
		});
	});

	it('serves the root list and the fallback list from the new page', () => {
		for (const name of ['WorkspaceCategories', 'FallbackCategoriesList']) {
			expect(String(byName(name).component)).toContain(
				'CategoriesSettingsPage',
			);
		}
	});
});
