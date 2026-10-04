import variant from '@jitl/quickjs-wasmfile-release-sync';
import {
	newQuickJSWASMModuleFromVariant,
	type QuickJSWASMModule,
} from 'quickjs-emscripten-core';

// eslint-disable-next-line @typescript-eslint/no-var-requires
const { createTestHost } = require('../../../plugin-sdk/testing/index.js');

const manifest = (engines: string) => ({
	id: 'tmgr-dev.ui-minor-test',
	name: 'UI Minor Test',
	version: '1.0.0',
	publisher: 'tmgr-dev',
	engines: { tmgr: engines },
	main: 'main.js',
	contributes: { views: [{ id: 'main', title: 'Main' }] },
});

const code = `
tmgr.ui.providePage('main', async () => ({
	type: 'stack',
	children: [
		{ type: 'card', tone: 'raised', children: [{ type: 'heading', text: 'Card title', level: 2 }] },
		{ type: 'grid', columns: 2, children: [{ type: 'text', text: 'a' }, { type: 'text', text: 'b' }] },
		{ type: 'menu', items: [{ text: 'Do it', command: 'x.y' }] },
	],
}));
`;

let quickjs: QuickJSWASMModule;
beforeAll(async () => {
	// jest runs CommonJS without dynamic import(); load the same emscripten module with require (see
	// sandbox.test.ts).
	quickjs = await newQuickJSWASMModuleFromVariant({
		...variant,
		importModuleLoader: async () =>
			require('@jitl/quickjs-wasmfile-release-sync/emscripten-module'),
	});
});

it('renderPage sanitizes card/grid/menu for an engines ^1.3 plugin', async () => {
	const host = await createTestHost({ manifest: manifest('^1.3'), code, quickjs });
	const page = await host.renderPage('main', null);
	expect(page.children.map((child: any) => child.type)).toEqual(['card', 'grid', 'menu']);
	host.dispose();
});

it('renderPage downgrades the same page to stacks for an engines ^1.2 plugin', async () => {
	const host = await createTestHost({ manifest: manifest('^1.2'), code, quickjs });
	const page = await host.renderPage('main', null);
	expect(page.children).toEqual([
		{
			type: 'stack',
			direction: 'column',
			children: [{ type: 'heading', text: 'Card title', level: 2 }],
		},
		{
			type: 'stack',
			direction: 'row',
			children: [
				{ type: 'text', text: 'a', tone: 'default' },
				{ type: 'text', text: 'b', tone: 'default' },
			],
		},
		{ type: 'stack', direction: 'column', children: [] },
	]);
	host.dispose();
});

const badgeManifest = (permissions: string[], engines = '^1.4') => ({
	id: 'tmgr-dev.badge-test',
	name: 'Badge Test',
	version: '1.0.0',
	publisher: 'tmgr-dev',
	engines: { tmgr: engines },
	main: 'main.js',
	permissions,
	contributes: {
		views: [{ id: 'inbox', title: 'Inbox' }],
		commands: [{ id: 'tmgr-dev.badge-test.set', title: 'Set' }],
	},
});

const badgeCode = `
tmgr.commands.register('tmgr-dev.badge-test.set', (args) =>
	tmgr.ui.setViewBadge(args.viewId, args.badge).then(() => 'ok', (e) => e.name));
`;

