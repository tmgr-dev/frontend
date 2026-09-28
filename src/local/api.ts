import { LocalRouter } from './router';
import { generateUniqueCategoryCode, sanitizeCategoryCode } from './categoryCode';
import { addRoutineRoutes } from './routines/routes';
import { isRoutineId, updateRoutineTaskFields } from './routines/service';
import {
	categoryJson,
	paginate,
	parseJson,
	statusJson,
	TASK_SELECT,
	taskJson,
	toJson,
} from './serialize';
import { LocalHttpError, LocalRaw, type LocalActor, type LocalContext, type LocalRequest } from './types';

const iso = (ctx: LocalContext) => ctx.now().toISOString();
const epoch = (ctx: LocalContext) => Math.floor(ctx.now().getTime() / 1000);

const notFound = (what: string) => new LocalHttpError(404, `${what} not found`);

const actorOf = (ctx: LocalContext): LocalActor =>
	ctx.actor ?? { kind: 'user', id: String(ctx.user.id), name: ctx.user.name };

/**
 * Who owns a plugin's runs and reactions: its storage id when known (so a different repository that
 * reuses the same plugin id owns nothing of the original), else the plain actor id.
 */
const ownerIdOf = (actor: LocalActor): string => actor.ownerId ?? actor.id;

/** Relations from this task's side only, matching the legacy `relatedTypesWithTask` shape the UI reads. */
const taskRelationsFor = async (ctx: LocalContext, taskId: number) => {
	const rows = await ctx.db.select<any>(
		`SELECT r.id, rt.id AS type_id, rt.name AS type_name,
			t.id AS rt_id, t.title AS rt_title, t.status_id AS rt_status_id, t.project_category_id AS rt_category_id
		 FROM task_relations r
		 JOIN task_relation_types rt ON rt.id = r.relation_type_id
		 JOIN tasks t ON t.id = r.related_task_id
		 WHERE r.task_id = ? AND t.deleted_at IS NULL
		 ORDER BY r.id`,
		[taskId],
	);
	return rows.map((row) => ({
		id: row.id,
		relation_type: { id: row.type_id, name: row.type_name },
		related_task: {
			id: row.rt_id,
			title: row.rt_title,
			status_id: row.rt_status_id,
			workspace_id: ctx.workspace.id,
			project_category_id: row.rt_category_id,
		},
	}));
};

const loadTask = async (ctx: LocalContext, id: number) => {
	const rows = await ctx.db.select(`${TASK_SELECT} WHERE t.id = ? AND t.deleted_at IS NULL`, [id]);
	if (!rows.length) throw notFound('Task');
	return { ...taskJson(rows[0], ctx), relationTypeWithTask: await taskRelationsFor(ctx, id) };
};

const requireActiveTask = async (ctx: LocalContext, id: number) => {
	const [row] = await ctx.db.select<any>(
		`SELECT id, common_time, start_time FROM tasks WHERE id = ? AND deleted_at IS NULL`,
		[id],
	);
	if (!row) throw notFound('Task');
	return row;
};

const agentWorkDuration = (row: any, ctx: LocalContext) => {
	if (row.duration_seconds != null) return row.duration_seconds;
	if (row.status !== 'running') return 0;
	return Math.max(0, epoch(ctx) - Math.floor(new Date(row.started_at).getTime() / 1000));
};

/** One `personas` query for a whole list, keyed by uuid, instead of one per row. */
const personaNamesFor = async (ctx: LocalContext, rows: any[]): Promise<Map<string, string>> => {
	const uuids = [...new Set(rows.filter((row) => row.actor_kind === 'persona').map((row) => row.actor_id))];
	if (!uuids.length) return new Map();
	const placeholders = uuids.map(() => '?').join(',');
	const found = await ctx.db.select<{ uuid: string; name: string }>(
		`SELECT uuid, name FROM personas WHERE uuid IN (${placeholders})`,
		uuids,
	);
	return new Map(found.map((row) => [row.uuid, row.name]));
};

const agentWorkActor = (row: any, ctx: LocalContext, personaNames: Map<string, string>) => {
	if (row.actor_kind === 'persona') {
		return {
			kind: 'persona',
			id: row.actor_id,
			name: personaNames.get(row.actor_id),
			owner: { id: String(ctx.user.id), name: ctx.user.name },
		};
	}
	if (row.actor_kind === 'plugin') return { kind: 'plugin', id: row.actor_id };
	return { kind: 'user', id: row.actor_id, name: ctx.user.name };
};

const agentWorkJson = (row: any, ctx: LocalContext, personaNames: Map<string, string>) => ({
	id: row.id,
	task_id: row.task_id,
	workspace_id: ctx.workspace.id,
	user_id: ctx.user.id,
	agent: row.agent,
	model: row.model,
	session_id: row.session_id,
	branch: row.branch,
	status: row.status,
	started_at: row.started_at,
	ended_at: row.ended_at,
	duration_seconds: agentWorkDuration(row, ctx),
	summary: row.summary,
	pr_url: row.pr_url,
	commits: parseJson(row.commits, []),
	tests: parseJson(row.tests, null),
	version: row.version,
	actor: agentWorkActor(row, ctx, personaNames),
});

const normalizeAgent = (value: unknown): string => {
	const normalized = String(value ?? '').trim().toLowerCase().replace(/\s+/g, '-');
	if (!normalized) throw new LocalHttpError(422, 'agent is required');
	if (normalized.length > 64) throw new LocalHttpError(422, 'agent must be at most 64 characters');
	return normalized;
};

const limitedOrNull = (value: unknown, max: number): string | null => {
	if (value == null) return null;
	const trimmed = String(value).trim();
	if (!trimmed) return null;
	if (trimmed.length > max) throw new LocalHttpError(422, `must be at most ${max} characters`);
	return trimmed;
};

const httpUrlOrThrow = (value: unknown): string => {
	const trimmed = String(value ?? '').trim();
	if (trimmed.length <= 512) {
		try {
			const url = new URL(trimmed);
			if ((url.protocol === 'http:' || url.protocol === 'https:') && url.host) return trimmed;
		} catch {
			// falls through to the rejection below
		}
	}
	throw new LocalHttpError(422, 'pr_url must be an http(s) url');
};

const MAX_AGENT_WORK_COMMITS = 200;

const validCommits = (value: unknown): { sha: string; message: string | null }[] => {
	if (!Array.isArray(value) || value.length > MAX_AGENT_WORK_COMMITS) {
		throw new LocalHttpError(422, `at most ${MAX_AGENT_WORK_COMMITS} commits per run`);
	}
	return (value as any[])
		.filter((commit) => commit && typeof commit.sha === 'string' && commit.sha.trim())
		.map((commit) => ({
			sha: limitedOrNull(commit.sha, 64) ?? '',
			message: limitedOrNull(commit.message, 500),
		}));
};

const validTests = (value: unknown): { passed: number | null; failed: number | null; command: string | null } => {
	const v = (value ?? {}) as any;
	if ((v.passed != null && Number(v.passed) < 0) || (v.failed != null && Number(v.failed) < 0)) {
		throw new LocalHttpError(422, 'test counts cannot be negative');
	}
	return {
		passed: v.passed == null ? null : Number(v.passed),
		failed: v.failed == null ? null : Number(v.failed),
		command: limitedOrNull(v.command, 500),
	};
};

const AGENT_WORK_FINISHED = ['succeeded', 'failed', 'cancelled'];

const finishedStatus = (value: unknown): string => {
	const normalized = String(value ?? '').trim().toLowerCase();
	if (!AGENT_WORK_FINISHED.includes(normalized)) {
		throw new LocalHttpError(422, 'status must be one of succeeded, failed, cancelled');
	}
	return normalized;
};

const inPluginNamespace = (agent: string, pluginId: string): boolean =>
	agent === `plugin:${pluginId}` || agent.startsWith(`plugin:${pluginId}/`);

