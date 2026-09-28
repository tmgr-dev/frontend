import { parseManifest } from '../manifest';

const valid = {
	id: 'tmgr.estimate',
	name: 'Estimate vs actual',
	version: '1.0.0',
	publisher: 'tmgr',
	engines: { tmgr: '^1.0' },
	main: 'main.js',
	permissions: ['tasks:read', 'time:read'],
	contributes: {
		boardCardBadges: [{ id: 'overrun' }],
		statusBarItems: [{ id: 'total' }],
		trayItems: [{ id: 'menu' }],
		commands: [{ id: 'tmgr.estimate.refresh', title: 'Refresh estimates' }],
		views: [{ id: 'report', title: 'Overrun' }],
		taskPanelSections: [{ id: 'summary', title: 'Estimate' }],
		boardFilters: [{ id: 'overrun-only', title: 'Overrun', badge: 'overrun' }],
		settings: {
			type: 'object',
			properties: {
				warnAt: { type: 'number', default: 0.8, title: 'Warn at' },
			},
		},
	},
};

it('accepts a complete manifest and fills defaults', () => {
	const manifest = parseManifest(valid);
	expect(manifest.id).toBe('tmgr.estimate');
	expect(manifest.permissions).toEqual(['tasks:read', 'time:read']);
	expect(manifest.contributes.views).toEqual([
		{ id: 'report', title: 'Overrun' },
	]);
	expect(manifest.contributes.boardFilters).toEqual([
		{ id: 'overrun-only', title: 'Overrun', badge: 'overrun', key: undefined },
	]);
	expect(
		parseManifest({ ...valid, contributes: undefined }).contributes,
	).toEqual({
		boardCardBadges: [],
		statusBarItems: [],
		trayItems: [],
		commands: [],
		views: [],
		taskPanelSections: [],
		boardFilters: [],
		settings: null,
	});
});

it('validates boardFilters against a declared boardCardBadges id', () => {
	expect(() =>
		parseManifest({
			...valid,
			contributes: {
				...valid.contributes,
				boardFilters: [{ id: 'x', title: 'X', badge: 'not-declared' }],
			},
		}),
	).toThrow('boardFilters');
	expect(
		parseManifest({
			...valid,
			contributes: {
				...valid.contributes,
				boardFilters: [
					{ id: 'x', title: 'X', badge: 'overrun', key: 'over-budget' },
				],
			},
		}).contributes.boardFilters,
	).toEqual([{ id: 'x', title: 'X', badge: 'overrun', key: 'over-budget' }]);
});

it('caps trayItems at 5 and validates their ids like statusBarItems', () => {
	const trayItems = (n: number) =>
		Array.from({ length: n }, (_, i) => ({ id: `item${i}` }));
	expect(
		parseManifest({
			...valid,
			contributes: { ...valid.contributes, trayItems: trayItems(5) },
		}).contributes.trayItems,
	).toHaveLength(5);
	expect(() =>
		parseManifest({
			...valid,
			contributes: { ...valid.contributes, trayItems: trayItems(6) },
		}),
	).toThrow('trayItems');
	expect(() =>
		parseManifest({
			...valid,
			contributes: { ...valid.contributes, trayItems: [{ id: 'a b' }] },
		}),
	).toThrow('trayItems');
});

it('accepts a deepLink command with a plain local id, and rejects the rest', () => {
	const withCommand = (command: unknown) => ({
		...valid,
		contributes: { commands: [command] },
	});
	expect(
		parseManifest(
			withCommand({ id: 'tmgr.estimate.refresh', title: 'x', deepLink: true }),
		).contributes.commands,
	).toEqual([{ id: 'tmgr.estimate.refresh', title: 'x', deepLink: true }]);
	expect(
		parseManifest(
			withCommand({ id: 'tmgr.estimate.refresh', title: 'x', deepLink: false }),
		).contributes.commands,
	).toEqual([{ id: 'tmgr.estimate.refresh', title: 'x' }]);
	expect(() =>
		parseManifest(
			withCommand({ id: 'tmgr.estimate.refresh', title: 'x', deepLink: 'yes' }),
		),
	).toThrow('deepLink');
	expect(() =>
		parseManifest(
			withCommand({
				id: 'tmgr.estimate.a.b',
				title: 'x',
				deepLink: true,
			}),
		),
	).toThrow('linkable');
});

