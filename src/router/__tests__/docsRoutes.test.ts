import { readFileSync } from 'fs';
import { ModuleKind, transpileModule } from 'typescript';
import { createMemoryHistory, createRouter } from 'vue-router';
import docsRoutes from '../docsRoutes';

const Stub = { render: () => null };

const stubbed = (routes: any[]): any[] =>
	routes.map((r) => ({
		...r,
		...(r.component ? { component: Stub } : {}),
		...(r.children ? { children: stubbed(r.children) } : {}),
	}));

const loadAppRoutes = () => {
	const source = transpileModule(
		readFileSync(require.resolve('../routes.js'), 'utf8'),
		{ compilerOptions: { module: ModuleKind.CommonJS, allowJs: true } },
	).outputText;
	const loaded = { exports: {} as any };
	(globalThis as any).window = { app: { $store: { getters: {}, state: {} } } };
	const dependencies: Record<string, unknown> = {
		'@/router/docsRoutes': { default: docsRoutes },
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

const docsRouter = () =>
	createRouter({ history: createMemoryHistory(), routes: stubbed(docsRoutes) });

describe('docs routes', () => {
	it.each([
		['/cli', '/docs/cli'],
		['/cli#safety', '/docs/cli#safety'],
		['/cli?x=1#tasks', '/docs/cli?x=1#tasks'],
		['/help', '/docs'],
		['/docs/agents#links', '/docs/agents#links'],
		['/docs/ru/cli', '/docs/ru/cli'],
	])('%s lands on %s', async (from, to) => {
		const router = docsRouter();
		await router.push(from);
		expect(router.currentRoute.value.fullPath).toBe(to);
	});

	it('serves every docs URL as a public page', async () => {
		const router = docsRouter();
		for (const path of [
			'/docs',
			'/docs/agents',
			'/docs/ru/cli/install-and-sign-in',
		]) {
			await router.push(path);
			expect(router.currentRoute.value.meta).toMatchObject({
				allowedGuests: true,
				notOnlyForLoggedUsers: true,
				navbarHidden: false,
				transitionName: 'fade-fast',
			});
		}
	});

	it('wins over the workspace routes of the app', () => {
		const router = createRouter({
			history: createMemoryHistory(),
			routes: stubbed(loadAppRoutes()),
		});
		expect(router.resolve('/docs').name).toBe('Docs');
		expect(router.resolve('/docs/').name).toBe('Docs');
		expect(router.resolve('/docs/agents').name).toBe('AgentsGuide');
		expect(router.resolve('/docs/ru/agents').name).toBe('Docs');
		expect(router.resolve('/docs/dashboard').name).toBe('Docs');
		expect(router.resolve('/docs/ru/cli/tasks-from-the-terminal').name).toBe(
			'Docs',
		);
		expect(router.resolve({ name: 'AgentsGuide' }).fullPath).toBe(
			'/docs/agents',
		);
		expect(
			loadAppRoutes().filter((r) => ['/cli', '/help'].includes(r.path)),
		).toHaveLength(2);
	});
});