/**
 * Only the run's own starting actor may change it (403), and never after it is finished (409). A
 * plugin actor is checked twice, like the server: by actor identity and by the run's agent namespace.
 */
const requireOwnRunningRun = async (ctx: LocalContext, id: number) => {
	const [row] = await ctx.db.select<any>(`SELECT * FROM agent_work_runs WHERE id = ?`, [id]);
	if (!row) throw notFound('Agent work run');
	const actor = actorOf(ctx);
	const sameActor = row.actor_kind === actor.kind && String(row.actor_id) === ownerIdOf(actor);
	const sameNamespace = actor.kind !== 'plugin' || inPluginNamespace(row.agent, actor.id);
	if (!sameActor || !sameNamespace) {
		throw new LocalHttpError(403, 'Only the user the agent works for can change this run', 'NOT_OWN');
	}
	if (row.status !== 'running') throw new LocalHttpError(409, `Agent work run is already ${row.status}`);
	return row;
};

/** Column assignments for a progress update; the caller adds its own version/status/updated_at and runs one UPDATE. */
const progressAssignments = (body: any, includeBranch: boolean): { sets: string[]; values: any[] } => {
	const sets: string[] = [];
	const values: any[] = [];
	if (includeBranch && body?.branch != null) {
		sets.push('branch = ?');
		values.push(limitedOrNull(body.branch, 255));
	}
	if (body?.summary != null) {
		sets.push('summary = ?');
		values.push(limitedOrNull(body.summary, 10_000));
	}
	if (body?.pr_url != null) {
		sets.push('pr_url = ?');
		values.push(httpUrlOrThrow(body.pr_url));
	}
	if (body?.commits != null) {
		sets.push('commits = ?');
		values.push(toJson(validCommits(body.commits)));
	}
	if (body?.tests != null) {
		sets.push('tests = ?');
		values.push(toJson(validTests(body.tests)));
	}
	return { sets, values };
};

const loadStatuses = (ctx: LocalContext) =>
	ctx.db.select(
		`SELECT * FROM statuses ORDER BY CASE WHEN type = 'archived' THEN 1 ELSE 0 END, sort_order, id`,
	);

const statusFor = async (ctx: LocalContext, idOrType: string) => {
	const statuses = await loadStatuses(ctx);
	const type = idOrType === 'done' ? 'archived' : idOrType;
	const found = /^\d+$/.test(idOrType)
		? statuses.find((s: any) => s.id === Number(idOrType))
		: statuses.find((s: any) => s.type === type);
	if (!found) throw notFound('Status');
	return found as any;
};

const defaultStatusId = async (ctx: LocalContext) => {
	const statuses = await loadStatuses(ctx);
	const pick =
		statuses.find((s: any) => s.type === 'default') ??
		statuses.find((s: any) => s.type === 'active') ??
		statuses.find((s: any) => s.type !== 'archived') ??
		statuses[0];
	return (pick as any)?.id ?? null;
};

const numberOrNull = (value: any) =>
	value === null || value === undefined || value === '' ? null : Number(value);

const DATE_ONLY = /^\d{4}-\d{2}-\d{2}$/;

/** A bare date ('2026-10-01') is kept as the UI sends it; anything with a time is normalised to ISO UTC. */
const normalizeExpiredAt = (value: unknown): string | null => {
	if (value === null || value === undefined) return null;
	const trimmed = String(value).trim();
	if (!trimmed) return null;
	if (DATE_ONLY.test(trimmed)) return trimmed;
	const ms = Date.parse(trimmed);
	if (Number.isNaN(ms)) throw new LocalHttpError(422, 'expired_at must be an ISO date');
	return new Date(ms).toISOString().replace(/\.000Z$/, 'Z');
};

/** Fields a create/PUT/PATCH may set; everything else in the body (category, user, …) is derived. */
const writableTaskFields = (body: any) => {
	const fields: Record<string, any> = {};
	if ('title' in body) fields.title = String(body.title ?? '').trim();
	if ('description' in body) fields.description = body.description ?? null;
	if ('description_json' in body) fields.description_json = toJson(body.description_json);
	if ('status_id' in body) fields.status_id = numberOrNull(body.status_id);
	if ('project_category_id' in body) fields.project_category_id = numberOrNull(body.project_category_id);
	if ('priority' in body && body.priority) fields.priority = String(body.priority);
	if ('approximately_time' in body) fields.approximately_time = Number(body.approximately_time ?? 0) || 0;
	if ('checkpoints' in body) fields.checkpoints = toJson(body.checkpoints ?? []);
	if ('settings' in body && Array.isArray(body.settings)) fields.settings = toJson(body.settings);
	if ('expired_at' in body) fields.expired_at = normalizeExpiredAt(body.expired_at);
	if ('common_time' in body && body.common_time !== undefined)
		fields.common_time = Math.max(0, Number(body.common_time) || 0);
	return fields;
};

// Personas have no timer permission: common_time is timer bookkeeping, not a task field.
const personaWritableFields = (ctx: LocalContext, body: any) => {
	const fields = writableTaskFields(body ?? {});
	if (ctx.actor?.kind === 'persona') delete fields.common_time;
	return fields;
};

const updateTask = async (ctx: LocalContext, id: number, fields: Record<string, any>) => {
	if ('status_id' in fields && fields.status_id != null) {
		const [status] = await ctx.db.select<any>(`SELECT type FROM statuses WHERE id = ?`, [
			fields.status_id,
		]);
		if (status?.type === 'archived') await stopTimer(ctx, id);
	}
	const keys = Object.keys(fields);
	const sets = [...keys.map((k) => `${k} = ?`), 'updated_at = ?'];
	const values: any[] = [...keys.map((k) => fields[k]), iso(ctx)];
	if ('project_category_id' in fields) {
		// Moving to another category takes the next ticket number there, like the Java API.
		sets.push(`category_tasks_sequence_id = CASE
			WHEN project_category_id IS ? THEN category_tasks_sequence_id
			WHEN ? IS NULL THEN NULL
			ELSE (SELECT COALESCE(MAX(category_tasks_sequence_id), 0) + 1 FROM tasks WHERE project_category_id = ?)
		END`);
		values.push(fields.project_category_id, fields.project_category_id, fields.project_category_id);
	}
	const result = await ctx.db.execute(
		`UPDATE tasks SET ${sets.join(', ')} WHERE id = ? AND deleted_at IS NULL`,
		[...values, id],
	);
	if (!result.rowsAffected) throw notFound('Task');
	return loadTask(ctx, id);
};

const stopTimer = async (ctx: LocalContext, id: number) => {
	const now = epoch(ctx);
	await ctx.db.execute(
		`UPDATE tasks SET common_time = common_time + MAX(0, ? - start_time), start_time = 0, end_time = ?,
			updated_at = ?
		 WHERE id = ? AND start_time > 0`,
		[now, now, iso(ctx), id],
	);
};

const searchClause = (req: LocalRequest, params: any[]) => {
	const clauses: string[] = [];
	const search = req.query.get('search')?.trim().toLowerCase();
	if (search) {
		clauses.push(`(LOWER(t.title) LIKE ? OR LOWER(COALESCE(t.description, '')) LIKE ?)`);
		params.push(`%${search}%`, `%${search}%`);
	}
	const category = req.query.get('project_category_id');
	if (category) {
		clauses.push(`t.project_category_id = ?`);
		params.push(Number(category));
	}
	const status = req.query.get('status_id');
	if (status) {
		clauses.push(`t.status_id = ?`);
		params.push(Number(status));
	}
	const statusType = req.query.get('status_type');
	if (statusType) {
		clauses.push(`s.type = ?`);
		params.push(statusType);
	}
	const priority = req.query.get('priority');
	if (priority) {
		clauses.push(`t.priority = ?`);
		params.push(priority);
	}
	const updatedSince = req.query.get('updated_since');
	if (updatedSince) {
		clauses.push(`julianday(t.updated_at) >= julianday(?)`);
		params.push(updatedSince);
	}
	const dueBefore = req.query.get('due_before');
	if (dueBefore) {
		clauses.push(`julianday(t.expired_at) < julianday(?)`);
		params.push(dueBefore);
	}
	const dueAfter = req.query.get('due_after');
	if (dueAfter) {
		clauses.push(`julianday(t.expired_at) > julianday(?)`);
		params.push(dueAfter);
	}
	return clauses;
};

