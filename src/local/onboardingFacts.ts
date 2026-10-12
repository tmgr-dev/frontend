import { domainEvents } from '@/utils/domainEvents';
import { BOARD_MOVE_META_PREFIX, createLocalApi } from './api';
import { dispatchLocal } from './dispatch';
import { TASK_SELECT, taskJson } from './serialize';
import type { LocalContext, SqlValue } from './types';

export const STARTERS_SEEDED_META = 'onboarding:starters_seeded';
export const STARTER_SETTINGS = '[{"key":"starter","value":"1"}]';
export const STARTER_TITLES = ['Plan your week', 'Try a timer on this task', 'Drag me to In progress'];

const NOT_STARTER = `settings NOT LIKE '%"key":"starter"%'`;
const IS_STARTER = `settings LIKE '%"key":"starter"%'`;

export const FACT_KEYS = [
	'has_task',
	'has_timer_minute',
	'has_board_move',
	'has_routine',
	'has_page',
	'local_agent_connected',
] as const;

export interface TokenLike {
	lastUsedAt?: string | null;
	revokedAt?: string | null;
}

const exists = async (ctx: LocalContext, sql: string, params: SqlValue[] = []) =>
	(await ctx.db.select(`SELECT 1 AS hit FROM ${sql} LIMIT 1`, params)).length > 0;

export const computeFacts = async (
	ctx: LocalContext,
	keys: readonly string[] = FACT_KEYS,
	tokens: () => Promise<TokenLike[]> = async () => [],
	taskId?: number,
): Promise<Record<string, boolean>> => {
	const facts: Record<string, boolean> = {};
	const scoped = Number.isFinite(taskId) && taskId !== undefined;
	for (const key of keys) {
		if (key === 'has_task') {
			facts[key] = await exists(ctx, `tasks WHERE deleted_at IS NULL AND ${NOT_STARTER}`);
		} else if (key === 'has_timer_minute') {
			facts[key] = scoped
				? await exists(ctx, `tasks WHERE deleted_at IS NULL AND common_time >= 60 AND id = ?`, [taskId as number])
				: await exists(ctx, `tasks WHERE deleted_at IS NULL AND common_time >= 60`);
		} else if (key === 'has_board_move') {
			facts[key] = scoped
				? await exists(ctx, `meta WHERE key = ?`, [`${BOARD_MOVE_META_PREFIX}${taskId}`])
				: await exists(ctx, `meta WHERE key LIKE ?`, [`${BOARD_MOVE_META_PREFIX}%`]);
		} else if (key === 'has_routine') {
			facts[key] = await exists(ctx, `routines WHERE deleted_at IS NULL`);
		} else if (key === 'has_page') {
			facts[key] = await exists(ctx, `pages WHERE deleted_at IS NULL AND type <> 'context'`);
		} else if (key === 'local_agent_connected') {
			facts[key] = (await tokens()).some((token) => !!token.lastUsedAt && !token.revokedAt);
		}
	}
	return facts;
};

const starterStatusId = async (ctx: LocalContext): Promise<number | null> => {
	const statuses = await ctx.db.select<{ id: number; type: string }>(
		`SELECT id, type FROM statuses ORDER BY sort_order, id`,
	);
	const pick =
		statuses.find((s) => s.type === 'default') ??
		statuses.find((s) => s.type === 'active') ??
		statuses.find((s) => s.type !== 'archived') ??
		statuses[0];
	return pick?.id ?? null;
};

export const seedStarters = async (ctx: LocalContext): Promise<number> => {
	const claim = await ctx.db.execute(`INSERT OR IGNORE INTO meta (key, value) VALUES (?, ?)`, [
		STARTERS_SEEDED_META,
		ctx.now().toISOString(),
	]);
	if (!claim.rowsAffected) return 0;
	const statusId = await starterStatusId(ctx);
	const now = ctx.now().toISOString();
	const ids: number[] = [];
	for (const title of STARTER_TITLES) {
		const result = await ctx.db.execute(
			`INSERT INTO tasks (title, status_id, settings, created_at, updated_at) VALUES (?, ?, ?, ?, ?)`,
			[title, statusId, STARTER_SETTINGS, now, now],
		);
		ids.push(Number(result.lastInsertId));
	}
	for (const id of ids) {
		const rows = await ctx.db.select(`${TASK_SELECT} WHERE t.id = ?`, [id]);
		if (!rows.length) continue;
		domainEvents.emit({
			type: 'task.created',
			workspaceId: ctx.workspace.id,
			taskId: id,
			task: taskJson(rows[0], ctx),
		});
	}
	return ids.length;
};

export const removeStarters = async (ctx: LocalContext): Promise<number> => {
	const rows = await ctx.db.select<{ id: number }>(
		`SELECT id FROM tasks WHERE deleted_at IS NULL AND ${IS_STARTER}`,
	);
	const api = createLocalApi();
	let removed = 0;
	for (const { id } of rows) {
		const result = await dispatchLocal(api, ctx, 'DELETE', `tasks/${id}`);
		if (!result || result.status >= 400) continue;
		removed += 1;
		domainEvents.emit({ type: 'task.deleted', workspaceId: ctx.workspace.id, taskId: id });
	}
	return removed;
};
