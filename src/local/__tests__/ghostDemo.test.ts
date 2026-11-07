import { createLocalApi } from '../api';
import {
	DEMO_AGENT,
	finishGhostRun,
	recoverDemo,
	resetDemoCard,
	seedDemo,
} from '../demoSeed';
import { dispatchLocal } from '../dispatch';
import { computeFacts } from '../onboardingFacts';
import { migrate } from '../schema';
import type { LocalContext, LocalDb } from '../types';
import { memoryDb, nodeSqliteAvailable } from './nodeDb';

const describeDb = nodeSqliteAvailable ? describe : describe.skip;
const NOW = '2026-11-06T10:00:00Z';

const ctxOf = (db: LocalDb, demo = true): LocalContext => ({
	db,
	workspace: {
		id: -9,
		name: 'Demo',
		code: 'demo',
		schema_version: 0,
		created_at: '',
		path: '/tmp/demo',
		database: '/tmp/demo/workspace.db',
		demo,
	},
	user: { id: 0, name: '', email: '' },
	now: () => new Date(NOW),
	accountless: true,
	files: {
		url: (key: string) => key,
		read: async () => new Blob([]),
		remove: async () => {},
	},
});

const seeded = async (demo = true): Promise<LocalContext> => {
	const db = memoryDb();
	await migrate(db, NOW);
	const ctx = ctxOf(db, demo);
	if (demo) await seedDemo(ctx);
	return ctx;
};

const api = createLocalApi();
const call = async (
	ctx: LocalContext,
	method: string,
	url: string,
	body?: unknown,
) => {
	const result = await dispatchLocal(api, ctx, method, url, body);
	return { status: result?.status, data: (result?.data as any)?.data };
};

const one = async (db: LocalDb, sql: string, params: any[] = []) =>
	(await db.select<any>(sql, params))[0];

describeDb('agent work in a seeded demo', () => {
	it('accepts a new agent run in the accountless demo', async () => {
		const ctx = await seeded();
		const card = await resetDemoCard(ctx);
		const run = await call(ctx, 'POST', `tasks/${card!.taskId}/agent-work`, {
			agent: DEMO_AGENT,
			branch: 'fix/login-redirect',
		});
		expect(run.status).toBe(201);
		expect(run.data.status).toBe('running');
		expect(run.data.agent).toBe(DEMO_AGENT);
	});
});

describeDb('resetDemoCard', () => {
	it('puts the card back in Todo with no timer, no time and no demo-agent runs', async () => {
		const ctx = await seeded();
		const card = await resetDemoCard(ctx);
		expect(card).not.toBeNull();
		const row = await one(
			ctx.db,
			`SELECT s.name AS status, t.start_time, t.common_time, t.title FROM tasks t
			 JOIN statuses s ON s.id = t.status_id WHERE t.id = ?`,
			[card!.taskId],
		);
		expect(row).toMatchObject({
			status: 'Todo',
			start_time: 0,
			common_time: 0,
			title: 'Fix login redirect',
		});
		expect(
			(await one(ctx.db, `SELECT COUNT(*) AS n FROM agent_work_runs`)).n,
		).toBe(0);
	});

	it('returns the ids the scenario moves the card between', async () => {
		const ctx = await seeded();
		const card = await resetDemoCard(ctx);
		const names = await Promise.all(
			[card!.todo, card!.inProgress, card!.review].map(
				async (id) =>
					(await one(ctx.db, `SELECT name FROM statuses WHERE id = ?`, [id]))
						.name,
			),
		);
		expect(names).toEqual(['Todo', 'In progress', 'Review']);
	});

	it('does nothing in a database that is not the demo', async () => {
		const ctx = await seeded(false);
		expect(await resetDemoCard(ctx)).toBeNull();
	});
});

