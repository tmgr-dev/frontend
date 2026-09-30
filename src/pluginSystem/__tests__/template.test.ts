import variant from '@jitl/quickjs-wasmfile-release-sync';
import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { newQuickJSWASMModuleFromVariant } from 'quickjs-emscripten-core';
import { createBroker, type DataApi } from '../broker';
import { bundleToPackage } from '../market';
import { createSandbox } from '../sandbox';
import { sanitizeTree } from '../uiTree';

const dir = join(__dirname, '../../../plugin-sdk/template');

it('the plugin template builds into a bundle the app accepts and runs', async () => {
	const manifest = JSON.parse(readFileSync(join(dir, 'manifest.json'), 'utf8'));
	const pages = Object.fromEntries(
		readdirSync(join(dir, 'ui')).map((name) => [
			`ui/${name}`,
			readFileSync(join(dir, 'ui', name), 'utf8'),
		]),
	);
	const bundle = JSON.stringify({
		manifest,
		code: readFileSync(join(dir, 'main.js'), 'utf8'),
		pages,
	});
	const pkg = bundleToPackage({
		repo: 'yourname/hello',
		tag: 'v1.0.0',
		sha256: 'x',
		bundle,
		signature: 'sig',
		public_key: 'RWkey',
		verified: false,
	});

	const quickjs = await newQuickJSWASMModuleFromVariant({
		...variant,
		importModuleLoader: async () =>
			require('@jitl/quickjs-wasmfile-release-sync/emscripten-module'),
	});
	const statusBar: unknown[] = [];
	const broker = createBroker({
		manifest: pkg.manifest,
		workspace: { id: -1, code: 'local-x', name: 'X', kind: 'local' },
		currentWorkspaceId: () => -1,
		api: {
			listStatuses: async () => [{ id: 2, type: 'active' }],
			listTasks: async () => ({
				items: [{ id: 5, title: 'Write docs', status_id: 2 }],
				total: 1,
			}),
		} as unknown as DataApi,
		settings: () => ({}),
		notify: () => undefined,
		setStatusBarItem: (id, item) => statusBar.push([id, item?.text]),
		refresh: () => undefined,
		setViewBadge: () => undefined,
		register: () => undefined,
		log: () => undefined,
		now: () => 0,
	});
	const sandbox = createSandbox({
		quickjs,
		code: pkg.code,
		call: (m, p) => broker.call(m, p),
	});
	await sandbox.start();
	await new Promise((resolve) => setTimeout(resolve, 20));
	expect(statusBar).toEqual([['count', '1 in progress']]);
	expect(
		JSON.stringify(
			sanitizeTree(await sandbox.dispatch('page', 'summary', null)),
		),
	).toContain('Write docs');
	sandbox.dispose();
});