const taskSortOrder = (req: LocalRequest): string => {
	const sort = req.query.get('sort');
	const direction = req.query.get('direction') === 'desc' ? 'DESC' : 'ASC';
	if (sort === 'due') return `t.expired_at IS NULL, julianday(t.expired_at) ${direction}, t.id DESC`;
	if (sort === 'updated') return `t.updated_at ${direction}, t.id DESC`;
	if (sort === 'created') return `t.created_at ${direction}, t.id DESC`;
	return 't.id DESC';
};

const listTasks = async (
	req: LocalRequest,
	where: string[],
	params: any[],
	orderBy: string,
	path: string,
) => {
	const { ctx } = req;
	const clauses = ['t.deleted_at IS NULL', ...where, ...searchClause(req, params)];
	const whereSql = `WHERE ${clauses.join(' AND ')}`;
	const all = req.query.has('all');
	const perPage = all ? 0 : Math.max(1, Number(req.query.get('per_page') ?? 20) || 20);
	const page = all ? 1 : Math.max(1, Number(req.query.get('page') ?? 1) || 1);
	const [{ n, seconds }] = await ctx.db.select<{ n: number; seconds: number }>(
		`SELECT COUNT(*) AS n, COALESCE(SUM(t.common_time), 0) AS seconds FROM tasks t
		 LEFT JOIN statuses s ON s.id = t.status_id ${whereSql}`,
		params,
	);
	const rows = await ctx.db.select(
		`${TASK_SELECT} ${whereSql} ORDER BY ${orderBy}${all ? '' : ' LIMIT ? OFFSET ?'}`,
		all ? params : [...params, perPage, (page - 1) * perPage],
	);
	const total = Number(n);
	return paginate(
		rows.map((row) => taskJson(row, ctx)),
		total,
		page,
		all ? Math.max(1, total) : perPage,
		`/api/${path}`,
		{ total_seconds: String(seconds ?? 0), total_overtime_seconds: '0' },
	);
};

const boardOrder = (req: LocalRequest) => {
	const column = req.query.get('order[column]');
	const direction = req.query.get('order[direction]') === 'asc' ? 'ASC' : 'DESC';
	if (column === 'order') {
		return direction === 'ASC'
			? 't.sort_order IS NOT NULL, t.sort_order ASC, t.id DESC'
			: 't.sort_order IS NULL, t.sort_order DESC, t.id DESC';
	}
	const columns: Record<string, string> = {
		created_at: 't.created_at',
		updated_at: 't.updated_at',
		expired_at: 't.expired_at',
		end_time: 't.end_time',
	};
	return `${columns[column ?? ''] ?? 't.updated_at'} ${direction}, t.id DESC`;
};

const categoryCounts = `
	SELECT c.*,
		(SELECT COUNT(*) FROM categories k WHERE k.parent_id = c.id AND k.deleted_at IS NULL) AS children_count,
		(SELECT COUNT(*) FROM tasks t WHERE t.project_category_id = c.id AND t.deleted_at IS NULL) AS tasks_count
	FROM categories c`;

const requireUniqueCategoryCode = async (ctx: LocalContext, code: string, exceptId?: number) => {
	const [{ n }] = await ctx.db.select<{ n: number }>(
		`SELECT COUNT(*) AS n FROM categories WHERE deleted_at IS NULL AND UPPER(code) = UPPER(?)${
			exceptId ? ' AND id <> ?' : ''
		}`,
		exceptId ? [code, exceptId] : [code],
	);
	if (Number(n)) throw new LocalHttpError(422, `code ${code} is already used`);
};

const PLUGIN_STORAGE_QUOTA = 5 * 1024 * 1024;
const PLUGIN_STORAGE_KEYS = 1000;
const PLUGIN_ROW_OVERHEAD = 64;
const byteLength = (value: string): number => new TextEncoder().encode(value).byteLength;

const FEATURE_TOGGLES: Record<string, boolean> = {
	board: true,
	categories: true,
	dashboard: false,
	daily_routines: true,
	'task.countdown': true,
	'task.checkpoints': true,
	'task.assignees': false,
	'task.files': true,
	'task.relations': true,
};

