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
		commands: [{ id: 'tmgr.estimate.refresh', title: 'Refresh estimates' }],
		views: [{ id: 'report', title: 'Overrun' }],
		taskPanelSections: [{ id: 'summary', title: 'Estimate' }],
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
	expect(
		parseManifest({ ...valid, contributes: undefined }).contributes,
	).toEqual({
		boardCardBadges: [],
		statusBarItems: [],
		commands: [],
		views: [],
		taskPanelSections: [],
		settings: null,
	});
});

it.each([
	[{ id: 'Estimate' }, 'id'],
	[{ id: 'tmgr.estimate/../x' }, 'id'],
	[{ version: '1.0' }, 'version'],
	[{ permissions: ['tasks:read', 'shell:exec'] }, 'shell:exec'],
	[{ engines: { tmgr: '^2.0' } }, 'engines'],
	[
		{ contributes: { commands: [{ id: 'other.plugin.cmd', title: 'x' }] } },
		'other.plugin.cmd',
	],
	[{ contributes: { views: [{ id: 'a b', title: 'x' }] } }, 'a b'],
])('rejects %j', (patch, message) => {
	expect(() => parseManifest({ ...valid, ...patch })).toThrow(message);
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
	for (const origin of [
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
