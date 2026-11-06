import type { LocalContext, LocalDb } from './types';

export const DEMO_META = 'onboarding:demo';
export const DEMO_WORKSPACE_NAME = 'Demo: Launch week';
export const DEMO_AGENT = 'demo-agent';

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
	const now = ctx.now().toISOString();
	const claim = await ctx.db.execute(
		`INSERT OR IGNORE INTO meta (key, value) VALUES (?, ?)`,
		[DEMO_META, now],
	);
	if (!claim.rowsAffected) return false;
	await ctx.db.execute(
		`UPDATE statuses SET name = 'Todo' WHERE type = 'default'`,
	);
	const statuses = await ctx.db.select<{ id: number; name: string }>(
		`SELECT id, name FROM statuses`,
	);
	if (!statuses.some((s) => s.name === 'Review')) {
		await ctx.db.execute(
			`INSERT INTO statuses (name, type, color, sort_order, created_at, updated_at)
			 VALUES ('Review', 'active', '#a855f7', 2, ?, ?)`,
			[now, now],
		);
		await ctx.db.execute(
			`UPDATE statuses SET sort_order = sort_order + 1 WHERE type IN ('completed', 'archived')`,
		);
	}
	const byName = new Map(
		(
			await ctx.db.select<{ id: number; name: string }>(
				`SELECT id, name FROM statuses`,
			)
		).map((s) => [s.name, s.id]),
	);
	let reviewTaskId = 0;
	for (const [index, card] of BOARD.entries()) {
		const result = await ctx.db.execute(
			`INSERT INTO tasks (title, description, status_id, sort_order, settings, created_at, updated_at)
			 VALUES (?, ?, ?, ?, '[]', ?, ?)`,
			[
				card.title,
				card.description ?? null,
				byName.get(card.status) ?? null,
				index + 1,
				now,
				now,
			],
		);
		if (card.status === 'Review') reviewTaskId = Number(result.lastInsertId);
	}
	await ctx.db.execute(
		`INSERT INTO agent_work_runs
		 (task_id, agent, model, branch, status, started_at, ended_at, duration_seconds, summary, commits, tests, actor_kind, actor_id, version, created_at, updated_at)
		 VALUES (?, ?, NULL, 'fix/login-redirect', 'succeeded', ?, ?, 312, ?, ?, ?, 'user', '-1', 1, ?, ?)`,
		[
			reviewTaskId,
			DEMO_AGENT,
			now,
			now,
			'Fixed the redirect after sign-in; kept the requested page in the URL.',
			JSON.stringify([
				{ sha: 'a1b2c3d', message: 'fix: keep the return path after sign-in' },
			]),
			JSON.stringify({ passed: 14, failed: 0, command: 'npm test' }),
			now,
			now,
		],
	);
	return true;
};