export const createLocalApi = () => {
	const router = new LocalRouter()
		// ── tasks ───────────────────────────────────────────────────────────────
		.add('GET', 'tasks/current', (req) =>
			listTasks(req, [`(s.id IS NULL OR s.type <> 'archived')`], [], 't.id DESC', 'tasks/current'),
		)
		.add('GET', 'tasks/runned', async ({ ctx }) => {
			const rows = await ctx.db.select(
				`${TASK_SELECT} WHERE t.deleted_at IS NULL AND t.start_time > 0 ORDER BY t.start_time DESC`,
			);
			return paginate(rows.map((row) => taskJson(row, ctx)), rows.length, 1, Math.max(1, rows.length), '/api/tasks/runned');
		})
		.add('GET', 'tasks/indexes', async ({ ctx, query }) => {
			const [{ n }] = await ctx.db.select<{ n: number }>(
				`SELECT COUNT(*) AS n FROM tasks WHERE deleted_at IS NULL`,
			);
			const category = query.get('category');
			const [{ c }] = category
				? await ctx.db.select<{ c: number }>(
						`SELECT COUNT(*) AS c FROM tasks WHERE deleted_at IS NULL AND project_category_id = ?`,
						[Number(category)],
				  )
				: [{ c: 0 }];
			return { index: { workspace: Number(n) + 1, category: Number(c) + 1 } };
		})
		.add('GET', 'tasks/status/:status', async (req) => {
			const status = await statusFor(req.ctx, req.params.status);
			return listTasks(req, ['t.status_id = ?'], [status.id], boardOrder(req), `tasks/status/${req.params.status}`);
		})
		.add('PUT', 'tasks/update-orders', async ({ ctx, body }) => {
			for (const item of body?.tasks ?? []) {
				await ctx.db.execute(`UPDATE tasks SET sort_order = ?, updated_at = ? WHERE id = ?`, [
					Number(item.order),
					iso(ctx),
					Number(item.id),
				]);
			}
			return [];
		})
		.add('POST', 'tasks', async ({ ctx, body }) => {
			const fields = personaWritableFields(ctx, body);
			if (!fields.title) throw new LocalHttpError(422, 'title is required');
			fields.status_id = fields.status_id ?? (await defaultStatusId(ctx));
			const now = iso(ctx);
			const keys = Object.keys(fields);
			// One statement, so the per-category ticket number is atomic with the insert.
			const result = await ctx.db.execute(
				`INSERT INTO tasks (${keys.join(', ')}, category_tasks_sequence_id, created_at, updated_at)
				 SELECT ${keys.map(() => '?').join(', ')},
					CASE WHEN ? IS NULL THEN NULL ELSE
						(SELECT COALESCE(MAX(category_tasks_sequence_id), 0) + 1 FROM tasks WHERE project_category_id = ?)
					END, ?, ?`,
				[...keys.map((k) => fields[k]), fields.project_category_id ?? null, fields.project_category_id ?? null, now, now],
			);
			return loadTask(ctx, Number(result.lastInsertId));
		}, 201)
		.add('GET', 'tasks', (req) => listTasks(req, [], [], taskSortOrder(req), 'tasks'))
		.add('GET', 'tasks/settings', () => [])
		.add('GET', 'tasks/:id(\\d+)', ({ ctx, params }) => loadTask(ctx, Number(params.id)))
		.add('PUT', 'tasks/:id(\\d+)', ({ ctx, params, body }) => {
			const id = Number(params.id);
			return isRoutineId(id)
				? updateRoutineTaskFields(ctx, id, body ?? {})
				: updateTask(ctx, id, writableTaskFields(body ?? {}));
		})
		.add('PATCH', 'tasks/:id(\\d+)', ({ ctx, params, body }) => {
			const id = Number(params.id);
			return isRoutineId(id)
				? updateRoutineTaskFields(ctx, id, body ?? {})
				: updateTask(ctx, id, personaWritableFields(ctx, body));
		})
		.add('DELETE', 'tasks/:id(\\d+)', async ({ ctx, params }) => {
			const id = Number(params.id);
			await stopTimer(ctx, id);
			const result = await ctx.db.execute(
				`UPDATE tasks SET deleted_at = ?, updated_at = ? WHERE id = ? AND deleted_at IS NULL`,
				[iso(ctx), iso(ctx), id],
			);
			if (!result.rowsAffected) throw notFound('Task');
			// Soft-deleted, so the ON DELETE CASCADE on task_id never fires: clean these up explicitly.
			await ctx.db.execute(`DELETE FROM plugin_task_data WHERE task_id = ?`, [id]);
			await ctx.db.execute(`DELETE FROM agent_work_runs WHERE task_id = ?`, [id]);
			await ctx.db.execute(`DELETE FROM task_relations WHERE task_id = ? OR related_task_id = ?`, [id, id]);
			await ctx.db.execute(
				`DELETE FROM comment_reactions WHERE comment_id IN (SELECT id FROM comments WHERE task_id = ?)`,
				[id],
			);
			return { success: true };
		})
		.add('POST', 'tasks/:id(\\d+)/countdown', async ({ ctx, params }) => {
			const id = Number(params.id);
			await ctx.db.execute(
				`UPDATE tasks SET start_time = ?, updated_at = ? WHERE id = ? AND deleted_at IS NULL AND start_time = 0`,
				[epoch(ctx), iso(ctx), id],
			);
			return loadTask(ctx, id);
		})
		.add('DELETE', 'tasks/:id(\\d+)/countdown', async ({ ctx, params }) => {
			const id = Number(params.id);
			await stopTimer(ctx, id);
			return loadTask(ctx, id);
		})
		.add('PUT', 'tasks/:id(\\d+)/settings', ({ ctx, params }) => loadTask(ctx, Number(params.id)))
		.add('PUT', 'tasks/:id(\\d+)/time', ({ ctx, params, body }) =>
			updateTask(ctx, Number(params.id), { common_time: Math.max(0, Number(body?.common_time) || 0) }),
		)
		.add('PUT', 'tasks/:id(\\d+)/:status', async ({ ctx, params }) => {
			const status = await statusFor(ctx, params.status);
			const id = Number(params.id);
			if (status.type === 'archived') await stopTimer(ctx, id);
			return updateTask(ctx, id, { status_id: status.id });
		})
		// ── comments ────────────────────────────────────────────────────────────
		.add('GET', 'tasks/:id(\\d+)/comments', async ({ ctx, params }) => {
			await loadTask(ctx, Number(params.id));
			const rows = await ctx.db.select<any>(
				`SELECT * FROM comments WHERE task_id = ? AND deleted_at IS NULL ORDER BY id ASC`,
				[Number(params.id)],
			);
			const reactions = await reactionsFor(ctx, rows.map((row) => row.id));
			return rows.map((row) => ({ ...commentJson(row, ctx), reactions: reactions.get(row.id) ?? [] }));
		})
		.add('POST', 'tasks/:id(\\d+)/comments', async ({ ctx, params, body }) => {
			const message = String(body?.message ?? body?.content ?? '').trim();
			if (!message) throw new LocalHttpError(422, 'message is required');
			await loadTask(ctx, Number(params.id));
			const actor = actorOf(ctx);
			const now = iso(ctx);
			const result = await ctx.db.execute(
				`INSERT INTO comments (task_id, message, author_kind, author_id, author_name, created_at, updated_at)
				 VALUES (?, ?, ?, ?, ?, ?, ?)`,
				[Number(params.id), message, actor.kind, actor.id, actor.name, now, now],
			);
			const [row] = await ctx.db.select(`SELECT * FROM comments WHERE id = ?`, [Number(result.lastInsertId)]);
			return { ...commentJson(row, ctx), reactions: [] };
		}, 201)
		.add('PUT', 'comments/:id(\\d+)', async ({ ctx, params, body }) => {
			const id = Number(params.id);
			const [comment] = await ctx.db.select<any>(`SELECT * FROM comments WHERE id = ? AND deleted_at IS NULL`, [id]);
			if (!comment) throw notFound('Comment');
			if (ctx.actor?.kind === 'persona' && (comment.author_kind !== 'persona' || String(comment.author_id) !== ctx.actor.id)) {
				throw new LocalHttpError(403, 'A persona may only edit its own comments', 'NOT_OWN');
			}
			const message = String(body?.message ?? body?.content ?? '').trim();
			if (!message) throw new LocalHttpError(422, 'message is required');
			const now = iso(ctx);
			await ctx.db.execute(`UPDATE comments SET message = ?, updated_at = ? WHERE id = ?`, [message, now, id]);
			const [row] = await ctx.db.select(`SELECT * FROM comments WHERE id = ?`, [id]);
			const reactions = (await reactionsFor(ctx, [id])).get(id) ?? [];
			return { ...commentJson(row, ctx), reactions };
		})
		.add('DELETE', 'comments/:id(\\d+)', async ({ ctx, params }) => {
			const [comment] = await ctx.db.select<any>(`SELECT task_id, author_kind, author_id FROM comments WHERE id = ?`, [
				Number(params.id),
			]);
			if (ctx.actor?.kind === 'persona') {
				if (!comment) throw notFound('Comment');
				if (comment.author_kind !== 'persona' || String(comment.author_id) !== ctx.actor.id) {
					throw new LocalHttpError(403, 'A persona may only delete its own comments', 'NOT_OWN');
				}
			}
			await ctx.db.execute(`UPDATE comments SET deleted_at = ? WHERE id = ?`, [iso(ctx), Number(params.id)]);
			return { success: true, task_id: comment?.task_id };
		})
		.add('POST', 'comments/:id(\\d+)/reactions/toggle', async ({ ctx, params, body }) => {
			const emoji = String(body?.emoji ?? '');
			if (!emoji.trim() || emoji.length > 32) throw new LocalHttpError(422, 'emoji is required');
			const [comment] = await ctx.db.select<any>(
				`SELECT c.* FROM comments c JOIN tasks t ON t.id = c.task_id
				 WHERE c.id = ? AND c.deleted_at IS NULL AND t.deleted_at IS NULL`,
				[Number(params.id)],
			);
			if (!comment) throw notFound('Comment');
			const actor = actorOf(ctx);
			const ownerId = ownerIdOf(actor);
			const existing = await ctx.db.select<{ id: number }>(
				`SELECT id FROM comment_reactions WHERE comment_id = ? AND emoji = ? AND actor_kind = ? AND actor_id = ?`,
				[comment.id, emoji, actor.kind, ownerId],
			);
			if (existing.length) {
				await ctx.db.execute(`DELETE FROM comment_reactions WHERE id = ?`, [existing[0].id]);
			} else {
				await ctx.db.execute(
					`INSERT INTO comment_reactions (comment_id, emoji, actor_kind, actor_id, created_at) VALUES (?, ?, ?, ?, ?)`,
					[comment.id, emoji, actor.kind, ownerId, iso(ctx)],
				);
			}
			const reactions = (await reactionsFor(ctx, [comment.id])).get(comment.id) ?? [];
			return { reactions, task_id: comment.task_id };
		})
		// ── task relations ──────────────────────────────────────────────────────
		.add('GET', 'task-relation-types', async ({ ctx }) =>
			(await ctx.db.select<any>(`SELECT * FROM task_relation_types ORDER BY id`)).map((row) => ({
				id: row.id,
				name: row.name,
			})),
		)
		.add('GET', 'tasks/:id(\\d+)/relations', async ({ ctx, params }) => {
			await loadTask(ctx, Number(params.id));
			return taskRelationsFor(ctx, Number(params.id));
		})
		.add(
			'POST',
			'tasks/:id(\\d+)/related-to/:otherId(\\d+)/with/:typeId(\\d+)',
			async ({ ctx, params }) => {
				const taskId = Number(params.id);
				const otherId = Number(params.otherId);
				const typeId = Number(params.typeId);
				if (taskId === otherId) throw new LocalHttpError(422, 'A task cannot relate to itself');
				await loadTask(ctx, taskId);
				await loadTask(ctx, otherId);
				const [type] = await ctx.db.select<any>(`SELECT * FROM task_relation_types WHERE id = ?`, [typeId]);
				if (!type) throw notFound('Relation type');
				await ctx.db.execute(
					`INSERT INTO task_relations (task_id, related_task_id, relation_type_id, created_at)
					 VALUES (?, ?, ?, ?)
					 ON CONFLICT (task_id, related_task_id, relation_type_id) DO NOTHING`,
					[taskId, otherId, typeId, iso(ctx)],
				);
				const [row] = await ctx.db.select<any>(
					`SELECT id FROM task_relations WHERE task_id = ? AND related_task_id = ? AND relation_type_id = ?`,
					[taskId, otherId, typeId],
				);
				return {
					id: row.id,
					task_id: taskId,
					related_task_id: otherId,
					task_relation_type_id: typeId,
					relation_type: { id: type.id, name: type.name },
				};
			},
		)
		.add(
			'DELETE',
			'tasks/:id(\\d+)/related-to/:otherId(\\d+)/with/:typeId(\\d+)',
			async ({ ctx, params }) => {
				const taskId = Number(params.id);
				const otherId = Number(params.otherId);
				const typeId = Number(params.typeId);
				await loadTask(ctx, taskId);
				await loadTask(ctx, otherId);
				const [type] = await ctx.db.select<any>(`SELECT * FROM task_relation_types WHERE id = ?`, [typeId]);
				await ctx.db.execute(
					`DELETE FROM task_relations WHERE task_id = ? AND related_task_id = ? AND relation_type_id = ?`,
					[taskId, otherId, typeId],
				);
				return {
					success: true,
					task_id: taskId,
					related_task_id: otherId,
					task_relation_type_id: typeId,
					relation_type: type ? { id: type.id, name: type.name } : null,
				};
			},
		)
		// ── attachments ─────────────────────────────────────────────────────────
		.add('POST', 'files/presign-upload', ({ ctx, body }) => {
			const size = Number(body?.size_bytes ?? 0);
			if (size > MAX_FILE_BYTES) throw new LocalHttpError(413, 'Files up to 25 MB');
			const key = fileKey(String(body?.file_name ?? 'file'), crypto.randomUUID());
			return {
				key,
				upload_url: ctx.files.url(key),
				method: 'PUT',
				content_type: body?.content_type || 'application/octet-stream',
				max_bytes: MAX_FILE_BYTES,
			};
		})
		.add('GET', 'tasks/:id(\\d+)/files', async ({ ctx, params }) => {
			await loadTask(ctx, Number(params.id));
			const rows = await ctx.db.select(`SELECT * FROM files WHERE task_id = ? ORDER BY id DESC`, [Number(params.id)]);
			return rows.map((row) => fileJson(row, ctx));
		})
		.add('POST', 'tasks/:id(\\d+)/files', async ({ ctx, params, body }) => {
			await loadTask(ctx, Number(params.id));
			const key = String(body?.file_path ?? '');
			if (!/^[A-Za-z0-9-]+\/[A-Za-z0-9._-]+$/.test(key)) throw new LocalHttpError(422, 'Unknown file');
			const result = await ctx.db.execute(
				`INSERT INTO files (task_id, name, file_path, mime_type, size, created_at) VALUES (?, ?, ?, ?, ?, ?)`,
				[
					Number(params.id),
					String(body?.file_name ?? key.split('/')[1]),
					key,
					body?.mime_type ?? null,
					Number(body?.size_bytes ?? 0),
					iso(ctx),
				],
			);
			return fileJson(await loadFile(ctx, Number(result.lastInsertId)), ctx);
		}, 201)
		.add('GET', 'files/:id(\\d+)', async ({ ctx, params }) => fileJson(await loadFile(ctx, Number(params.id)), ctx))
		.add('GET', 'files/:id(\\d+)/signed-url', async ({ ctx, params }) => {
			const file = await loadFile(ctx, Number(params.id));
			return { url: ctx.files.url(file.file_path), expires_at: '9999-12-31T23:59:59Z' };
		})
		.add('GET', 'files/:id(\\d+)/content', async ({ ctx, params }) => {
			const file = await loadFile(ctx, Number(params.id));
			return new LocalRaw(await ctx.files.read(file.file_path));
		})
		.add('DELETE', 'files/:id(\\d+)', async ({ ctx, params }) => {
			const file = await loadFile(ctx, Number(params.id));
			await ctx.db.execute(`DELETE FROM files WHERE id = ?`, [file.id]);
			await ctx.files.remove(file.file_path);
			return { success: true };
		})
		.add('GET', 'workspaces/:wid/files', async ({ ctx, query }) => {
			const perPage = Math.max(1, Number(query.get('per_page') ?? 40) || 40);
			const page = Math.max(1, Number(query.get('page') ?? 1) || 1);
			const images = query.get('images') === 'true';
			const where = `WHERE t.deleted_at IS NULL${images ? ` AND f.mime_type LIKE 'image/%'` : ''}`;
			const [{ n }] = await ctx.db.select<{ n: number }>(
				`SELECT COUNT(*) AS n FROM files f JOIN tasks t ON t.id = f.task_id ${where}`,
			);
			const rows = await ctx.db.select<any>(
				`SELECT f.*, t.title AS task_title, t.category_tasks_sequence_id AS seq, c.code AS c_code
				 FROM files f JOIN tasks t ON t.id = f.task_id LEFT JOIN categories c ON c.id = t.project_category_id
				 ${where} ORDER BY f.id DESC LIMIT ? OFFSET ?`,
				[perPage, (page - 1) * perPage],
			);
			const total = Number(n);
			return {
				data: rows.map((row) => ({
					id: row.id,
					name: row.name,
					mime_type: row.mime_type,
					size: row.size,
					user_id: ctx.user.id,
					created_at: row.created_at,
					task: {
						id: row.task_id,
						key: row.c_code && row.seq ? `${row.c_code}-${row.seq}` : null,
						title: row.task_title,
					},
				})),
				meta: { current_page: page, per_page: perPage, total, last_page: Math.max(1, Math.ceil(total / perPage)) },
			};
		})
		// ── statuses ────────────────────────────────────────────────────────────
		.add('GET', 'workspaces/statuses', async ({ ctx }) => (await loadStatuses(ctx)).map(statusJson))
		.add('GET', 'workspaces/:wid/statuses', async ({ ctx }) => (await loadStatuses(ctx)).map(statusJson))
		.add('POST', 'workspaces/:wid/statuses', async ({ ctx, body }) => {
			const name = String(body?.name ?? '').trim();
			if (!name) throw new LocalHttpError(422, 'name is required');
			const now = iso(ctx);
			const result = await ctx.db.execute(
				`INSERT INTO statuses (name, type, color, sort_order, created_at, updated_at)
				 SELECT ?, ?, ?, COALESCE(MAX(sort_order), 0) + 1, ?, ? FROM statuses`,
				[name, body?.type || 'active', body?.color ?? null, now, now],
			);
			const [row] = await ctx.db.select(`SELECT * FROM statuses WHERE id = ?`, [Number(result.lastInsertId)]);
			return statusJson(row);
		}, 201)
		.add('PUT', 'workspaces/:wid/statuses/order', async ({ ctx, body }) => {
			for (const item of body?.statuses_with_order ?? []) {
				await ctx.db.execute(`UPDATE statuses SET sort_order = ?, updated_at = ? WHERE id = ?`, [
					Number(item.order),
					iso(ctx),
					Number(item.status_id),
				]);
			}
			return (await loadStatuses(ctx)).map(statusJson);
		})
		.add('GET', 'statuses/:id(\\d+)', async ({ ctx, params }) => statusJson(await statusFor(ctx, params.id)))
		.add('PUT', 'statuses/:id(\\d+)', async ({ ctx, params, body }) => {
			const status = await statusFor(ctx, params.id);
			await ctx.db.execute(`UPDATE statuses SET name = ?, type = ?, color = ?, updated_at = ? WHERE id = ?`, [
				body?.name ?? status.name,
				body?.type ?? status.type,
				body?.color ?? status.color,
				iso(ctx),
				status.id,
			]);
			return statusJson(await statusFor(ctx, params.id));
		})
		.add('DELETE', 'statuses/:id(\\d+)', async ({ ctx, params }) => {
			const status = await statusFor(ctx, params.id);
			const [{ n }] = await ctx.db.select<{ n: number }>(
				`SELECT COUNT(*) AS n FROM tasks WHERE status_id = ? AND deleted_at IS NULL`,
				[status.id],
			);
			if (Number(n)) throw new LocalHttpError(409, 'Move the tasks out of this status first');
			await ctx.db.execute(`DELETE FROM statuses WHERE id = ?`, [status.id]);
			return { success: true };
		})
		.add('PUT', 'statuses/:id(\\d+)/tasks', async ({ ctx, params, body }) => {
			const status = await statusFor(ctx, params.id);
			for (const taskId of body?.task_ids ?? []) {
				if (status.type === 'archived') await stopTimer(ctx, Number(taskId));
				await ctx.db.execute(`UPDATE tasks SET status_id = ?, updated_at = ? WHERE id = ?`, [
					status.id,
					iso(ctx),
					Number(taskId),
				]);
			}
			return { success: true };
		})
		// ── categories ──────────────────────────────────────────────────────────
		.add('GET', 'project_categories', async ({ ctx }) => {
			const rows = await ctx.db.select(`${categoryCounts} WHERE c.deleted_at IS NULL ORDER BY c.title COLLATE NOCASE`);
			return rows.map((row) => categoryJson(row, ctx));
		})
		.add('GET', 'project_categories/children', async ({ ctx }) => {
			const rows = await ctx.db.select(
				`${categoryCounts} WHERE c.deleted_at IS NULL AND c.parent_id IS NULL ORDER BY c.updated_at DESC`,
			);
			return rows.map((row) => categoryJson(row, ctx));
		})
		.add('GET', 'project_categories/children/:id(\\d+)', async ({ ctx, params, query }) => {
			const rows = await ctx.db.select(
				`${categoryCounts} WHERE c.deleted_at IS NULL AND c.parent_id = ? ORDER BY c.updated_at DESC`,
				[Number(params.id)],
			);
			const perPage = Math.max(1, Number(query.get('per_page') ?? 20) || 20);
			const page = Math.max(1, Number(query.get('page') ?? 1) || 1);
			return paginate(
				rows.slice((page - 1) * perPage, page * perPage).map((row) => categoryJson(row, ctx)),
				rows.length,
				page,
				perPage,
				`/api/project_categories/children/${params.id}`,
			);
		})
		.add('GET', 'project_categories/:id(\\d+)/with/parents', async ({ ctx, params }) => {
			const load = async (id: number, depth: number): Promise<any> => {
				const [row] = await ctx.db.select<any>(`${categoryCounts} WHERE c.id = ? AND c.deleted_at IS NULL`, [id]);
				if (!row) return null;
				const json = categoryJson(row, ctx);
				json.parent_category = row.parent_id && depth < 32 ? await load(row.parent_id, depth + 1) : null;
				return json;
			};
			const category = await load(Number(params.id), 0);
			if (!category) throw notFound('Category');
			return category;
		})
		.add('GET', 'project_categories/:id(\\d+)', async ({ ctx, params }) => {
			const rows = await ctx.db.select(`${categoryCounts} WHERE c.id = ? AND c.deleted_at IS NULL`, [Number(params.id)]);
			if (!rows.length) throw notFound('Category');
			return categoryJson(rows[0], ctx);
		})
		.add('POST', 'project_categories', async ({ ctx, body }) => {
			const title = String(body?.title ?? '').trim();
			if (!title) throw new LocalHttpError(422, 'title is required');
			let code = body?.code ? String(body.code).toUpperCase() : null;
			if (code) {
				await requireUniqueCategoryCode(ctx, code);
			} else {
				const existing = await ctx.db.select<{ code: string | null }>(
					`SELECT code FROM categories WHERE deleted_at IS NULL AND code IS NOT NULL`,
				);
				code = generateUniqueCategoryCode(
					sanitizeCategoryCode(title),
					existing.map((row) => row.code as string),
				);
			}
			const now = iso(ctx);
			const result = await ctx.db.execute(
				`INSERT INTO categories (title, slug, code, parent_id, settings, created_at, updated_at)
				 VALUES (?, ?, ?, ?, ?, ?, ?)`,
				[
					title,
					body?.slug ?? slugOf(title),
					code,
					numberOrNull(body?.project_category_id),
					toJson(Array.isArray(body?.settings) ? body.settings : []),
					now,
					now,
				],
			);
			const rows = await ctx.db.select(`${categoryCounts} WHERE c.id = ?`, [Number(result.lastInsertId)]);
			return categoryJson(rows[0], ctx);
		}, 201)
		.add('PUT', 'project_categories/:id(\\d+)', async ({ ctx, params, body }) => {
			const id = Number(params.id);
			const [row] = await ctx.db.select<any>(`SELECT * FROM categories WHERE id = ? AND deleted_at IS NULL`, [id]);
			if (!row) throw notFound('Category');
			const nextCode = 'code' in (body ?? {}) ? (body.code ? String(body.code).toUpperCase() : null) : row.code;
			if (nextCode && nextCode.toUpperCase() !== String(row.code ?? '').toUpperCase()) {
				await requireUniqueCategoryCode(ctx, nextCode, id);
			}
			await ctx.db.execute(
				`UPDATE categories SET title = ?, code = ?, parent_id = ?, settings = ?, updated_at = ? WHERE id = ?`,
				[
					body?.title ?? row.title,
					nextCode,
					'project_category_id' in (body ?? {}) ? numberOrNull(body.project_category_id) : row.parent_id,
					Array.isArray(body?.settings) ? toJson(body.settings) : row.settings,
					iso(ctx),
					id,
				],
			);
			const rows = await ctx.db.select(`${categoryCounts} WHERE c.id = ?`, [id]);
			return categoryJson(rows[0], ctx);
		})
		.add('DELETE', 'project_categories/:id(\\d+)', async ({ ctx, params }) => {
			await ctx.db.execute(`UPDATE categories SET deleted_at = ? WHERE id = ?`, [iso(ctx), Number(params.id)]);
			return { success: true };
		})
		// ── plugin storage ──────────────────────────────────────────────────────
		.add('GET', 'plugins/:pid/storage', async ({ ctx, params }) =>
			(
				await ctx.db.select<{ key: string }>(`SELECT key FROM plugin_kv WHERE plugin_id = ? ORDER BY key`, [
					params.pid,
				])
			).map((row) => row.key),
		)
		.add('GET', 'plugins/:pid/storage/:key', async ({ ctx, params }) => {
			const [row] = await ctx.db.select<{ value: string }>(
				`SELECT value FROM plugin_kv WHERE plugin_id = ? AND key = ?`,
				[params.pid, params.key],
			);
			return { value: row?.value ?? null };
		})
		.add('PUT', 'plugins/:pid/storage/:key', async ({ ctx, params, body }) => {
			const value = typeof body?.value === 'string' ? body.value : null;
			if (value === null) throw new LocalHttpError(422, 'value must be a JSON string');
			// One statement, so concurrent writes cannot all pass the check before any of them lands.
			const { rowsAffected } = await ctx.db.execute(
				`INSERT INTO plugin_kv (plugin_id, key, value, updated_at)
				 SELECT ?, ?, ?, ?
				 WHERE ((SELECT COALESCE(SUM(LENGTH(CAST(key AS BLOB)) + LENGTH(CAST(value AS BLOB)) + ${PLUGIN_ROW_OVERHEAD}), 0)
				         FROM plugin_kv WHERE plugin_id = ? AND key <> ?)
				        + (SELECT COALESCE(SUM(LENGTH(CAST(key AS BLOB)) + LENGTH(CAST(value AS BLOB)) + ${PLUGIN_ROW_OVERHEAD}), 0)
				           FROM plugin_task_data WHERE plugin_id = ?)
				        + ? <= ${PLUGIN_STORAGE_QUOTA})
				   AND ((SELECT COUNT(*) FROM plugin_kv WHERE plugin_id = ? AND key <> ?)
				        + (SELECT COUNT(*) FROM plugin_task_data WHERE plugin_id = ?)
				        < ${PLUGIN_STORAGE_KEYS})
				 ON CONFLICT (plugin_id, key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at`,
				[
					params.pid,
					params.key,
					value,
					iso(ctx),
					params.pid,
					params.key,
					params.pid,
					byteLength(params.key) + byteLength(value) + PLUGIN_ROW_OVERHEAD,
					params.pid,
					params.key,
					params.pid,
				],
			);
			if (!rowsAffected) throw new LocalHttpError(413, 'plugin storage is full (5 MB or 1000 keys)');
			return { success: true };
		})
		.add('DELETE', 'plugins/:pid/storage/:key', async ({ ctx, params }) => {
			await ctx.db.execute(`DELETE FROM plugin_kv WHERE plugin_id = ? AND key = ?`, [params.pid, params.key]);
			return { success: true };
		})
		// ── per-task plugin data ────────────────────────────────────────────────
		.add('GET', 'plugins/:pid/tasks/:tid(\\d+)/data/:key', async ({ ctx, params }) => {
			await requireActiveTask(ctx, Number(params.tid));
			const [row] = await ctx.db.select<{ value: string }>(
				`SELECT value FROM plugin_task_data WHERE plugin_id = ? AND task_id = ? AND key = ?`,
				[params.pid, Number(params.tid), params.key],
			);
			return { value: row?.value ?? null };
		})
		.add('PUT', 'plugins/:pid/tasks/:tid(\\d+)/data/:key', async ({ ctx, params, body }) => {
			const taskId = Number(params.tid);
			const value = typeof body?.value === 'string' ? body.value : null;
			if (value === null) throw new LocalHttpError(422, 'value must be a JSON string');
			// EXISTS keeps "the task is active" and the write atomic, closing the race with a concurrent delete.
			const { rowsAffected } = await ctx.db.execute(
				`INSERT INTO plugin_task_data (plugin_id, task_id, key, value, updated_at)
				 SELECT ?, ?, ?, ?, ?
				 WHERE EXISTS (SELECT 1 FROM tasks WHERE id = ? AND deleted_at IS NULL)
				   AND ((SELECT COALESCE(SUM(LENGTH(CAST(key AS BLOB)) + LENGTH(CAST(value AS BLOB)) + ${PLUGIN_ROW_OVERHEAD}), 0)
				         FROM plugin_task_data WHERE plugin_id = ? AND NOT (task_id = ? AND key = ?))
				        + (SELECT COALESCE(SUM(LENGTH(CAST(key AS BLOB)) + LENGTH(CAST(value AS BLOB)) + ${PLUGIN_ROW_OVERHEAD}), 0)
				           FROM plugin_kv WHERE plugin_id = ?)
				        + ? <= ${PLUGIN_STORAGE_QUOTA})
				   AND ((SELECT COUNT(*) FROM plugin_task_data WHERE plugin_id = ? AND NOT (task_id = ? AND key = ?))
				        + (SELECT COUNT(*) FROM plugin_kv WHERE plugin_id = ?)
				        < ${PLUGIN_STORAGE_KEYS})
				 ON CONFLICT (plugin_id, task_id, key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at`,
				[
					params.pid,
					taskId,
					params.key,
					value,
					iso(ctx),
					taskId,
					params.pid,
					taskId,
					params.key,
					params.pid,
					byteLength(params.key) + byteLength(value) + PLUGIN_ROW_OVERHEAD,
					params.pid,
					taskId,
					params.key,
					params.pid,
				],
			);
			if (!rowsAffected) {
				await requireActiveTask(ctx, taskId);
				throw new LocalHttpError(413, 'plugin storage is full (5 MB or 1000 keys)');
			}
			return { success: true };
		})
		.add('DELETE', 'plugins/:pid/tasks/:tid(\\d+)/data/:key', async ({ ctx, params }) => {
			await requireActiveTask(ctx, Number(params.tid));
			await ctx.db.execute(`DELETE FROM plugin_task_data WHERE plugin_id = ? AND task_id = ? AND key = ?`, [
				params.pid,
				Number(params.tid),
				params.key,
			]);
			return { success: true };
		})
		.add('POST', 'plugins/:pid/task-data/query', async ({ ctx, params, body }) => {
			const taskIds = Array.isArray(body?.task_ids) ? body.task_ids.map(Number).slice(0, 500) : [];
			const key = String(body?.key ?? '');
			if (!taskIds.length || !key) return {};
			const placeholders = taskIds.map(() => '?').join(',');
			const rows = await ctx.db.select<{ task_id: number; value: string }>(
				`SELECT task_id, value FROM plugin_task_data WHERE plugin_id = ? AND key = ? AND task_id IN (${placeholders})`,
				[params.pid, key, ...taskIds],
			);
			const result: Record<number, string> = {};
			for (const row of rows) result[row.task_id] = row.value;
			return result;
		})
		// ── agent work ──────────────────────────────────────────────────────────
		.add('GET', 'tasks/:id(\\d+)/agent-work', async ({ ctx, params }) => {
			const task = await requireActiveTask(ctx, Number(params.id));
			const rows = await ctx.db.select<any>(
				`SELECT * FROM agent_work_runs WHERE task_id = ? ORDER BY started_at DESC, id DESC LIMIT 50`,
				[task.id],
			);
			const runningRows = await ctx.db.select<any>(
				`SELECT started_at FROM agent_work_runs WHERE task_id = ? AND status = 'running'`,
				[task.id],
			);
			const [{ finished }] = await ctx.db.select<{ finished: number }>(
				`SELECT COALESCE(SUM(duration_seconds), 0) AS finished FROM agent_work_runs
				 WHERE task_id = ? AND status <> 'running'`,
				[task.id],
			);
			const now = epoch(ctx);
			const running = runningRows.reduce(
				(sum, row) => sum + Math.max(0, now - Math.floor(new Date(row.started_at).getTime() / 1000)),
				0,
			);
			const personaNames = await personaNamesFor(ctx, rows);
			return {
				runs: rows.map((row) => agentWorkJson(row, ctx, personaNames)),
				totals: {
					agent_seconds: Number(finished) + running,
					human_seconds: Number(task.common_time ?? 0) + (task.start_time > 0 ? Math.max(0, now - task.start_time) : 0),
					human_timer_running: Number(task.start_time ?? 0) > 0,
				},
			};
		})
		.add('POST', 'tasks/:id(\\d+)/agent-work', async ({ ctx, params, body }) => {
			const taskId = Number(params.id);
			// A plugin's DataApi already sends "plugin:<pluginId>[/<agent>]"; this just stores it as given.
			const agent = normalizeAgent(body?.agent);
			const model = limitedOrNull(body?.model, 128);
			const sessionId = limitedOrNull(body?.session_id, 191);
			const branch = limitedOrNull(body?.branch, 255);
			const actor = actorOf(ctx);
			const ownerId = ownerIdOf(actor);
			const now = iso(ctx);
			await ctx.db.execute(
				`UPDATE agent_work_runs SET status = 'abandoned', ended_at = COALESCE(updated_at, started_at),
					duration_seconds = CAST((julianday(COALESCE(updated_at, started_at)) - julianday(started_at)) * 86400 AS INTEGER),
					version = version + 1, updated_at = ?
				 WHERE task_id = ? AND actor_kind = ? AND actor_id = ? AND agent = ? AND status = 'running'`,
				[now, taskId, actor.kind, ownerId, agent],
			);
			// EXISTS keeps the check and the insert atomic: the task cannot vanish between them.
			const result = await ctx.db.execute(
				`INSERT INTO agent_work_runs
					(task_id, agent, model, session_id, branch, status, started_at, actor_kind, actor_id, version, created_at, updated_at)
				 SELECT ?, ?, ?, ?, ?, 'running', ?, ?, ?, 1, ?, ?
				 WHERE EXISTS (SELECT 1 FROM tasks WHERE id = ? AND deleted_at IS NULL)`,
				[taskId, agent, model, sessionId, branch, now, actor.kind, ownerId, now, now, taskId],
			);
			if (!result.rowsAffected) throw notFound('Task');
			const [row] = await ctx.db.select<any>(`SELECT * FROM agent_work_runs WHERE id = ?`, [
				Number(result.lastInsertId),
			]);
			return agentWorkJson(row, ctx, await personaNamesFor(ctx, [row]));
		}, 201)
		.add('PATCH', 'agent-work/:id(\\d+)', async ({ ctx, params, body }) => {
			const run = await requireOwnRunningRun(ctx, Number(params.id));
			const { sets, values } = progressAssignments(body, true);
			sets.push('version = version + 1', 'updated_at = ?');
			values.push(iso(ctx));
			await ctx.db.execute(`UPDATE agent_work_runs SET ${sets.join(', ')} WHERE id = ?`, [...values, run.id]);
			const [row] = await ctx.db.select<any>(`SELECT * FROM agent_work_runs WHERE id = ?`, [run.id]);
			return agentWorkJson(row, ctx, await personaNamesFor(ctx, [row]));
		})
		.add('POST', 'agent-work/:id(\\d+)/finish', async ({ ctx, params, body }) => {
			const run = await requireOwnRunningRun(ctx, Number(params.id));
			const status = finishedStatus(body?.status);
			const { sets, values } = progressAssignments(body, false);
			const now = ctx.now();
			const started = new Date(run.started_at);
			const end = now < started ? started : now;
			const durationSeconds = Math.round((end.getTime() - started.getTime()) / 1000);
			sets.push('status = ?', 'ended_at = ?', 'duration_seconds = ?', 'version = version + 1', 'updated_at = ?');
			values.push(status, end.toISOString(), durationSeconds, iso(ctx));
			await ctx.db.execute(`UPDATE agent_work_runs SET ${sets.join(', ')} WHERE id = ?`, [...values, run.id]);
			const [row] = await ctx.db.select<any>(`SELECT * FROM agent_work_runs WHERE id = ?`, [run.id]);
			return agentWorkJson(row, ctx, await personaNamesFor(ctx, [row]));
		})
		// ── workspace-level odds and ends ───────────────────────────────────────
		.add('GET', 'workspaces/:wid/members', ({ ctx }) => [
			{ id: ctx.user.id, name: ctx.user.name, email: ctx.user.email, role: 'owner', has_avatar: false },
		])
		.add('GET', 'workspaces/:wid/feature-toggles', () =>
			Object.fromEntries(
				Object.entries(FEATURE_TOGGLES).map(([key, enabled]) => [key, { key, name: key, group: 'local', enabled }]),
			),
		);
	addRoutineRoutes(router);
	return router;
};

