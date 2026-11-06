import type { BatchStatement, LocalContext, LocalDb } from './types';

export const DEMO_META = 'onboarding:demo';
export const DEMO_WORKSPACE_NAME = 'Demo: Launch week';
export const DEMO_AGENT = 'demo-agent';
export const GHOST_CARD = 'Fix login redirect';
const REVIEW_TITLE = GHOST_CARD;

const BOARD: Array<{ title: string; status: string; description?: string }> = [
	{ title: 'Write the launch announcement', status: 'Todo' },
	{ title: 'Prepare the pricing page copy', status: 'Todo' },
	{ title: 'Record a 60-second walkthrough', status: 'Todo' },
	{
		title: 'Fix login redirect',
		status: 'Review',
		description:
			'After signing in, users land on the home page instead of the page they asked for.',
	},
	{ title: 'Set up error alerts', status: 'In progress' },
	{ title: 'Choose the launch date', status: 'Done' },
	{ title: 'Draft the changelog', status: 'Done' },
];

export const isDemoDb = async (db: LocalDb): Promise<boolean> =>
	(await db.select(`SELECT 1 AS found FROM meta WHERE key = ?`, [DEMO_META]))
		.length > 0;

export const seedDemo = async (ctx: LocalContext): Promise<boolean> => {
	if (await isDemoDb(ctx.db)) return false;
	const now = ctx.now().toISOString();
	const statusId = `(SELECT id FROM statuses WHERE name = ? ORDER BY id LIMIT 1)`;
	const statements: BatchStatement[] = [
		{ sql: `UPDATE statuses SET name = 'Todo' WHERE type = 'default'` },
		{
			sql: `UPDATE statuses SET sort_order = sort_order + 1
				WHERE type IN ('completed', 'archived')
				AND NOT EXISTS (SELECT 1 FROM statuses WHERE name = 'Review')`,
		},
		{
			sql: `INSERT INTO statuses (name, type, color, sort_order, created_at, updated_at)
				SELECT 'Review', 'active', '#a855f7', 3, ?, ?
				WHERE NOT EXISTS (SELECT 1 FROM statuses WHERE name = 'Review')`,
			params: [now, now],
		},
		...BOARD.map((card, index): BatchStatement => ({
			sql: `INSERT INTO tasks (title, description, status_id, sort_order, settings, created_at, updated_at)
					VALUES (?, ?, ${statusId}, ?, '[]', ?, ?)`,
			params: [
				card.title,
				card.description ?? null,
				card.status,
				index + 1,
				now,
				now,
			],
		})),
		{
			sql: `INSERT INTO agent_work_runs
				(task_id, agent, model, branch, status, started_at, ended_at, duration_seconds, summary, commits, tests, actor_kind, actor_id, version, created_at, updated_at)
				VALUES ((SELECT id FROM tasks WHERE title = ? ORDER BY id LIMIT 1), ?, NULL, 'fix/login-redirect', 'succeeded', ?, ?, 312, ?, ?, ?, 'user', '-1', 1, ?, ?)`,
			params: [
				REVIEW_TITLE,
				DEMO_AGENT,
				now,
				now,
				'Fixed the redirect after sign-in; kept the requested page in the URL.',
				JSON.stringify([
					{
						sha: 'a1b2c3d',
						message: 'fix: keep the return path after sign-in',
					},
				]),
				JSON.stringify({ passed: 14, failed: 0, command: 'npm test' }),
				now,
				now,
			],
		},
		{
			sql: `INSERT OR IGNORE INTO meta (key, value) VALUES (?, ?)`,
			params: [DEMO_META, now],
			expectChanges: true,
		},
	];
	const { failedAt } = await ctx.db.batch(statements);
	return failedAt === null;
};

export interface DemoCard {
	taskId: number;
	todo: number;
	inProgress: number;
	review: number;
}

const statusIdOf = async (db: LocalDb, name: string): Promise<number | null> =>
	(
		await db.select<{ id: number }>(
			`SELECT id FROM statuses WHERE name = ? ORDER BY id LIMIT 1`,
			[name],
		)
	)[0]?.id ?? null;

export const resetDemoCard = async (
	ctx: LocalContext,
): Promise<DemoCard | null> => {
	if (!(await isDemoDb(ctx.db))) return null;
	const [task] = await ctx.db.select<{ id: number }>(
		`SELECT id FROM tasks WHERE title = ? AND deleted_at IS NULL ORDER BY id LIMIT 1`,
		[GHOST_CARD],
	);
	const [todo, inProgress, review] = await Promise.all(
		['Todo', 'In progress', 'Review'].map((name) => statusIdOf(ctx.db, name)),
	);
	if (!task || todo === null || inProgress === null || review === null)
		return null;
	const { failedAt } = await ctx.db.batch([
		{
			sql: `DELETE FROM agent_work_runs WHERE task_id = ? AND agent = ?`,
			params: [task.id, DEMO_AGENT],
		},
		{
			sql: `UPDATE tasks SET status_id = ?, start_time = 0, common_time = 0, updated_at = ? WHERE id = ?`,
			params: [todo, ctx.now().toISOString(), task.id],
			expectChanges: true,
		},
	]);
	if (failedAt !== null) return null;
	return { taskId: task.id, todo, inProgress, review };
};

export const recoverDemo = async (ctx: LocalContext): Promise<number[]> => {
	if (!(await isDemoDb(ctx.db))) return [];
	const now = ctx.now().toISOString();
	const timed = await ctx.db.select<{ id: number }>(
		`SELECT id FROM tasks WHERE start_time > 0 AND title = ?`,
		[GHOST_CARD],
	);
	const dangling = await ctx.db.select<{ task_id: number }>(
		`SELECT DISTINCT task_id FROM agent_work_runs WHERE status = 'running' AND agent = ?`,
		[DEMO_AGENT],
	);
	if (timed.length)
		await ctx.db.execute(
			`UPDATE tasks SET start_time = 0, updated_at = ? WHERE start_time > 0 AND title = ?`,
			[now, GHOST_CARD],
		);
	if (dangling.length)
		await ctx.db.execute(
			`UPDATE agent_work_runs SET status = 'abandoned', ended_at = COALESCE(updated_at, started_at),
				duration_seconds = CAST((julianday(COALESCE(updated_at, started_at)) - julianday(started_at)) * 86400 AS INTEGER),
				version = version + 1, updated_at = ?
			 WHERE status = 'running' AND agent = ?`,
			[now, DEMO_AGENT],
		);
	const touched = [
		...new Set([...timed.map((t) => t.id), ...dangling.map((r) => r.task_id)]),
	];
	if (touched.length)
		await ctx.db.execute(
			`UPDATE tasks SET common_time = 0 WHERE title = ? AND common_time > 0`,
			[GHOST_CARD],
		);
	return touched;
};
