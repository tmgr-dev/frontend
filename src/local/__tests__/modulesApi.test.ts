import { parseModulesPayload } from '@/utils/modules';
import { createLocalApi } from '../api';
import { dispatchLocal } from '../dispatch';
import { MODULES, PRESETS } from '../modulesCatalog';
import { migrate } from '../schema';
import type { LocalContext } from '../types';
import { memoryDb, nodeSqliteAvailable } from './nodeDb';

const describeSqlite = nodeSqliteAvailable ? describe : describe.skip;

const storage = new Map<string, string>();
(globalThis as any).localStorage = {
	getItem: (key: string) => storage.get(key) ?? null,
	setItem: (key: string, value: string) => storage.set(key, value),
	removeItem: (key: string) => storage.delete(key),
	clear: () => storage.clear(),
};

describeSqlite('local modules API', () => {
	let ctx: LocalContext;
	const api = createLocalApi();
	const data = async (method: string, url: string, body?: unknown) => {
		const res = await dispatchLocal(api, ctx, method, url, body);
		if (!res) throw new Error(`no local route for ${method} ${url}`);
		return res.data.data;
	};
	const payload = async () => parseModulesPayload({ data: await data('GET', 'workspaces/-42/modules') });
	const enabled = (modules: { key: string; enabled?: boolean }[]) =>
		Object.fromEntries(modules.map((m) => [m.key, m.enabled]));

	beforeEach(async () => {
		storage.clear();
		ctx = {
			db: memoryDb(),
			workspace: {
				id: -42,
				name: 'Personal',
				code: 'local-personal',
				schema_version: 0,
				created_at: '',
				path: '/tmp/x',
				database: '/tmp/x/workspace.db',
			},
			user: { id: 7, name: 'Yurij', email: '' },
			now: () => new Date('2026-10-11T10:00:00Z'),
			files: { url: (key: string) => key, read: async () => new Blob([]), remove: async () => {} },
		};
		await migrate(ctx.db, '2026-10-11T10:00:00Z');
	});

	it('matches the legacy feature toggles for a fresh workspace', async () => {
		const legacy = await data('GET', 'workspaces/-42/feature-toggles');
		const modules = enabled((await payload()).modules);

		for (const [key, toggle] of Object.entries<any>(legacy)) {
			expect({ key, enabled: modules[key] }).toEqual({ key, enabled: toggle.enabled });
		}
	});

	it('keeps Workflow on by default and lets the board follow it', async () => {
		expect(enabled((await payload()).modules)).toMatchObject({ workflow: true, board: true });

		await data('PUT', 'workspaces/-42/modules', { preset: 'personal' });
		expect(enabled((await payload()).modules)).toMatchObject({ workflow: false, board: false });

		await data('PUT', 'workspaces/-42/modules', { modules: { board: true } });
		expect(enabled((await payload()).modules)).toMatchObject({ workflow: true, board: true });
	});

	it('shows every module of the catalog and keeps cloud-only ones off', async () => {
		const { modules } = await payload();

		const cloudOnly = MODULES.filter((m) => !m.local && m.scope === 'workspace').map((m) => m.key);
		expect(cloudOnly).toEqual(
			expect.arrayContaining(['dashboard', 'pomodoro', 'exports', 'ai.assistant', 'cursor', 'github', 'telegram', 'import']),
		);
		for (const key of cloudOnly) {
			const entry = modules.find((m) => m.key === key)!;
			expect(entry).toMatchObject({ enabled: false, requires_account: true, local: false });
		}
		expect(modules.find((m) => m.key === 'pages')).toMatchObject({ local: true, enabled: true });
		expect(modules.find((m) => m.key === 'pages')?.requires_account).toBeUndefined();
	});

	it('leaves user-scope modules to the server unless there is no account', async () => {
		expect((await payload()).modules.some((m) => m.scope === 'user')).toBe(false);

		ctx.accountless = true;
		const { modules } = await payload();

		expect(modules.find((m) => m.key === 'mcp')).toMatchObject({ scope: 'user', enabled: true });
		expect(modules.find((m) => m.key === 'alerts')).toMatchObject({ enabled: false, requires_account: true });
	});

	it('saves a preset and ignores cloud-only keys', async () => {
		const result = await data('PUT', 'workspaces/-42/modules', {
			preset: 'personal',
			modules: { pomodoro: true, graph: true },
		});

		const map = enabled(parseModulesPayload({ data: result }).modules);
		expect(map.pages).toBe(false);
		expect(map['task.checkpoints']).toBe(true);
		expect(map.graph).toBe(true);
		expect(map.pomodoro).toBe(false);
	});

	it('has no team preset and no cloud-only key in any preset', async () => {
		expect(PRESETS.map((p) => p.key)).toEqual(['minimal', 'personal', 'developer', 'everything']);
		const local = new Set(MODULES.filter((m) => m.local).map((m) => m.key));
		for (const preset of PRESETS) expect(preset.modules.every((key) => local.has(key))).toBe(true);
	});

	it('toggles a module through the feature-toggles route', async () => {
		await data('PUT', 'workspaces/-42/feature-toggles', { features: { pages: false, 'task.assignees': true } });

		const map = enabled((await payload()).modules);
		expect(map.pages).toBe(false);
		expect(map['task.assignees']).toBe(true);
	});

	it('hides a module for me', async () => {
		const result = parseModulesPayload({ data: await data('PUT', 'workspaces/-42/modules/hidden', { pages: true }) });

		expect(result.modules.find((m) => m.key === 'pages')?.hidden).toBe(true);
	});
});
