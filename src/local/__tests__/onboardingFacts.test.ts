import { createLocalApi } from '../api';
import { dispatchLocal } from '../dispatch';
import { createOnboardingHandler, type OnboardingStorage } from '../onboarding';
import { FACT_KEYS, computeFacts } from '../onboardingFacts';
import { ensureContextPage } from '../pages/service';
import { migrate } from '../schema';
import type { LocalContext } from '../types';
import { domainEvents } from '@/utils/domainEvents';
import { memoryDb, nodeSqliteAvailable } from './nodeDb';

const describeSqlite = nodeSqliteAvailable ? describe : describe.skip;

const makeCtx = async (code: string, id: number, clock: { now: Date }): Promise<LocalContext> => {
	const ctx: LocalContext = {
		db: memoryDb(),
		workspace: {
			id,
			name: code,
			code,
			schema_version: 0,
			created_at: '',
			path: `/tmp/${code}`,
			database: `/tmp/${code}/workspace.db`,
		},
		user: { id: 7, name: 'Me', email: 'me@example.com' },
		now: () => clock.now,
		files: { url: (key: string) => key, read: async () => new Blob([]), remove: async () => {} },
	};
	await migrate(ctx.db, clock.now.toISOString());
	await ensureContextPage(ctx);
	return ctx;
};

const memory = (): OnboardingStorage & { map: Map<string, string> } => {
	const map = new Map<string, string>();
	return {
		map,
		getItem: (key) => map.get(key) ?? null,
		setItem: (key, value) => void map.set(key, value),
	};
};

