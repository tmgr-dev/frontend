import variant from '@jitl/quickjs-wasmfile-release-sync';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import {
	newQuickJSWASMModuleFromVariant,
	type QuickJSWASMModule,
} from 'quickjs-emscripten-core';
import { createBroker, type DataApi } from '../broker';
import { parseManifest } from '../manifest';
import { createSandbox } from '../sandbox';
import { sanitizeTree } from '../uiTree';

const dir = join(__dirname, '../builtin/estimate');
const manifest = parseManifest(
	JSON.parse(readFileSync(join(dir, 'manifest.json'), 'utf8')),
);
const code = readFileSync(join(dir, 'main.js'), 'utf8');

let quickjs: QuickJSWASMModule;
beforeAll(async () => {
	quickjs = await newQuickJSWASMModuleFromVariant({
		...variant,
		importModuleLoader: async () =>
			require('@jitl/quickjs-wasmfile-release-sync/emscripten-module'),
	});
});

const HOUR = 3600;
const tasks = [
	{
		id: 1,
		title: 'Over',
		status_id: 2,
		common_time: 3 * HOUR,
		approximately_time: 2 * HOUR,
		start_time: 0,
	},
	{
		id: 2,
		title: 'Close',
		status_id: 2,
		common_time: 0.9 * HOUR,
		approximately_time: HOUR,
		start_time: 0,
	},
	{
		id: 3,
		title: 'Fine',
		status_id: 1,
		common_time: 0.5 * HOUR,
		approximately_time: HOUR,
		start_time: 0,
	},
	{
		id: 4,
		title: 'No estimate',
		status_id: 2,
		common_time: HOUR,
		approximately_time: 0,
		start_time: 0,
	},
	{
		id: 5,
		title: 'Done late',
		status_id: 3,
		common_time: 5 * HOUR,
		approximately_time: HOUR,
		start_time: 0,
	},
];

const run = async () => {
	const statusBar: unknown[] = [];
	const api = {
		listTasks: async () => ({ items: tasks, total: tasks.length }),
		listStatuses: async () => [
			{ id: 1, type: 'default' },
			{ id: 2, type: 'active' },
			{ id: 3, type: 'completed' },
		],
	} as unknown as DataApi;
	const broker = createBroker({
		manifest,
		workspace: { id: -7, code: 'local-notes', name: 'Notes', kind: 'local' },
		currentWorkspaceId: () => -7,
		api,
		settings: () => ({ warnAt: 0.8 }),
		notify: () => undefined,
		setStatusBarItem: (id, item) => statusBar.push([id, item]),
		refresh: () => undefined,
		setViewBadge: () => undefined,
		register: () => undefined,
		log: () => undefined,
		now: () => 0,
	});
	const sandbox = createSandbox({
		quickjs,
		code,
		call: (m, p) => broker.call(m, p),
	});
	await sandbox.start();
	await new Promise((resolve) => setTimeout(resolve, 20));
	return { sandbox, statusBar };
};

it('colours card badges by the share of the estimate used', async () => {
	const { sandbox } = await run();
	expect(await sandbox.dispatch('badges', 'ratio', tasks)).toEqual({
		1: { text: '150%', color: 'red', tooltip: '3h 0m of 2h 0m estimated' },
		2: { text: '90%', color: 'yellow', tooltip: '54m of 1h 0m estimated' },
		3: { text: '50%', color: 'green', tooltip: '30m of 1h 0m estimated' },
		5: { text: '500%', color: 'red', tooltip: '5h 0m of 1h 0m estimated' },
	});
	sandbox.dispose();
});

it('shows the overrun of tasks in progress in the status bar', async () => {
	const { sandbox, statusBar } = await run();
	expect(statusBar).toEqual([
		[
			'overrun',
			{
				text: '1h 0m over estimate',
				tooltip: '1 task in progress went past the estimate',
				command: 'tmgr.estimate.refresh',
			},
		],
	]);
	sandbox.dispose();
});

it('renders a report page and a task section the host accepts', async () => {
	const { sandbox } = await run();
	const page = sanitizeTree(
		await sandbox.dispatch('page', 'report', null),
	) as any;
	expect(page.children[0]).toEqual({
		type: 'heading',
		text: 'Estimate vs actual',
		level: 1,
	});
	const table = page.children.find((c: any) => c.type === 'table');
	expect(table.rows.map((r: any) => r.taskId)).toEqual([5, 1, 2, 3]);
	expect(page.children[page.children.length - 1]).toEqual({
		type: 'button',
		text: 'Recalculate',
		command: 'tmgr.estimate.refresh',
	});

	const section = sanitizeTree(
		await sandbox.dispatch('section', 'estimate', tasks[0]),
	) as any;
	expect(
		section.children[0].children.map((c: any) => [c.label, c.value]),
	).toEqual([
		['Spent', '3h 0m'],
		['Estimate', '2h 0m'],
		['Over', '1h 0m'],
	]);
	expect(
		sanitizeTree(await sandbox.dispatch('section', 'estimate', tasks[3])),
	).toMatchObject({ type: 'text', tone: 'muted' });
	sandbox.dispose();
});
