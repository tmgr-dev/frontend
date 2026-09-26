import type { LocalContext } from './types';

export const parseJson = <T>(raw: string | null | undefined, fallback: T): T => {
	if (raw === null || raw === undefined || raw === '') return fallback;
	try {
		return JSON.parse(raw) as T;
	} catch {
		return fallback;
	}
};

export const toJson = (value: unknown): string | null =>
	value === undefined || value === null ? null : JSON.stringify(value);

export const categoryJson = (row: any, ctx: LocalContext) =>
	row && {
		id: row.id,
		title: row.title,
		slug: row.slug,
		code: row.code,
		workspace_id: ctx.workspace.id,
		project_category_id: row.parent_id ?? null,
		parent_category: null,
		settings: parseJson(row.settings, []),
		children_count: Number(row.children_count ?? 0),
		tasks_count: Number(row.tasks_count ?? 0),
		user_id: ctx.user.id,
		user: { id: ctx.user.id, name: ctx.user.name },
		created_at: row.created_at,
		updated_at: row.updated_at,
		deleted_at: row.deleted_at ?? null,
	};

/** Row joined as `t.*, s.type AS status_type, c.*` aliases → the Task JSON the Java API returns. */
export const taskJson = (row: any, ctx: LocalContext) => ({
	id: row.id,
	title: row.title,
	description: row.description,
	description_json: parseJson(row.description_json, null),
	status: row.status_type ?? null,
	status_id: row.status_id,
	project_category_id: row.project_category_id,
	category_tasks_sequence_id: row.category_tasks_sequence_id,
	order: row.sort_order,
	priority: row.priority,
	common_time: Number(row.common_time ?? 0),
	start_time: Number(row.start_time ?? 0),
	end_time: row.end_time,
	approximately_time: Number(row.approximately_time ?? 0),
	checkpoints: parseJson(row.checkpoints, []),
	settings: parseJson(row.settings, []),
	expired_at: row.expired_at,
	is_daily_routine: false,
	daily_routine: false,
	user_id: ctx.user.id,
	user: { id: ctx.user.id, name: ctx.user.name },
	workspace_id: ctx.workspace.id,
	assignees: [],
	comments_count: Number(row.comments_count ?? 0),
	category: row.c_id
		? {
				id: row.c_id,
				title: row.c_title,
				slug: row.c_slug,
				code: row.c_code,
				workspace_id: ctx.workspace.id,
				project_category_id: row.c_parent_id ?? null,
		  }
		: null,
	created_at: row.created_at,
	updated_at: row.updated_at,
	deleted_at: row.deleted_at,
});

export const TASK_SELECT = `
	SELECT t.*, s.type AS status_type,
		c.id AS c_id, c.title AS c_title, c.slug AS c_slug, c.code AS c_code, c.parent_id AS c_parent_id,
		(SELECT COUNT(*) FROM comments m WHERE m.task_id = t.id AND m.deleted_at IS NULL) AS comments_count
	FROM tasks t
	LEFT JOIN statuses s ON s.id = t.status_id
	LEFT JOIN categories c ON c.id = t.project_category_id`;

export const statusJson = (row: any) => ({
	id: row.id,
	name: row.name,
	type: row.type,
	color: row.color,
	pivot: { status_id: row.id, is_active: 1, order: row.sort_order },
});

export const paginate = <T>(
	rows: T[],
	total: number,
	page: number,
	perPage: number,
	path: string,
	extra: Record<string, any> = {},
) => {
	const lastPage = Math.max(1, Math.ceil(total / perPage));
	const from = total ? (page - 1) * perPage + 1 : null;
	return {
		data: rows,
		links: { first: null, last: null, prev: null, next: null },
		meta: {
			current_page: page,
			from,
			last_page: lastPage,
			links: [],
			path,
			per_page: perPage,
			to: total ? Math.min(page * perPage, total) : null,
			total,
			...extra,
		},
	};
};