describe('setViewBadge in the test host', () => {
	const set = (host: any, viewId: unknown, badge: unknown) =>
		host.runCommand('tmgr-dev.badge-test.set', { viewId, badge });

	it('exposes tmgr.viewBadges[viewId] at once, and null or a 0 count removes it', async () => {
		const host = await createTestHost({ manifest: badgeManifest(['views:badge']), code: badgeCode, quickjs });
		expect(await set(host, 'inbox', { count: 12, tone: 'danger' })).toBe('ok');
		expect(host.tmgr.viewBadges.inbox).toEqual({ count: 12, text: null, tone: 'danger' });
		expect(await set(host, 'inbox', { text: 'new' })).toBe('ok');
		expect(host.tmgr.viewBadges.inbox).toEqual({ count: null, text: 'new', tone: 'default' });
		await set(host, 'inbox', { count: 0 });
		expect(host.tmgr.viewBadges).toEqual({});
		await set(host, 'inbox', { count: 2 });
		await set(host, 'inbox', null);
		expect(host.tmgr.viewBadges).toEqual({});
		host.dispose();
	});

	it('answers with the same errors as the app', async () => {
		const host = await createTestHost({ manifest: badgeManifest(['views:badge']), code: badgeCode, quickjs });
		expect(await set(host, 'other', { count: 1 })).toBe('INVALID_PARAMS');
		expect(await set(host, 'inbox', { count: 1, text: 'a' })).toBe('INVALID_PARAMS');
		expect(await set(host, 'inbox', { text: 'abcde' })).toBe('INVALID_PARAMS');
		expect(await set(host, 'inbox', { count: -1 })).toBe('INVALID_PARAMS');
		expect(host.tmgr.viewBadges).toEqual({});
		host.dispose();
	});

	it('needs the views:badge permission', async () => {
		const host = await createTestHost({ manifest: badgeManifest([]), code: badgeCode, quickjs });
		expect(await set(host, 'inbox', { count: 1 })).toBe('PERMISSION_DENIED');
		host.dispose();
	});

	it('refuses to load a ^1.3 manifest declaring views:badge', async () => {
		await expect(
			createTestHost({ manifest: badgeManifest(['views:badge'], '^1.3'), code: badgeCode, quickjs }),
		).rejects.toThrow('views:badge needs engines.tmgr ^1.4');
	});
});

describe('task menu in the test host (API 1.6)', () => {
	const menuManifest = (permissions: string[], engines = '^1.6') => ({
		id: 'tmgr-dev.menu-test',
		name: 'Menu Test',
		version: '1.0.0',
		publisher: 'tmgr-dev',
		engines: { tmgr: engines },
		main: 'main.js',
		permissions,
		contributes: {
			commands: [
				{ id: 'tmgr-dev.menu-test.show', title: 'Show' },
				{ id: 'tmgr-dev.menu-test.ghost', title: 'Ghost' },
				{ id: 'tmgr-dev.menu-test.other', title: 'Other' },
			],
			menus: {
				'task/card': [
					{ command: 'tmgr-dev.menu-test.show', title: 'Show it' },
					{ command: 'tmgr-dev.menu-test.ghost', title: 'Never registered' },
				],
			},
		},
	});
	const menuCode = `
tmgr.commands.register('tmgr-dev.menu-test.show', (args) => args);
tmgr.commands.register('tmgr-dev.menu-test.other', (args) => args);
`;

	it('lists the registered items and clicks with { taskId, workspaceId }', async () => {
		const host = await createTestHost({ manifest: menuManifest(['menus:task']), code: menuCode, quickjs });
		expect(host.taskMenuItems()).toEqual([{ command: 'tmgr-dev.menu-test.show', title: 'Show it' }]);
		expect(await host.clickTaskMenu('tmgr-dev.menu-test.show', 12)).toEqual({ taskId: 12, workspaceId: -1 });
		host.dispose();
	});

	it('refuses what the app refuses', async () => {
		const host = await createTestHost({ manifest: menuManifest(['menus:task']), code: menuCode, quickjs });
		for (const [command, taskId] of [
			['tmgr-dev.menu-test.other', 1],
			['tmgr-dev.menu-test.ghost', 1],
			['tmgr-dev.menu-test.show', 0],
			['tmgr-dev.menu-test.show', 1.5],
		] as const)
			await expect(host.clickTaskMenu(command, taskId)).rejects.toMatchObject({ code: 'NOT_DECLARED' });
		host.dispose();
	});

	it('shows nothing for a ^1.5 plugin that declares menus', async () => {
		const host = await createTestHost({ manifest: menuManifest([], '^1.5'), code: menuCode, quickjs });
		expect(host.taskMenuItems()).toEqual([]);
		await expect(host.clickTaskMenu('tmgr-dev.menu-test.show', 1)).rejects.toMatchObject({ code: 'NOT_DECLARED' });
		host.dispose();
	});

	it('refuses to load menus without the permission', async () => {
		await expect(
			createTestHost({ manifest: menuManifest([]), code: menuCode, quickjs }),
		).rejects.toThrow('menus:task');
	});
});
