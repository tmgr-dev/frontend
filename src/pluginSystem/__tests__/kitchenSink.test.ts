import variant from '@jitl/quickjs-wasmfile-release-sync';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import {
	newQuickJSWASMModuleFromVariant,
	type QuickJSWASMModule,
} from 'quickjs-emscripten-core';
import { parseManifest } from '../manifest';

// eslint-disable-next-line @typescript-eslint/no-var-requires
const { createTestHost } = require('../../../plugin-sdk/testing/index.js');

const dir = join(__dirname, '../../../plugin-sdk/examples/kitchen-sink');
const loadManifest = () => JSON.parse(readFileSync(join(dir, 'manifest.json'), 'utf8'));

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

const startHost = (options: Record<string, unknown> = {}) =>
	createTestHost({ manifest: loadManifest(), mainPath: join(dir, 'main.js'), quickjs, ...options });

it('the kitchen-sink manifest is valid', () => {
	expect(() => parseManifest(loadManifest())).not.toThrow();
});

it('"Set up workspace" creates the status and category', async () => {
	const host = await startHost();
	await host.runCommand('tmgr-dev.kitchen-sink.setup', null);
	expect(host.tmgr.statuses.map((s: any) => s.name)).toContain('Needs answer');
	expect(host.tmgr.categories.map((c: any) => c.code)).toContain('KS');
	// Idempotent: running it again must not create a second status or category.
	await host.runCommand('tmgr-dev.kitchen-sink.setup', null);
	expect(host.tmgr.statuses).toHaveLength(1);
	expect(host.tmgr.categories).toHaveLength(1);
	host.dispose();
});

it('"Create sample task" gets a key, a comment and taskData', async () => {
	const host = await startHost();
	await host.runCommand('tmgr-dev.kitchen-sink.setup', null);
	const task = await host.runCommand('tmgr-dev.kitchen-sink.createSample', null);
	expect(task.key).toBe('KS-1');
	expect(host.tmgr.comments[task.id]).toHaveLength(1);
	expect(host.tmgr.comments[task.id][0].reactions['👍']).toBe(1);
	expect(host.tmgr.taskData[`${task.id}:kitchenSink.note`]).toBeDefined();
	expect(host.tmgr.relations[task.id]).toEqual([{ taskId: expect.any(Number), type: 'relates to' }]);
	host.dispose();
});

it('badges returns an array of normalized badges per task', async () => {
	const host = await startHost();
	await host.runCommand('tmgr-dev.kitchen-sink.setup', null);
	const task = await host.runCommand('tmgr-dev.kitchen-sink.createSample', null);
	const badges = await host.badges([task]);
	expect(Array.isArray(badges[task.id])).toBe(true);
	expect(badges[task.id].map((b: any) => b.text)).toEqual(expect.arrayContaining(['HIGH', 'KS']));
	host.dispose();
});

it('sets a counter badge on its view while active tasks exist (API 1.4)', async () => {
	const host = await startHost();
	await host.runCommand('tmgr-dev.kitchen-sink.setup', null);
	await host.runCommand('tmgr-dev.kitchen-sink.createSample', null);
	expect(host.tmgr.viewBadges.view).toEqual({ count: expect.any(Number), text: null, tone: 'info' });
	host.dispose();
});

it('the alarm fires and refreshes the tray item', async () => {
	const host = await startHost();
	await host.runCommand('tmgr-dev.kitchen-sink.setup', null);
	await host.runCommand('tmgr-dev.kitchen-sink.createSample', null);
	expect(host.tmgr.alarms['ks-tick']).toBeDefined();
	const scheduledAt = host.tmgr.alarms['ks-tick'].scheduledAtMs;
	await host.fireAlarms(scheduledAt + 1);
	expect(host.tmgr.trayItems['ks-tray']).toBeDefined();
	// Periodic: the alarm reschedules itself instead of disappearing.
	expect(host.tmgr.alarms['ks-tick'].scheduledAtMs).toBeGreaterThan(scheduledAt);
	host.dispose();
});

it('"Capture today\'s notes" converts an undated note into a task (API 1.2)', async () => {
	const host = await startHost({ routines: [{ title: 'Undated note' }] });
	await host.runCommand('tmgr-dev.kitchen-sink.setup', null);
	const task: any = await host.runCommand('tmgr-dev.kitchen-sink.captureNotes', null);
	expect(task.title).toBe('Undated note');
	expect(host.tmgr.routines).toHaveLength(0);
	expect(host.tmgr.tasks.map((t: any) => t.title)).toContain('Undated note');
	host.dispose();
});

it('routine.created is delivered to the plugin (API 1.2)', async () => {
	const host = await startHost({ routines: [{ title: 'Undated note' }] });
	await host.emit({
		type: 'routine.created',
		routineId: 1,
		routine: {
			id: 1,
			title: 'Undated note',
			description: null,
			scheduledDate: null,
			scheduledTime: null,
			createdAt: new Date().toISOString(),
			updatedAt: new Date().toISOString(),
		},
	});
	expect(host.tmgr.log.some((l: any) => l.message.includes('routine.created 1'))).toBe(true);
	host.dispose();
});

const containsNodeType = (node: any, type: string): boolean => {
	if (!node || typeof node !== 'object') return false;
	if (node.type === type) return true;
	const lists = [node.children, node.items].filter(Array.isArray);
	return lists.some((list) => list.some((child: any) => containsNodeType(child, type)));
};

it('the "Kitchen Sink view" page renders card, grid and menu nodes (API 1.3)', async () => {
	const host = await startHost();
	const tree = await host.renderPage('view', null);
	expect(containsNodeType(tree, 'grid')).toBe(true);
	expect(containsNodeType(tree, 'card')).toBe(true);
	expect(containsNodeType(tree, 'menu')).toBe(true);
	host.dispose();
});
