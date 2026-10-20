import { readFileSync } from 'fs';
import { ModuleKind, transpileModule } from 'typescript';
import { createMemoryHistory, createRouter } from 'vue-router';
import {
	buildOpenUrl,
	DESKTOP_BAR_DISMISS_KEY,
	DESKTOP_BAR_PLATFORMS,
	DesktopBarInput,
	desktopBarVisible,
	OPEN_LINK_MESSAGES,
	openAppPath,
	OpenAppPathDeps,
	parseOpenPath,
} from '../openDesktopLink';

const loadRoutes = () => {
	const source = transpileModule(
		readFileSync(require.resolve('../../router/routes.js'), 'utf8'),
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

const router = createRouter({
	history: createMemoryHistory(),
	routes: loadRoutes(),
});
const resolve = (p: string) => router.resolve(p) as any;

describe('parseOpenPath', () => {
	it.each([
		'/tmgrdev/tasks/10107',
		'/ws/board',
		'/ws/pages/some-slug',
		'/ws/ABC/12',
		'/ws/dashboard',
		'/ws/list',
		'/ws/pages',
		'/ws/categories/5/children',
	])('allows %s', (path) => {
		expect(parseOpenPath(path, resolve)).toBe(path);
	});

	it.each([
		'//evil',
		'/ws/plugins/x/y',
		'/ws/task-window/1',
		'/ws/page-window/s',
		'/ws/categories/create',
		'/ws/categories/5',
		'/ws/tasks/create',
		'/ws/tasks/abc',
		'/ws/ABC/create',
		'/ws/categories/5/create',
		'/ws/getting-started',
		'/ws/pages/_/trash',
		'/ws/%2Fx',
		'/ws/../x',
		'/ws\\x',
		'/ws/board?a=1',
		'/ws/board#x',
		'/ws/борд',
		'/a/b/c/d/e',
		'/ws',
		'/settings/workspaces',
		'/login/foo',
		'javascript:alert(1)',
		'https://x',
		'',
	])('rejects %s', (path) => {
		expect(parseOpenPath(path, resolve)).toBeNull();
	});
});

describe('buildOpenUrl', () => {
	it('encodes the path', () => {
		expect(buildOpenUrl('/ws/board')).toBe('tmgr://open?path=%2Fws%2Fboard');
	});
	it('is bare without a path', () => {
		expect(buildOpenUrl(null)).toBe('tmgr://open');
	});
});

const MAC_SAFARI =
	'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15';
const MAC_CHROME =
	'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36';
const NOW = Date.parse('2026-10-21T12:00:00Z');
const DAY = 86_400_000;

const input = (over: Partial<DesktopBarInput> = {}): DesktopBarInput => ({
	desktopSeenAt: new Date(NOW - DAY).toISOString(),
	now: NOW,
	isDesktopApp: false,
	loggedIn: true,
	userLoaded: true,
	dismissedForUserId: null,
	userId: 7,
	nav: { platform: 'MacIntel', userAgent: MAC_SAFARI, maxTouchPoints: 0 },
	...over,
});

describe('desktopBarVisible', () => {
	it('exports its constants', () => {
		expect(DESKTOP_BAR_PLATFORMS).toEqual(['macos']);
		expect(DESKTOP_BAR_DISMISS_KEY).toBe('tmgr:desktop-bar-dismissed');
	});

	it('shows for a fresh flag on macOS Safari and Chrome', () => {
		expect(desktopBarVisible(input())).toBe(true);
		expect(
			desktopBarVisible(
				input({ nav: { platform: 'MacIntel', userAgent: MAC_CHROME } }),
			),
		).toBe(true);
		expect(
			desktopBarVisible(
				input({
					nav: { userAgent: MAC_CHROME, userAgentData: { platform: 'macOS' } },
				}),
			),
		).toBe(true);
	});

	it.each([
		['flag missing', { desktopSeenAt: null }],
		['flag undefined', { desktopSeenAt: undefined }],
		['flag garbage', { desktopSeenAt: 'nope' }],
		[
			'flag older than 30 days',
			{ desktopSeenAt: new Date(NOW - 31 * DAY).toISOString() },
		],
		[
			'flag in the far future',
			{ desktopSeenAt: new Date(NOW + 3 * DAY).toISOString() },
		],
		['desktop app', { isDesktopApp: true }],
		['logged out', { loggedIn: false }],
		['user not loaded', { userLoaded: false }],
		['dismissed for the same user', { dismissedForUserId: '7' }],
		[
			'Windows',
			{
				nav: {
					platform: 'Win32',
					userAgent: 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/120',
				},
			},
		],
		[
			'Linux',
			{
				nav: {
					platform: 'Linux x86_64',
					userAgent: 'Mozilla/5.0 (X11; Linux x86_64) Chrome/120',
				},
			},
		],
		[
			'iPhone',
			{
				nav: {
					platform: 'iPhone',
					userAgent: 'Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X)',
				},
			},
		],
		[
			'iPad reporting MacIntel',
			{
				nav: {
					platform: 'MacIntel',
					userAgent: MAC_SAFARI,
					maxTouchPoints: 5,
				},
			},
		],
		[
			'Android',
			{
				nav: {
					platform: 'Linux armv8l',
					userAgent: 'Mozilla/5.0 (Linux; Android 14; Pixel 8) Mobile Safari',
				},
			},
		],
		[
			'userAgentData says Windows over a Mac platform',
			{
				nav: {
					platform: 'MacIntel',
					userAgent: MAC_CHROME,
					userAgentData: { platform: 'Windows' },
				},
			},
		],
	])('hides: %s', (_name, over) => {
		expect(desktopBarVisible(input(over as Partial<DesktopBarInput>))).toBe(
			false,
		);
	});

	it('shows when dismissed by another user', () => {
		expect(desktopBarVisible(input({ dismissedForUserId: '8' }))).toBe(true);
	});
});

const setup = (over: Partial<OpenAppPathDeps> = {}) => {
	const deps = {
		resolve,
		loadWorkspaces: jest.fn(async () => [{ id: 1, code: 'ws' }]),
		isLocalOnly: jest.fn(() => false),
		showMain: jest.fn(async () => undefined),
		navigate: jest.fn(async () => undefined),
		notify: jest.fn(),
		...over,
	};
	return deps;
};

describe('openAppPath', () => {
	it('only shows the window when there is no path', async () => {
		const deps = setup();
		expect(await openAppPath({ path: null }, deps)).toBe(true);
		expect(deps.showMain).toHaveBeenCalled();
		expect(deps.navigate).not.toHaveBeenCalled();
		expect(deps.notify).not.toHaveBeenCalled();
	});

	it('navigates to a valid path in a known workspace', async () => {
		const deps = setup();
		expect(await openAppPath({ path: '/ws/board' }, deps)).toBe(true);
		expect(deps.navigate).toHaveBeenCalledWith('/ws/board');
		expect(deps.notify).not.toHaveBeenCalled();
	});

	it('reports an invalid path after showing the window', async () => {
		const deps = setup();
		expect(await openAppPath({ path: '/ws/plugins/a/b' }, deps)).toBe(false);
		expect(deps.showMain).toHaveBeenCalled();
		expect(deps.notify).toHaveBeenCalledWith(OPEN_LINK_MESSAGES.invalid);
		expect(deps.navigate).not.toHaveBeenCalled();
	});

	it('asks for a cloud account in local-only mode', async () => {
		const deps = setup({ isLocalOnly: jest.fn(() => true) });
		expect(await openAppPath({ path: '/other/board' }, deps)).toBe(false);
		expect(deps.notify).toHaveBeenCalledWith(OPEN_LINK_MESSAGES.needsCloud);
		expect(deps.navigate).not.toHaveBeenCalled();
	});

	it('says the page is unavailable in a cloud account without the workspace', async () => {
		const deps = setup();
		expect(await openAppPath({ path: '/other/board' }, deps)).toBe(false);
		expect(deps.notify).toHaveBeenCalledWith(OPEN_LINK_MESSAGES.unavailable);
		expect(deps.navigate).not.toHaveBeenCalled();
	});

	it('treats a failed workspace load as unavailable', async () => {
		const deps = setup({
			loadWorkspaces: jest.fn(async () => {
				throw new Error('offline');
			}),
		});
		expect(await openAppPath({ path: '/ws/board' }, deps)).toBe(false);
		expect(deps.notify).toHaveBeenCalledWith(OPEN_LINK_MESSAGES.unavailable);
	});
});
