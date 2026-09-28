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