describeDb('recoverDemo', () => {
	it('stops a running timer, drops a dangling demo-agent run and puts the card back in Todo', async () => {
		const ctx = await seeded();
		const card = (await resetDemoCard(ctx))!;
		await call(ctx, 'POST', `tasks/${card.taskId}/countdown`);
		await call(ctx, 'POST', `tasks/${card.taskId}/agent-work`, {
			agent: DEMO_AGENT,
		});
		const touched = await recoverDemo(ctx);
		expect(touched).toContain(card.taskId);
		expect(
			(
				await one(ctx.db, `SELECT start_time FROM tasks WHERE id = ?`, [
					card.taskId,
				])
			).start_time,
		).toBe(0);
		expect(
			(await one(ctx.db, `SELECT COUNT(*) AS n FROM agent_work_runs`)).n,
		).toBe(0);
		expect(
			(
				await one(
					ctx.db,
					`SELECT s.name AS status FROM tasks t JOIN statuses s ON s.id = t.status_id WHERE t.id = ?`,
					[card.taskId],
				)
			).status,
		).toBe('Todo');
	});

	it('leaves a timer and time the user tracked on the card alone when no ghost run was in progress', async () => {
		const ctx = await seeded();
		const card = (await resetDemoCard(ctx))!;
		await finishGhostRun(ctx);
		await ctx.db.execute(
			`UPDATE tasks SET start_time = 100, common_time = 75 WHERE id = ?`,
			[card.taskId],
		);
		expect(await recoverDemo(ctx)).toEqual([]);
		expect(
			await one(
				ctx.db,
				`SELECT start_time, common_time FROM tasks WHERE id = ?`,
				[card.taskId],
			),
		).toEqual({ start_time: 100, common_time: 75 });
	});

	it('does nothing on a demo that was never played', async () => {
		const ctx = await seeded();
		expect(await recoverDemo(ctx)).toEqual([]);
		expect(
			(await one(ctx.db, `SELECT COUNT(*) AS n FROM agent_work_runs`)).n,
		).toBe(1);
	});

	it('recovers only once', async () => {
		const ctx = await seeded();
		const card = (await resetDemoCard(ctx))!;
		expect(await recoverDemo(ctx)).toEqual([card.taskId]);
		expect(await recoverDemo(ctx)).toEqual([]);
	});

	it('leaves other agents and finished runs alone', async () => {
		const ctx = await seeded();
		const card = (await resetDemoCard(ctx))!;
		await call(ctx, 'POST', `tasks/${card.taskId}/agent-work`, {
			agent: 'claude-code',
		});
		await recoverDemo(ctx);
		expect(
			(await one(ctx.db, `SELECT status FROM agent_work_runs`)).status,
		).toBe('running');
	});

	it('keeps a timer the user started on another card', async () => {
		const ctx = await seeded();
		const card = (await resetDemoCard(ctx))!;
		const [other] = await ctx.db.select<{ id: number }>(
			`SELECT id FROM tasks WHERE id <> ? LIMIT 1`,
			[card.taskId],
		);
		await call(ctx, 'POST', `tasks/${other.id}/countdown`);
		await call(ctx, 'POST', `tasks/${card.taskId}/countdown`);
		await recoverDemo(ctx);
		const rows = await ctx.db.select<{ id: number; start_time: number }>(
			`SELECT id, start_time FROM tasks WHERE id IN (?, ?)`,
			[card.taskId, other.id],
		);
		const byId = Object.fromEntries(rows.map((r) => [r.id, r.start_time]));
		expect(byId[card.taskId]).toBe(0);
		expect(byId[other.id]).toBeGreaterThan(0);
	});

	it('clears the time an interrupted timer stop left on the card', async () => {
		const ctx = await seeded();
		const card = (await resetDemoCard(ctx))!;
		await ctx.db.execute(
			`UPDATE tasks SET start_time = 100, common_time = 75 WHERE id = ?`,
			[card.taskId],
		);
		await recoverDemo(ctx);
		const row = await one(
			ctx.db,
			`SELECT start_time, common_time FROM tasks WHERE id = ?`,
			[card.taskId],
		);
		expect(row).toEqual({ start_time: 0, common_time: 0 });
		expect(
			(await computeFacts(ctx, ['has_timer_minute'])).has_timer_minute,
		).toBe(false);
	});

	it('touches nothing outside the demo', async () => {
		const ctx = await seeded(false);
		expect(await recoverDemo(ctx)).toEqual([]);
	});
});

describeDb('ghost writes', () => {
	it('do not leave a board move behind, unlike a user move', async () => {
		const ctx = await seeded();
		const card = (await resetDemoCard(ctx))!;
		ctx.ghost = true;
		await call(ctx, 'PUT', `tasks/${card.taskId}/${card.inProgress}`);
		expect((await computeFacts(ctx, ['has_board_move'])).has_board_move).toBe(
			false,
		);
		ctx.ghost = false;
		await call(ctx, 'PUT', `tasks/${card.taskId}/${card.review}`);
		expect((await computeFacts(ctx, ['has_board_move'])).has_board_move).toBe(
			true,
		);
	});
});