export const MAX_FILE_BYTES = 25 * 1024 * 1024;

/** Keys the file scheme accepts: `<uuid>/<name of [A-Za-z0-9._-]>`. */
export const fileKey = (name: string, id: string) => {
	const base =
		name
			.normalize('NFKD')
			.replace(/[^A-Za-z0-9._-]+/g, '-')
			.replace(/^[-.]+|-+$/g, '')
			.slice(0, 120) || 'file';
	return `${id}/${base}`;
};

const fileJson = (row: any, ctx: LocalContext) => ({
	id: row.id,
	task_id: row.task_id,
	user_id: ctx.user.id,
	workspace_id: ctx.workspace.id,
	name: row.name,
	original_name: row.name,
	file_path: row.file_path,
	mime_type: row.mime_type,
	size: row.size,
	created_at: row.created_at,
});

const loadFile = async (ctx: LocalContext, id: number) => {
	const [row] = await ctx.db.select<any>(`SELECT * FROM files WHERE id = ?`, [id]);
	if (!row) throw notFound('File');
	return row;
};

const commentJson = (row: any, ctx: LocalContext) => ({
	id: row.id,
	task_id: row.task_id,
	message: row.message,
	content: row.message,
	created_at: row.created_at,
	updated_at: row.updated_at,
	user_id: ctx.user.id,
	user: { id: ctx.user.id, name: ctx.user.name, email: ctx.user.email, has_avatar: false },
	author: {
		kind: row.author_kind ?? 'user',
		id: row.author_id ?? String(ctx.user.id),
		name: row.author_name ?? ctx.user.name,
		...((row.author_kind ?? 'user') === 'user'
			? {}
			: { owner: { id: String(ctx.user.id), name: ctx.user.name } }),
	},
	cursor_agent_id: null,
	cursor_message_type: null,
});