describeSqlite('local onboarding facts and starters on SQLite', () => {
	const clock = { now: new Date('2026-10-12T10:00:00Z') };
	const api = createLocalApi();
	let a: LocalContext;
	let b: LocalContext;
	let active: LocalContext;
	let tokens: { lastUsedAt: string | null }[];
	let handle: ReturnType<typeof createOnboardingHandler>;
	let storage: ReturnType<typeof memory>;
	const events: any[] = [];
	let stopEvents: () => void;

	const call = (ctx: LocalContext, method: string, url: string, body?: unknown) =>
		dispatchLocal(api, ctx, method, url, body).then((res) => res!.data.data);
	const facts = async (keys: string = '', taskId?: number) =>
		(await handle('GET', 'onboarding/facts', undefined, { keys, task_id: taskId })).data.data;

	beforeEach(async () => {
		clock.now = new Date('2026-10-12T10:00:00Z');
		a = await makeCtx('local-a', -1, clock);
		b = await makeCtx('local-b', -2, clock);
		active = a;
		tokens = [];
		storage = memory();
		events.length = 0;
		stopEvents = domainEvents.on((event) => events.push(event));
		handle = createOnboardingHandler(storage, {
			ctxFor: async (code) => (code ? [a, b].find((c) => c.workspace.code === code) ?? null : active),
			tokens: async () => tokens,
		});
	});
	afterEach(() => stopEvents());

	it('reports every fact false on a fresh workspace that holds the context page', async () => {
		const [{ n }] = await a.db.select<{ n: number }>(`SELECT COUNT(*) AS n FROM pages WHERE type = 'context'`);
		expect(n).toBe(1);
		expect(await computeFacts(a, FACT_KEYS)).toEqual({
			has_task: false,
			has_timer_minute: false,
			has_board_move: false,
			has_routine: false,
			has_page: false,
			local_agent_connected: false,
		});
		const state = (await handle('GET', 'onboarding')).data.data;
		expect(Object.values(state.facts).every((value) => value === false)).toBe(true);
	});

	it('turns each fact true after the matching action', async () => {
		const task = await call(a, 'POST', 'tasks', { title: 'Real work' });
		expect(await facts('has_task')).toEqual({ has_task: true });

		await call(a, 'POST', `tasks/${task.id}/countdown`);
		clock.now = new Date(clock.now.getTime() + 61_000);
		expect(await facts('has_timer_minute')).toEqual({ has_timer_minute: false });
		await dispatchLocal(api, a, 'DELETE', `tasks/${task.id}/countdown`);
		expect(await facts('has_timer_minute')).toEqual({ has_timer_minute: true });

		const statuses = await call(a, 'GET', 'workspaces/statuses');
		const active2 = statuses.find((s: any) => s.type === 'active');
		await call(a, 'PUT', `statuses/${active2.id}/tasks`, { task_ids: [task.id] });
		expect(await facts('has_board_move')).toEqual({ has_board_move: true });

		await call(a, 'POST', 'daily-routines/tasks', { title: 'Water plants' });
		expect(await facts('has_routine')).toEqual({ has_routine: true });

		await call(a, 'POST', 'pages', { title: 'Notes' });
		expect(await facts('has_page')).toEqual({ has_page: true });

		tokens = [{ lastUsedAt: null }];
		expect(await facts('local_agent_connected')).toEqual({ local_agent_connected: false });
		tokens = [{ lastUsedAt: '2026-10-12T10:05:00Z' }];
		expect(await facts('local_agent_connected')).toEqual({ local_agent_connected: true });
	});

	it('marks a board move only when the status really changes, by all three paths', async () => {
		const statuses = await call(a, 'GET', 'workspaces/statuses');
		const [first, second] = statuses;
		const t1 = await call(a, 'POST', 'tasks', { title: 'one', status_id: first.id });
		await call(a, 'PUT', `tasks/${t1.id}`, { title: 'one renamed', status_id: first.id });
		await call(a, 'PUT', `tasks/${t1.id}/${first.type}`);
		expect(await facts('has_board_move')).toEqual({ has_board_move: false });
		await call(a, 'PATCH', `tasks/${t1.id}`, { status_id: second.id });
		expect(await facts('has_board_move', t1.id)).toEqual({ has_board_move: true });

		const t2 = await call(a, 'POST', 'tasks', { title: 'two', status_id: first.id });
		expect(await facts('has_board_move', t2.id)).toEqual({ has_board_move: false });
		await call(a, 'PUT', `tasks/${t2.id}/${second.type}`);
		expect(await facts('has_board_move', t2.id)).toEqual({ has_board_move: true });
	});

	it('scopes has_timer_minute to the given task', async () => {
		const t1 = await call(a, 'POST', 'tasks', { title: 'one' });
		const t2 = await call(a, 'POST', 'tasks', { title: 'two' });
		await call(a, 'PUT', `tasks/${t1.id}/time`, { common_time: 120 });
		expect(await facts('has_timer_minute', t1.id)).toEqual({ has_timer_minute: true });
		expect(await facts('has_timer_minute', t2.id)).toEqual({ has_timer_minute: false });
		expect(await facts('has_timer_minute')).toEqual({ has_timer_minute: true });
	});

	it('answers only the requested keys and ignores unknown ones', async () => {
		expect(await facts('has_task,nonsense')).toEqual({ has_task: false });
	});

	it('seeds three human starters once: 3, then 0, flagged and not counted as tasks', async () => {
		const first = (await handle('POST', 'onboarding/intent', { intent: 'human', seed: true })).data.data;
		expect(first.starter_tasks_created).toBe(3);
		const second = (await handle('POST', 'onboarding/intent', { intent: 'human', seed: true })).data.data;
		expect(second.starter_tasks_created).toBe(0);

		const tasks = await call(a, 'GET', 'tasks');
		const rows = Array.isArray(tasks) ? tasks : tasks.data;
		expect(rows.map((t: any) => t.title).sort()).toEqual(
			['Drag me to In progress', 'Plan your week', 'Try a timer on this task'].sort(),
		);
		expect(rows.every((t: any) => t.settings.some((s: any) => s.key === 'starter'))).toBe(true);
		expect(await facts('has_task')).toEqual({ has_task: false });
		expect(events.filter((e) => e.type === 'task.created').map((e) => e.workspaceId)).toEqual([-1, -1, -1]);

		await call(a, 'PUT', `tasks/${rows[0].id}/time`, { common_time: 300 });
		expect(await facts('has_timer_minute')).toEqual({ has_timer_minute: true });
		const saved = JSON.parse(storage.map.get('tmgr:onboarding:v1')!);
		expect(saved.starter_workspace).toBe('local-a');
		expect(typeof saved.starter_seeded_at).toBe('string');
	});

	it('does not seed for seed:false, the agent intent, or a workspace already claimed', async () => {
		expect((await handle('POST', 'onboarding/intent', { intent: 'human', seed: false })).data.data.starter_tasks_created).toBe(0);
		expect((await handle('POST', 'onboarding/intent', { intent: 'agent' })).data.data.starter_tasks_created).toBe(0);
		const [{ n }] = await a.db.select<{ n: number }>(`SELECT COUNT(*) AS n FROM tasks`);
		expect(n).toBe(0);
	});

	it('removes starters from the seeded workspace while another one is active', async () => {
		await handle('POST', 'onboarding/intent', { intent: 'human', seed: true });
		await call(a, 'POST', 'tasks', { title: 'Keep me' });
		await call(b, 'POST', 'tasks', { title: 'Other workspace' });
		active = b;
		events.length = 0;
		const removed = (await handle('DELETE', 'onboarding/starter-tasks')).data.data;
		expect(removed).toEqual({ removed: 3 });
		const left = await a.db.select<{ title: string; deleted_at: string | null }>(
			`SELECT title, deleted_at FROM tasks WHERE deleted_at IS NULL`,
		);
		expect(left.map((t) => t.title)).toEqual(['Keep me']);
		const soft = await a.db.select(`SELECT id FROM tasks WHERE deleted_at IS NOT NULL`);
		expect(soft).toHaveLength(3);
		const other = await b.db.select(`SELECT id FROM tasks WHERE deleted_at IS NULL`);
		expect(other).toHaveLength(1);
		expect(events.filter((e) => e.type === 'task.deleted')).toHaveLength(3);
		expect((await handle('DELETE', 'onboarding/starter-tasks')).data.data).toEqual({ removed: 0 });
	});

	it('stores the starter ids and never touches a user task carrying a starter key', async () => {
		await handle('POST', 'onboarding/intent', { intent: 'human', seed: true });
		const [{ value }] = await a.db.select<{ value: string }>(`SELECT value FROM meta WHERE key = 'onboarding:starter_ids'`);
		expect(JSON.parse(value)).toHaveLength(3);
		const mine = await call(a, 'POST', 'tasks', { title: 'Mine', settings: [{ key: 'starter', value: '1' }] });
		expect(await facts('has_task')).toEqual({ has_task: true });
		expect((await handle('DELETE', 'onboarding/starter-tasks')).data.data).toEqual({ removed: 3 });
		const left = await a.db.select<{ id: number }>(`SELECT id FROM tasks WHERE deleted_at IS NULL`);
		expect(left.map((t) => t.id)).toEqual([mine.id]);
	});

	it('removes nothing when no starter ids were recorded', async () => {
		await call(a, 'POST', 'tasks', { title: 'Mine', settings: [{ key: 'starter', value: '1' }] });
		await a.db.execute(`DELETE FROM meta WHERE key = 'onboarding:starter_ids'`);
		storage.setItem('tmgr:onboarding:v1', JSON.stringify({ starter_workspace: 'local-a', starter_seeded_at: 'x' }));
		expect((await handle('DELETE', 'onboarding/starter-tasks')).data.data).toEqual({ removed: 0 });
		expect(await a.db.select(`SELECT id FROM tasks WHERE deleted_at IS NULL`)).toHaveLength(1);
	});

	it('drops only local_agent_connected when the token list fails', async () => {
		await call(a, 'POST', 'tasks', { title: 'Real work' });
		const facts = await computeFacts(a, FACT_KEYS, async () => {
			throw new Error('unsupported');
		});
		expect(facts.has_task).toBe(true);
		expect(facts.local_agent_connected).toBe(false);
	});

	it('passes the query params through to the facts handler', async () => {
		const seen: unknown[] = [];
		const spy = createOnboardingHandler(storage, {
			ctxFor: async () => {
				seen.push('ctx');
				return a;
			},
			tokens: async () => [],
		});
		const t = await call(a, 'POST', 'tasks', { title: 'x' });
		await call(a, 'PUT', `tasks/${t.id}/time`, { common_time: 90 });
		const res = await spy('GET', 'onboarding/facts', undefined, { keys: 'has_timer_minute', task_id: t.id });
		expect(res.data.data).toEqual({ has_timer_minute: true });
		expect(seen).toHaveLength(1);
	});
});