it.each([
	[{ id: 'Estimate' }, 'id'],
	[{ id: 'tmgr.estimate/../x' }, 'id'],
	[{ version: '1.0' }, 'version'],
	[{ permissions: ['tasks:read', 'shell:exec'] }, 'shell:exec'],
	[{ engines: { tmgr: '^2.0' } }, 'engines'],
	[{ engines: { tmgr: '^1.3' } }, 'engines'],
	[{ links: { allowedDomains: ['gitlab.com'] } }, 'links:open'],
	[
		{ permissions: ['links:open'], links: { allowedDomains: ['https://x.io'] } },
		'links domain',
	],
	[
		{ contributes: { commands: [{ id: 'other.plugin.cmd', title: 'x' }] } },
		'other.plugin.cmd',
	],
	[{ contributes: { views: [{ id: 'a b', title: 'x' }] } }, 'a b'],
])('rejects %j', (patch, message) => {
	expect(() => parseManifest({ ...valid, ...patch })).toThrow(message);
});

it('accepts plugins written for 1.0, 1.1 and 1.2', () => {
	expect(parseManifest({ ...valid, engines: { tmgr: '^1.0' } }).id).toBe(valid.id);
	expect(parseManifest({ ...valid, engines: { tmgr: '^1.1' } }).id).toBe(valid.id);
	expect(parseManifest({ ...valid, engines: { tmgr: '^1.2' } }).id).toBe(valid.id);
});

it('accepts the routines permissions', () => {
	expect(
		parseManifest({ ...valid, permissions: ['routines:read', 'routines:write'] }).permissions,
	).toEqual(['routines:read', 'routines:write']);
});

it('keeps link domains of a plugin allowed to open links', () => {
	expect(
		parseManifest({
			...valid,
			permissions: ['links:open'],
			links: { allowedDomains: ['gitlab.com', 'gitlab.com', 'jira.example.org'] },
		}).links,
	).toEqual({ allowedDomains: ['gitlab.com', 'jira.example.org'] });
});

it('ignores contribution kinds it does not know', () => {
	expect(
		parseManifest({
			...valid,
			contributes: { ...valid.contributes, menus: [{}] },
		}).contributes,
	).not.toHaveProperty('menus');
});

it('accepts network origins only on this computer', () => {
	expect(
		parseManifest({
			...valid,
			network: {
				allowedOrigins: ['http://localhost:11434', 'http://127.0.0.1:8080/'],
			},
		}).network,
	).toEqual({
		allowedOrigins: ['http://localhost:11434', 'http://127.0.0.1:8080'],
	});
	expect(parseManifest(valid).network).toEqual({ allowedOrigins: [] });
	expect(
		parseManifest({
			...valid,
			network: { allowedOrigins: ['http://localhost:80'] },
		}).network.allowedOrigins,
	).toEqual(['http://localhost']);
	for (const origin of [
		'http://localhost:08080',
		'https://api.openai.com',
		'http://localhost',
		'https://localhost:1',
		'http://192.0.2.1:80',
		'http://localhost.evil:80',
	]) {
		expect(() =>
			parseManifest({ ...valid, network: { allowedOrigins: [origin] } }),
		).toThrow('network');
	}
});

it('defaults companion to null and accepts a description with an optional homepage', () => {
	expect(parseManifest(valid).companion).toBeNull();
	expect(
		parseManifest({ ...valid, companion: { description: 'A CLI you run locally' } })
			.companion,
	).toEqual({ description: 'A CLI you run locally' });
	expect(
		parseManifest({
			...valid,
			companion: { description: 'A CLI', homepage: 'https://example.com/cli' },
		}).companion,
	).toEqual({ description: 'A CLI', homepage: 'https://example.com/cli' });
});

it('rejects an invalid companion section', () => {
	expect(() =>
		parseManifest({ ...valid, companion: { description: '' } }),
	).toThrow('companion');
	expect(() =>
		parseManifest({ ...valid, companion: { description: 'x'.repeat(201) } }),
	).toThrow('companion');
	expect(() =>
		parseManifest({
			...valid,
			companion: { description: 'A CLI', homepage: 'http://example.com' },
		}),
	).toThrow('companion.homepage');
});

it('lets a view name an html page from its ui folder', () => {
	const withUi = (ui: unknown) => ({
		...valid,
		contributes: { views: [{ id: 'board', title: 'Board', ui }] },
	});
	expect(parseManifest(withUi('ui/board.html')).contributes.views).toEqual([
		{ id: 'board', title: 'Board', ui: 'ui/board.html' },
	]);
	for (const bad of [
		'board.html',
		'ui/../main.js',
		'ui/a/b.html',
		'ui/board.js',
		'http://x/ui/a.html',
	]) {
		expect(() => parseManifest(withUi(bad))).toThrow('ui');
	}
});
