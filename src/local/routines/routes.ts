import type { LocalRouter } from '../router';
import { TASK_SELECT, taskJson } from '../serialize';
import { LocalHttpError, type LocalContext } from '../types';
import { normalizeRecurrence, localDate } from './recurrence';
import { instanceJson, patternJson, routineJson } from './serialize';
import {
	archiveRoutine,
	completeForDate,
	convertRoutineToTask,
	createRecurringRoutine,
	createRoutine,
	deleteInstance,
	destroyRoutine,
	expandRange,
	loadActiveRoutine,
	notFoundRoutine,
	quickCreateRoutine,
	rescheduleInstance,
	rescheduleVirtual,
	resolveReschedule,
	savePattern,
	setInstanceStatus,
	updateRoutine,
} from './service';

const requireActive = async (ctx: LocalContext, id: number) => {
	const row = await loadActiveRoutine(ctx, id);
	if (!row) throw notFoundRoutine();
	return row;
};

export const addRoutineRoutes = (router: LocalRouter): LocalRouter =>
	router
		// ── workspace + counts ────────────────────────────────────────────────
		.add('GET', 'daily-routines/workspace', ({ ctx }) => ({ workspace_id: ctx.workspace.id }))
		.add('GET', 'daily-routines/tasks/count', async ({ ctx }) => {
			const [{ n }] = await ctx.db.select<{ n: number }>(
				`SELECT COUNT(*) AS n FROM routines WHERE deleted_at IS NULL AND archived_at IS NULL`,
			);
			return { count: Number(n) };
		})
		.add('GET', 'daily-routines/tasks/archived/count', async ({ ctx }) => {
			const [{ n }] = await ctx.db.select<{ n: number }>(
				`SELECT COUNT(*) AS n FROM routines WHERE deleted_at IS NULL AND archived_at IS NOT NULL`,
			);
			return { count: Number(n) };
		})
		.add('GET', 'daily-routines/tasks/completed/count', async ({ ctx }) => {
			const [{ n }] = await ctx.db.select<{ n: number }>(
				`SELECT COUNT(*) AS n FROM routine_instances ri
				 JOIN routines r ON r.id = ri.routine_id
				 WHERE r.deleted_at IS NULL AND r.archived_at IS NULL AND ri.status = 'COMPLETED'`,
			);
			return { count: Number(n) };
		})
		// ── task CRUD ────────────────────────────────────────────────────────
		.add(
			'POST',
			'daily-routines/tasks',
			async ({ ctx, body }) => routineJson(await createRoutine(ctx, body ?? {}), ctx),
			201,
		)
		.add(
			'POST',
			'daily-routines/tasks/recurring',
			async ({ ctx, body }) => routineJson(await createRecurringRoutine(ctx, body ?? {}), ctx),
			201,
		)
		.add(
			'POST',
			'daily-routines/tasks/quick',
			async ({ ctx, body }) => routineJson(await quickCreateRoutine(ctx, body ?? {}), ctx),
			201,
		)
		.add('GET', 'daily-routines/tasks/upcoming', async ({ ctx, query }) => {
			const limit = Math.max(1, Number(query.get('limit') ?? 100) || 100);
			const from = `${localDate(ctx.now())}T00:00:00`;
			const rows = await ctx.db.select<any>(
				`SELECT ri.* FROM routine_instances ri JOIN routines r ON r.id = ri.routine_id
				 WHERE r.deleted_at IS NULL AND r.archived_at IS NULL AND ri.scheduled_for >= ?
				 ORDER BY ri.scheduled_for ASC LIMIT ?`,
				[from, limit],
			);
			return rows.map(instanceJson);
		})
		.add('GET', 'daily-routines/tasks', async ({ ctx }) => {
			const rows = await ctx.db.select<any>(
				`SELECT * FROM routines WHERE deleted_at IS NULL AND archived_at IS NULL ORDER BY id DESC`,
			);
			return rows.map((row) => routineJson(row, ctx));
		})
		.add('GET', 'daily-routines/tasks/:id(\\d+)', async ({ ctx, params }) =>
			routineJson(await requireActive(ctx, Number(params.id)), ctx),
		)
		.add('PUT', 'daily-routines/tasks/:id(\\d+)', async ({ ctx, params, body }) => {
			const row = await requireActive(ctx, Number(params.id));
			return routineJson(await updateRoutine(ctx, row, body ?? {}), ctx);
		})
		.add(
			'DELETE',
			'daily-routines/tasks/:id(\\d+)',
			async ({ ctx, params }) => {
				await destroyRoutine(ctx, Number(params.id));
				return null;
			},
			204,
		)
		.add('POST', 'daily-routines/tasks/:id(\\d+)/complete', async ({ ctx, params }) => {
			const row = await requireActive(ctx, Number(params.id));
			return instanceJson(await completeForDate(ctx, row.id, localDate(ctx.now())));
		})
		.add('POST', 'daily-routines/tasks/:id(\\d+)/archive', async ({ ctx, params }) => {
			const row = await requireActive(ctx, Number(params.id));
			return routineJson(await archiveRoutine(ctx, row), ctx);
		})
		.add('POST', 'daily-routines/tasks/:id(\\d+)/complete-on', async ({ ctx, params, body, query }) => {
			const row = await requireActive(ctx, Number(params.id));
			const date = query.get('date') || body?.date || localDate(ctx.now());
			const instance = await completeForDate(ctx, row.id, date);
			return {
				instance_id: instance.id,
				task_id: row.id,
				date,
				status: instance.status,
				completed: instance.status === 'COMPLETED',
			};
		})
		.add('POST', 'daily-routines/tasks/:id(\\d+)/convert', async ({ ctx, params, body }) => {
			const row = await requireActive(ctx, Number(params.id));
			if (Number(body?.workspace_id) !== ctx.workspace.id) {
				throw new LocalHttpError(409, 'Local routines can only convert into the current local workspace');
			}
			const taskId = await convertRoutineToTask(ctx, row, body ?? {});
			const [taskRow] = await ctx.db.select<any>(`${TASK_SELECT} WHERE t.id = ?`, [taskId]);
			return taskJson(taskRow, ctx);
		})
		// ── instances ────────────────────────────────────────────────────────
		.add('GET', 'daily-routines/tasks/:id(\\d+)/instances', async ({ ctx, params }) => {
			const row = await requireActive(ctx, Number(params.id));
			const rows = await ctx.db.select<any>(
				`SELECT * FROM routine_instances WHERE routine_id = ? ORDER BY scheduled_for ASC`,
				[row.id],
			);
			return rows.map(instanceJson);
		})
		.add('POST', 'daily-routines/tasks/:id(\\d+)/instances/:instanceId(\\d+)/complete', async ({ ctx, params }) => {
			const row = await requireActive(ctx, Number(params.id));
			return instanceJson(await setInstanceStatus(ctx, row.id, Number(params.instanceId), 'COMPLETED'));
		})
		.add('POST', 'daily-routines/tasks/:id(\\d+)/instances/:instanceId(\\d+)/skip', async ({ ctx, params }) => {
			const row = await requireActive(ctx, Number(params.id));
			return instanceJson(await setInstanceStatus(ctx, row.id, Number(params.instanceId), 'SKIPPED'));
		})
		.add(
			'DELETE',
			'daily-routines/tasks/:id(\\d+)/instances/:instanceId(\\d+)',
			async ({ ctx, params }) => {
				const row = await requireActive(ctx, Number(params.id));
				await deleteInstance(ctx, row.id, Number(params.instanceId));
				return null;
			},
			204,
		)
		.add('PATCH', 'daily-routines/tasks/:id(\\d+)/instances/:instanceId', async ({ ctx, params, body }) => {
			const row = await requireActive(ctx, Number(params.id));
			const { date, time } = resolveReschedule(body ?? {});
			if (!date) throw new LocalHttpError(422, 'a target date is required');
			if (params.instanceId.toLowerCase() === 'virtual') {
				return instanceJson(await rescheduleVirtual(ctx, row.id, date, time));
			}
			const instanceId = Number(params.instanceId);
			if (!Number.isFinite(instanceId)) throw new LocalHttpError(400, 'Invalid task instance id');
			return instanceJson(await rescheduleInstance(ctx, row.id, instanceId, date, time));
		})
		.add('PUT', 'daily-routines/tasks/:id(\\d+)/pattern', async ({ ctx, params, body }) => {
			const row = await requireActive(ctx, Number(params.id));
			const pattern = normalizeRecurrence(body ?? {});
			if (!pattern) throw new LocalHttpError(422, 'a recurrence pattern is required');
			await savePattern(ctx, row.id, pattern);
			const [patternRow] = await ctx.db.select<any>(`SELECT * FROM routine_patterns WHERE routine_id = ?`, [row.id]);
			return patternJson(patternRow);
		})
		.add('GET', 'daily-routines/tasks/:id(\\d+)/stats', async ({ ctx, params }) => {
			const row = await requireActive(ctx, Number(params.id));
			const rows = await ctx.db.select<{ status: string }>(`SELECT status FROM routine_instances WHERE routine_id = ?`, [
				row.id,
			]);
			return {
				total: rows.length,
				completed: rows.filter((r) => r.status === 'COMPLETED').length,
				skipped: rows.filter((r) => r.status === 'SKIPPED').length,
			};
		})
		// ── calendar expansion ───────────────────────────────────────────────
		.add('GET', 'daily-routines/expand', async ({ ctx, query }) => {
			const from = query.get('from');
			const to = query.get('to');
			if (!from || !to) throw new LocalHttpError(422, 'from and to are required');
			return expandRange(ctx, from, to);
		})
		.add('GET', 'daily-routines/expand/stats', async ({ ctx, query }) => {
			const year = query.get('year');
			const today = localDate(ctx.now());
			const from = query.get('from') ?? (year ? `${year}-01-01` : `${today.slice(0, 4)}-01-01`);
			const to = query.get('to') ?? (year ? `${year}-12-31` : today);
			const entries = await expandRange(ctx, from, to);
			const byDate: Record<string, { fires: number; completed: number }> = {};
			for (const entry of entries) {
				const day = (byDate[entry.date] ??= { fires: 0, completed: 0 });
				day.fires++;
				if (entry.completed) day.completed++;
			}
			return byDate;
		});
