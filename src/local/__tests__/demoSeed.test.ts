import { DEMO_AGENT, isDemoDb, seedDemo } from '../demoSeed';
import { computeFacts } from '../onboardingFacts';
import { migrate } from '../schema';
import type { LocalContext, LocalDb } from '../types';
import { memoryDb, nodeSqliteAvailable } from './nodeDb';

const describeDb = nodeSqliteAvailable ? describe : describe.skip;
const NOW = '2026-11-06T10:00:00Z';

const ctxOf = (db: LocalDb): LocalContext => ({
	db,
	workspace: {
		id: -9,
		name: 'Demo',
		code: 'demo',
		schema_version: 0,
		created_at: '',
		path: '/tmp/demo',
		database: '/tmp/demo/workspace.db',
		demo: true,
	},
	user: { id: 0, name: '', email: '' },
	now: () => new Date(NOW),
	files: {
		url: (key: string) => key,
		read: async () => new Blob([]),
		remove: async () => {},
	},
});

const migrated = async (): Promise<LocalContext> => {
	const db = memoryDb();
	await migrate(db, NOW);
	return ctxOf(db);
};

const count = async (db: LocalDb, sql: string): Promise<number> =>
	Number((await db.select<{ n: number }>(sql))[0]?.n ?? 0);

describeDb('seedDemo', () => {
	it('seeds the board once and reports false on the second call without adding rows', async () => {
		const ctx = await migrated();
		expect(await seedDemo(ctx)).toBe(true);
		const tasks = await count(ctx.db, `SELECT COUNT(*) AS n FROM tasks`);
		const runs = await count(
			ctx.db,
			`SELECT COUNT(*) AS n FROM agent_work_runs`,
		);
		expect(await seedDemo(ctx)).toBe(false);
		expect(await count(ctx.db, `SELECT COUNT(*) AS n FROM tasks`)).toBe(tasks);
		expect(
			await count(ctx.db, `SELECT COUNT(*) AS n FROM agent_work_runs`),
		).toBe(runs);
	});

	it('creates the Todo, In progress, Review and Done columns and places the cards in them', async () => {
		const ctx = await migrated();
		await seedDemo(ctx);
		const columns = await ctx.db.select<{ name: string }>(
			`SELECT name FROM statuses ORDER BY sort_order, id`,
		);
		expect(columns.map((c) => c.name)).toEqual(
			expect.arrayContaining(['Todo', 'In progress', 'Review', 'Done']),
		);
		const cards = await ctx.db.select<{ status: string; n: number }>(
			`SELECT s.name AS status, COUNT(*) AS n FROM tasks t
			 JOIN statuses s ON s.id = t.status_id GROUP BY s.name ORDER BY s.name`,
		);
		expect(Object.fromEntries(cards.map((c) => [c.status, c.n]))).toEqual({
			Done: 2,
			'In progress': 1,
			Review: 1,
			Todo: 3,
		});
	});

	it('writes exactly one succeeded agent run by the demo agent on behalf of the user', async () => {
		const ctx = await migrated();
		await seedDemo(ctx);
		const runs = await ctx.db.select<Record<string, unknown>>(
			`SELECT agent, actor_kind, actor_id, status FROM agent_work_runs`,
		);
		expect(runs).toEqual([
			{
				agent: DEMO_AGENT,
				actor_kind: 'user',
				actor_id: '-1',
				status: 'succeeded',
			},
		]);
	});

	it('creates no personas, workspace personas, board move notes, timer minutes or routines', async () => {
		const ctx = await migrated();
		await seedDemo(ctx);
		expect(await count(ctx.db, `SELECT COUNT(*) AS n FROM personas`)).toBe(0);
		expect(
			await count(ctx.db, `SELECT COUNT(*) AS n FROM workspace_personas`),
		).toBe(0);
		expect(
			await count(
				ctx.db,
				`SELECT COUNT(*) AS n FROM meta WHERE key LIKE 'onboarding:board_move:%'`,
			),
		).toBe(0);
		expect(
			await count(
				ctx.db,
				`SELECT COUNT(*) AS n FROM tasks WHERE common_time >= 60`,
			),
		).toBe(0);
		expect(await count(ctx.db, `SELECT COUNT(*) AS n FROM routines`)).toBe(0);
	});

	it('is recognised as a demo database only after it is seeded', async () => {
		const ctx = await migrated();
		expect(await isDemoDb(ctx.db)).toBe(false);
		await seedDemo(ctx);
		expect(await isDemoDb(ctx.db)).toBe(true);
	});

	it('leaves onboarding facts about the user all false after seeding', async () => {
		const ctx = await migrated();
		await seedDemo(ctx);
		expect(
			await computeFacts(ctx, [
				'has_board_move',
				'has_timer_minute',
				'has_routine',
			]),
		).toEqual({
			has_board_move: false,
			has_timer_minute: false,
			has_routine: false,
		});
	});
});
