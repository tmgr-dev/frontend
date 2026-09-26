import { LocalRouter } from './router';
import {
	categoryJson,
	paginate,
	statusJson,
	TASK_SELECT,
	taskJson,
	toJson,
} from './serialize';
import { LocalHttpError, type LocalContext, type LocalRequest } from './types';

const iso = (ctx: LocalContext) => ctx.now().toISOString();
const epoch = (ctx: LocalContext) => Math.floor(ctx.now().getTime() / 1000);

const notFound = (what: string) => new LocalHttpError(404, `${what} not found`);

const loadTask = async (ctx: LocalContext, id: number) => {
	const rows = await ctx.db.select(`${TASK_SELECT} WHERE t.id = ? AND t.deleted_at IS NULL`, [id]);
	if (!rows.length) throw notFound('Task');
	return taskJson(rows[0], ctx);
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
	if ('expired_at' in body) fields.expired_at = body.expired_at ?? null;
	if ('common_time' in body && body.common_time !== undefined)
		fields.common_time = Math.max(0, Number(body.common_time) || 0);
	return fields;
};

const updateTask = async (ctx: LocalContext, id: number, fields: Record<string, any>) => {
	const keys = Object.keys(fields);
	const sets = [...keys.map((k) => `${k} = ?`), 'updated_at = ?'];
	const result = await ctx.db.execute(
		`UPDATE tasks SET ${sets.join(', ')} WHERE id = ? AND deleted_at IS NULL`,
		[...keys.map((k) => fields[k]), iso(ctx), id],
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
	return clauses;
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

const FEATURE_TOGGLES: Record<string, boolean> = {
	board: true,
	categories: true,
	dashboard: false,
	daily_routines: false,
	'task.countdown': true,
	'task.checkpoints': true,
	'task.assignees': false,
	'task.files': false,
	'task.relations': false,
};

export const createLocalApi = () =>
	new LocalRouter()
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
			const fields = writableTaskFields(body ?? {});
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
		.add('GET', 'tasks/settings', () => [])
		.add('GET', 'tasks/:id(\\d+)', ({ ctx, params }) => loadTask(ctx, Number(params.id)))
		.add('PUT', 'tasks/:id(\\d+)', ({ ctx, params, body }) =>
			updateTask(ctx, Number(params.id), writableTaskFields(body ?? {})),
		)
		.add('PATCH', 'tasks/:id(\\d+)', ({ ctx, params, body }) =>
			updateTask(ctx, Number(params.id), writableTaskFields(body ?? {})),
		)
		.add('DELETE', 'tasks/:id(\\d+)', async ({ ctx, params }) => {
			const id = Number(params.id);
			await stopTimer(ctx, id);
			const result = await ctx.db.execute(
				`UPDATE tasks SET deleted_at = ?, updated_at = ? WHERE id = ? AND deleted_at IS NULL`,
				[iso(ctx), iso(ctx), id],
			);
			if (!result.rowsAffected) throw notFound('Task');
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
			const rows = await ctx.db.select(
				`SELECT * FROM comments WHERE task_id = ? AND deleted_at IS NULL ORDER BY id ASC`,
				[Number(params.id)],
			);
			return rows.map((row: any) => commentJson(row, ctx));
		})
		.add('POST', 'tasks/:id(\\d+)/comments', async ({ ctx, params, body }) => {
			const message = String(body?.message ?? body?.content ?? '').trim();
			if (!message) throw new LocalHttpError(422, 'message is required');
			await loadTask(ctx, Number(params.id));
			const now = iso(ctx);
			const result = await ctx.db.execute(
				`INSERT INTO comments (task_id, message, created_at, updated_at) VALUES (?, ?, ?, ?)`,
				[Number(params.id), message, now, now],
			);
			const [row] = await ctx.db.select(`SELECT * FROM comments WHERE id = ?`, [Number(result.lastInsertId)]);
			return commentJson(row, ctx);
		}, 201)
		.add('DELETE', 'comments/:id(\\d+)', async ({ ctx, params }) => {
			await ctx.db.execute(`UPDATE comments SET deleted_at = ? WHERE id = ?`, [iso(ctx), Number(params.id)]);
			return { success: true };
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
		.add('GET', 'project_categories/:id(\\d+)', async ({ ctx, params }) => {
			const rows = await ctx.db.select(`${categoryCounts} WHERE c.id = ? AND c.deleted_at IS NULL`, [Number(params.id)]);
			if (!rows.length) throw notFound('Category');
			return categoryJson(rows[0], ctx);
		})
		.add('POST', 'project_categories', async ({ ctx, body }) => {
			const title = String(body?.title ?? '').trim();
			if (!title) throw new LocalHttpError(422, 'title is required');
			const now = iso(ctx);
			const result = await ctx.db.execute(
				`INSERT INTO categories (title, slug, code, parent_id, settings, created_at, updated_at)
				 VALUES (?, ?, ?, ?, ?, ?, ?)`,
				[
					title,
					body?.slug ?? slugOf(title),
					body?.code ? String(body.code).toUpperCase() : null,
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
			await ctx.db.execute(
				`UPDATE categories SET title = ?, code = ?, parent_id = ?, settings = ?, updated_at = ? WHERE id = ?`,
				[
					body?.title ?? row.title,
					'code' in (body ?? {}) ? (body.code ? String(body.code).toUpperCase() : null) : row.code,
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
		// ── workspace-level odds and ends ───────────────────────────────────────
		.add('GET', 'workspaces/:wid/members', ({ ctx }) => [
			{ id: ctx.user.id, name: ctx.user.name, email: ctx.user.email, role: 'owner', has_avatar: false },
		])
		.add('GET', 'workspaces/:wid/feature-toggles', () =>
			Object.fromEntries(
				Object.entries(FEATURE_TOGGLES).map(([key, enabled]) => [key, { key, name: key, group: 'local', enabled }]),
			),
		)
		.add('GET', 'daily-routines/tasks/count', () => ({ count: 0 }));

const commentJson = (row: any, ctx: LocalContext) => ({
	id: row.id,
	task_id: row.task_id,
	message: row.message,
	content: row.message,
	created_at: row.created_at,
	updated_at: row.updated_at,
	user_id: ctx.user.id,
	user: { id: ctx.user.id, name: ctx.user.name, email: ctx.user.email, has_avatar: false },
	reactions: [],
	cursor_agent_id: null,
	cursor_message_type: null,
});

const slugOf = (title: string) =>
	title
		.toLowerCase()
		.replace(/[^a-z0-9]+/g, '-')
		.replace(/^-+|-+$/g, '') || 'category';