/** Per-comment reaction summaries, `reacted` and `users` relative to who is asking (see `actorOf`). */
const reactionsFor = async (ctx: LocalContext, commentIds: number[]) => {
	const result = new Map<number, { emoji: string; count: number; reacted: boolean; users: { id: number; name: string }[] }[]>();
	if (!commentIds.length) return result;
	const placeholders = commentIds.map(() => '?').join(',');
	const rows = await ctx.db.select<any>(
		`SELECT comment_id, emoji, actor_kind, actor_id FROM comment_reactions WHERE comment_id IN (${placeholders})`,
		commentIds,
	);
	const actor = actorOf(ctx);
	const ownerId = ownerIdOf(actor);
	const byComment = new Map<number, Map<string, { emoji: string; count: number; reacted: boolean; users: { id: number; name: string }[] }>>();
	for (const row of rows) {
		const perComment = byComment.get(row.comment_id) ?? new Map();
		byComment.set(row.comment_id, perComment);
		const entry = perComment.get(row.emoji) ?? { emoji: row.emoji, count: 0, reacted: false, users: [] };
		entry.count += 1;
		if (row.actor_kind === actor.kind && String(row.actor_id) === ownerId) entry.reacted = true;
		if (row.actor_kind === 'user') entry.users.push({ id: Number(row.actor_id), name: ctx.user.name });
		perComment.set(row.emoji, entry);
	}
	for (const [commentId, perComment] of byComment) result.set(commentId, [...perComment.values()]);
	return result;
};

const slugOf = (title: string) =>
	title
		.toLowerCase()
		.replace(/[^a-z0-9]+/g, '-')
		.replace(/^-+|-+$/g, '') || 'category';

