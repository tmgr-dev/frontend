import { parseJson, toJson } from '../serialize';
import { LocalHttpError, type LocalContext } from '../types';
import {
	addDays,
	firesOn,
	localDate,
	localDateTime,
	nextInstance,
	normalizeRecurrence,
	resolveCategory,
	type RecurrencePattern,
} from './recurrence';
import {
	buildOrphanEntry,
	buildPatternEntry,
	buildPlainEntry,
	instanceJson,
} from './serialize';

export const ROUTINE_ID_BASE = 1_000_000_000;

export const isRoutineId = (id: number): boolean => id > ROUTINE_ID_BASE;

export interface RoutineEntryJson {
	task_id: number;
	title: string;
	description: string | null;
	routine_category: { id: string; name: string; color: string };
	date: string;
	time: string | null;
	scheduled_for: string | null;
	duration_min: number | null;
	reminder_min: number | null;
	frequency: string | null;
	status: string;
	completed: boolean;
	instance_id: number | null;
	virtual: boolean;
	created_at: string | null;
	updated_at: string | null;
}

export const notFoundRoutine = (): LocalHttpError => new LocalHttpError(404, 'Routine task not found');

const iso = (ctx: LocalContext) => ctx.now().toISOString();

const numberOrNull = (value: any): number | null =>
	value === null || value === undefined || value === '' ? null : Number(value);

const normalizeDateField = (value: any): string | null =>
	typeof value === 'string' && value.length >= 10 ? value.slice(0, 10) : null;

/** Accepts `{hours,minutes[,seconds]}` or an `"HH:mm[:ss]"` string, like the Java DTOs do. */
const resolveTimeInput = (raw: any): string | null => {
	const pad2 = (n: number) => String(n).padStart(2, '0');
	if (raw && typeof raw === 'object' && !Array.isArray(raw)) {
		const h = raw.hours;
		const m = raw.minutes;
		if (typeof h === 'number' && typeof m === 'number') {
			return `${pad2(h)}:${pad2(m)}:${pad2(typeof raw.seconds === 'number' ? raw.seconds : 0)}`;
		}
		return null;
	}
	if (typeof raw === 'string' && raw.includes(':')) {
		const [h, m, s] = raw.split(':');
		const hh = Number(h);
		const mm = Number(m);
		if (Number.isNaN(hh) || Number.isNaN(mm)) return null;
		return `${pad2(hh)}:${pad2(mm)}:${pad2(Number(s ?? 0) || 0)}`;
	}
	return null;
};

// ── routine row access ──────────────────────────────────────────────────────

/** Any routine that has not been (soft) deleted — includes archived ones. */
export const loadRoutineRow = async (ctx: LocalContext, id: number) => {
	const rows = await ctx.db.select<any>(`SELECT * FROM routines WHERE id = ? AND deleted_at IS NULL`, [id]);
	return rows[0] ?? null;
};

/** A routine visible to normal reads/writes: neither archived nor deleted (matches Java's single deleted_at gate). */
export const loadActiveRoutine = async (ctx: LocalContext, id: number) => {
	const rows = await ctx.db.select<any>(
		`SELECT * FROM routines WHERE id = ? AND deleted_at IS NULL AND archived_at IS NULL`,
		[id],
	);
	return rows[0] ?? null;
};

const patternFromRow = (row: any): RecurrencePattern => ({
	frequency: row.frequency,
	interval: row.interval ?? 1,
	day_of_frequency: row.day_of_frequency ?? null,
	month: row.month ?? null,
	days_of_week: parseJson<string[]>(row.days_of_week, []),
	start_date: row.start_at ?? null,
	end_date: row.end_at ?? null,
	occurrences: row.occurrences ?? null,
	scheduled_time: row.scheduled_time ?? null,
	duration_min: row.duration_min ?? null,
	reminder_min: row.reminder_min ?? null,
});

export const savePattern = async (ctx: LocalContext, routineId: number, pattern: RecurrencePattern) => {
	const now = iso(ctx);
	await ctx.db.execute(
		`INSERT INTO routine_patterns
			(routine_id, frequency, interval, day_of_frequency, month, days_of_week, start_at, end_at,
			 occurrences, scheduled_time, duration_min, reminder_min, created_at, updated_at)
		 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
		 ON CONFLICT (routine_id) DO UPDATE SET
			frequency = excluded.frequency, interval = excluded.interval, day_of_frequency = excluded.day_of_frequency,
			month = excluded.month, days_of_week = excluded.days_of_week, start_at = excluded.start_at,
			end_at = excluded.end_at, occurrences = excluded.occurrences, scheduled_time = excluded.scheduled_time,
			duration_min = excluded.duration_min, reminder_min = excluded.reminder_min, updated_at = excluded.updated_at`,
		[
			routineId,
			pattern.frequency,
			pattern.interval,
			pattern.day_of_frequency,
			pattern.month,
			toJson(pattern.days_of_week),
			pattern.start_date,
			pattern.end_date,
			pattern.occurrences,
			pattern.scheduled_time,
			pattern.duration_min,
			pattern.reminder_min,
			now,
			now,
		],
	);
};

/** Create-or-update the one instance for a (routine, date) pair; a null time stores midnight (unscheduled). */
const upsertInstanceForDate = async (ctx: LocalContext, routineId: number, date: string, time: string | null) => {
	const dayStart = `${date}T00:00:00`;
	const dayEnd = `${addDays(date, 1)}T00:00:00`;
	const [existing] = await ctx.db.select<any>(
		`SELECT id FROM routine_instances WHERE routine_id = ? AND scheduled_for >= ? AND scheduled_for < ?`,
		[routineId, dayStart, dayEnd],
	);
	const now = iso(ctx);
	const scheduledFor = `${date}T${time ?? '00:00:00'}`;
	if (existing) {
		await ctx.db.execute(`UPDATE routine_instances SET scheduled_for = ?, updated_at = ? WHERE id = ?`, [
			scheduledFor,
			now,
			existing.id,
		]);
		return Number(existing.id);
	}
	const result = await ctx.db.execute(
		`INSERT INTO routine_instances (routine_id, scheduled_for, status, created_at, updated_at) VALUES (?, ?, 'PENDING', ?, ?)`,
		[routineId, scheduledFor, now, now],
	);
	return Number(result.lastInsertId);
};

// ── create ───────────────────────────────────────────────────────────────────

const settingsOf = (body: any): string => (Array.isArray(body?.settings) ? toJson(body.settings)! : '[]');

/** One statement so the disjoint routine-id range assignment is atomic with the insert. */
const insertRoutineRow = async (ctx: LocalContext, fields: Record<string, any>) => {
	const keys = Object.keys(fields);
	const now = iso(ctx);
	const result = await ctx.db.execute(
		`INSERT INTO routines (id, ${keys.join(', ')}, created_at, updated_at)
		 SELECT COALESCE(MAX(id), ${ROUTINE_ID_BASE}) + 1, ${keys.map(() => '?').join(', ')}, ?, ?
		 FROM routines`,
		[...keys.map((k) => fields[k]), now, now],
	);
	return Number(result.lastInsertId);
};

export const createRoutine = async (ctx: LocalContext, body: any) => {
	const title = String(body?.title ?? '').trim();
	if (!title) throw new LocalHttpError(422, 'title is required');
	const routineId = await insertRoutineRow(ctx, {
		title,
		description: body?.description ?? null,
		routine_category: 'routine_category' in (body ?? {}) ? body.routine_category : null,
		priority: body?.priority || 'medium',
		approximately_time: numberOrNull(body?.approximately_time),
		settings: settingsOf(body),
	});
	return loadRoutineRow(ctx, routineId);
};

export const createRecurringRoutine = async (ctx: LocalContext, body: any) => {
	const title = String(body?.title ?? '').trim();
	if (!title) throw new LocalHttpError(422, 'title is required');
	const routineId = await insertRoutineRow(ctx, {
		title,
		description: body?.description ?? null,
		routine_category: 'routine_category' in (body ?? {}) ? body.routine_category : null,
		priority: body?.priority || 'medium',
		approximately_time: numberOrNull(body?.approximately_time),
		settings: settingsOf(body),
	});
	const pattern = normalizeRecurrence(body?.recurrence);
	if (pattern) await savePattern(ctx, routineId, pattern);
	if (body?.scheduled_date) {
		const date = normalizeDateField(body.scheduled_date) ?? localDate(ctx.now());
		await upsertInstanceForDate(ctx, routineId, date, resolveTimeInput(body?.scheduled_time));
	}
	return loadRoutineRow(ctx, routineId);
};

export const quickCreateRoutine = async (ctx: LocalContext, body: any) => {
	const title = String(body?.title ?? '').trim();
	if (!title) throw new LocalHttpError(422, 'title is required');
	const routineId = await insertRoutineRow(ctx, { title, priority: 'medium', settings: '[]' });
	const today = localDate(ctx.now());
	const requestedDate = normalizeDateField(body?.scheduled_date ?? body?.date);
	const date = requestedDate ?? today;
	const time = resolveTimeInput(body?.scheduled_time ?? body?.time);
	const undated = !time && (!requestedDate || (date >= addDays(today, -1) && date <= addDays(today, 1)));
	if (undated) return loadRoutineRow(ctx, routineId);
	await upsertInstanceForDate(ctx, routineId, date, time);
	return loadRoutineRow(ctx, routineId);
};

// ── update / archive / destroy ───────────────────────────────────────────────

export const updateRoutine = async (ctx: LocalContext, row: any, body: any) => {
	const now = iso(ctx);
	const fields: Record<string, any> = {};
	if ('title' in body && body.title != null) fields.title = String(body.title);
	if ('description' in body) fields.description = body.description ?? null;
	if ('routine_category' in body) fields.routine_category = body.routine_category ?? null;
	if (body?.priority) fields.priority = String(body.priority);
	if ('approximately_time' in body) fields.approximately_time = numberOrNull(body.approximately_time);
	if (Array.isArray(body?.settings)) fields.settings = toJson(body.settings);
	if (Object.keys(fields).length) {
		const sets = [...Object.keys(fields).map((k) => `${k} = ?`), 'updated_at = ?'];
		await ctx.db.execute(`UPDATE routines SET ${sets.join(', ')} WHERE id = ?`, [
			...Object.values(fields),
			now,
			row.id,
		]);
	}
	if (body?.is_recurring === false) {
		await ctx.db.execute(`DELETE FROM routine_patterns WHERE routine_id = ?`, [row.id]);
	} else {
		const pattern = normalizeRecurrence(body?.recurrence);
		if (pattern) await savePattern(ctx, row.id, pattern);
	}
	if (body?.unscheduled === true) {
		await ctx.db.execute(`DELETE FROM routine_instances WHERE routine_id = ? AND status = 'PENDING'`, [row.id]);
		await ctx.db.execute(`UPDATE routines SET scheduled_date = NULL, scheduled_time = NULL WHERE id = ?`, [row.id]);
	} else if (body?.scheduled_date) {
		const date = normalizeDateField(body.scheduled_date) ?? localDate(ctx.now());
		await upsertInstanceForDate(ctx, row.id, date, resolveTimeInput(body?.scheduled_time));
	}
	return loadRoutineRow(ctx, row.id);
};

/** Honours only `approximately_time` and `title`, for the day view's `PUT tasks/{id}` resize call. */
export const updateRoutineTaskFields = async (ctx: LocalContext, id: number, body: any) => {
	const row = await loadRoutineRow(ctx, id);
	if (!row) throw notFoundRoutine();
	const fields: Record<string, any> = {};
	if ('title' in body && body.title != null) {
		const title = String(body.title).trim();
		if (title) fields.title = title;
	}
	if ('approximately_time' in body) fields.approximately_time = Number(body.approximately_time ?? 0) || 0;
	if (Object.keys(fields).length) {
		const sets = [...Object.keys(fields).map((k) => `${k} = ?`), 'updated_at = ?'];
		await ctx.db.execute(`UPDATE routines SET ${sets.join(', ')} WHERE id = ?`, [
			...Object.values(fields),
			iso(ctx),
			id,
		]);
	}
	return loadRoutineRow(ctx, id);
};

export const archiveRoutine = async (ctx: LocalContext, row: any) => {
	await ctx.db.execute(`UPDATE routines SET archived_at = ?, updated_at = ? WHERE id = ?`, [
		iso(ctx),
		iso(ctx),
		row.id,
	]);
	return loadRoutineRow(ctx, row.id);
};

/** Soft-deletes (consistent with tasks/categories elsewhere in this local API); Java hard-deletes the row. */
export const destroyRoutine = async (ctx: LocalContext, id: number) => {
	const result = await ctx.db.execute(
		`UPDATE routines SET deleted_at = ?, updated_at = ? WHERE id = ? AND deleted_at IS NULL`,
		[iso(ctx), iso(ctx), id],
	);
	if (!result.rowsAffected) throw notFoundRoutine();
};

export const convertRoutineToTask = async (ctx: LocalContext, row: any, body: any) => {
	const projectCategoryId = numberOrNull(body?.project_category_id);
	if (projectCategoryId) {
		const rows = await ctx.db.select<any>(`SELECT id FROM categories WHERE id = ? AND deleted_at IS NULL`, [
			projectCategoryId,
		]);
		if (!rows.length) throw new LocalHttpError(422, 'The project category does not belong to the target workspace.');
	}
	const requestedStatusId = numberOrNull(body?.status_id);
	if (requestedStatusId) {
		const rows = await ctx.db.select<any>(`SELECT id FROM statuses WHERE id = ?`, [requestedStatusId]);
		if (!rows.length) throw new LocalHttpError(422, 'The status does not belong to the target workspace.');
	}
	const statusId = requestedStatusId ?? (await defaultStatusId(ctx));
	const now = iso(ctx);
	const result = await ctx.db.execute(
		`INSERT INTO tasks (title, description, status_id, project_category_id, priority, approximately_time,
			category_tasks_sequence_id, created_at, updated_at)
		 SELECT ?, ?, ?, ?, ?, ?,
			CASE WHEN ? IS NULL THEN NULL ELSE (SELECT COALESCE(MAX(category_tasks_sequence_id), 0) + 1 FROM tasks WHERE project_category_id = ?) END,
			?, ?`,
		[
			row.title,
			row.description,
			statusId,
			projectCategoryId,
			row.priority ?? 'medium',
			row.approximately_time ?? 0,
			projectCategoryId,
			projectCategoryId,
			now,
			now,
		],
	);
	await ctx.db.execute(`DELETE FROM routines WHERE id = ?`, [row.id]);
	return Number(result.lastInsertId);
};

const defaultStatusId = async (ctx: LocalContext): Promise<number | null> => {
	const statuses = await ctx.db.select<any>(
		`SELECT * FROM statuses ORDER BY CASE WHEN type = 'archived' THEN 1 ELSE 0 END, sort_order, id`,
	);
	const pick =
		statuses.find((s) => s.type === 'default') ??
		statuses.find((s) => s.type === 'active') ??
		statuses.find((s) => s.type !== 'archived') ??
		statuses[0];
	return pick?.id ?? null;
};

// ── completion / instances ───────────────────────────────────────────────────

export const completeForDate = async (ctx: LocalContext, routineId: number, date: string) => {
	const dayStart = `${date}T00:00:00`;
	const dayEnd = `${addDays(date, 1)}T00:00:00`;
	let [row] = await ctx.db.select<any>(
		`SELECT * FROM routine_instances WHERE routine_id = ? AND scheduled_for >= ? AND scheduled_for < ?`,
		[routineId, dayStart, dayEnd],
	);
	const now = iso(ctx);
	if (!row) {
		const result = await ctx.db.execute(
			`INSERT INTO routine_instances (routine_id, scheduled_for, status, created_at, updated_at) VALUES (?, ?, 'PENDING', ?, ?)`,
			[routineId, dayStart, now, now],
		);
		[row] = await ctx.db.select<any>(`SELECT * FROM routine_instances WHERE id = ?`, [Number(result.lastInsertId)]);
	}
	const wasCompleted = row.status === 'COMPLETED';
	if (wasCompleted) {
		const [{ n }] = await ctx.db.select<{ n: number }>(
			`SELECT COUNT(*) AS n FROM routine_instances WHERE routine_id = ?`,
			[routineId],
		);
		const [pattern] = await ctx.db.select<any>(
			`SELECT frequency FROM routine_patterns WHERE routine_id = ? AND frequency IS NOT NULL AND frequency NOT IN ('', 'NONE')`,
			[routineId],
		);
		const [routine] = await ctx.db.select<any>(`SELECT created_at FROM routines WHERE id = ?`, [routineId]);
		const gapMs = (x: any, y: any) => Math.abs(Date.parse(x) - Date.parse(y));
		const createdByCompletion = gapMs(row.created_at, row.completed_at) <= 60_000;
		const createdWithRoutine =
			!!routine &&
			gapMs(row.created_at, routine.created_at) <= 60_000 &&
			Math.abs(Date.parse(`${date}T00:00:00Z`) - Date.parse(`${String(routine.created_at).slice(0, 10)}T00:00:00Z`)) <=
				86_400_000;
		if (
			!pattern &&
			Number(n) === 1 &&
			String(row.scheduled_for).slice(11, 19) === '00:00:00' &&
			(createdByCompletion || createdWithRoutine)
		) {
			await ctx.db.execute(`DELETE FROM routine_instances WHERE id = ?`, [row.id]);
			return { ...row, id: null, status: 'PENDING', completed_at: null, skipped_at: null, updated_at: now };
		}
	}
	const status = wasCompleted ? 'PENDING' : 'COMPLETED';
	await ctx.db.execute(
		`UPDATE routine_instances SET status = ?, completed_at = ?, skipped_at = NULL, updated_at = ? WHERE id = ?`,
		[status, wasCompleted ? null : now, now, row.id],
	);
	const [updated] = await ctx.db.select<any>(`SELECT * FROM routine_instances WHERE id = ?`, [row.id]);
	return updated;
};

export const setInstanceStatus = async (
	ctx: LocalContext,
	routineId: number,
	instanceId: number,
	status: 'COMPLETED' | 'SKIPPED',
) => {
	const [row] = await ctx.db.select<any>(`SELECT id FROM routine_instances WHERE id = ? AND routine_id = ?`, [
		instanceId,
		routineId,
	]);
	if (!row) throw new LocalHttpError(404, 'Task instance not found for routine');
	const now = iso(ctx);
	await ctx.db.execute(
		`UPDATE routine_instances SET status = ?, completed_at = ?, skipped_at = ?, updated_at = ? WHERE id = ?`,
		[status, status === 'COMPLETED' ? now : null, status === 'SKIPPED' ? now : null, now, instanceId],
	);
	const [updated] = await ctx.db.select<any>(`SELECT * FROM routine_instances WHERE id = ?`, [instanceId]);
	return updated;
};

export const deleteInstance = async (ctx: LocalContext, routineId: number, instanceId: number) => {
	const [row] = await ctx.db.select<any>(`SELECT id FROM routine_instances WHERE id = ? AND routine_id = ?`, [
		instanceId,
		routineId,
	]);
	if (!row) throw new LocalHttpError(404, 'Task instance not found for routine');
	await ctx.db.execute(`DELETE FROM routine_instances WHERE id = ?`, [instanceId]);
};

export const resolveReschedule = (body: any): { date: string | null; time: string | null } => {
	const sf = typeof body?.scheduled_for === 'string' ? body.scheduled_for : null;
	if (sf && sf.length >= 10) {
		const date = sf.slice(0, 10);
		const rest = sf.length > 10 ? sf.slice(11).trim() : '';
		const time = rest ? (rest.length === 5 ? `${rest}:00` : rest) : resolveTimeInput(body?.scheduled_time);
		return { date, time };
	}
	return { date: normalizeDateField(body?.scheduled_date), time: resolveTimeInput(body?.scheduled_time) };
};

export const rescheduleVirtual = async (ctx: LocalContext, routineId: number, date: string, time: string | null) => {
	const now = iso(ctx);
	const scheduledFor = `${date}T${time ?? '00:00:00'}`;
	const result = await ctx.db.execute(
		`INSERT INTO routine_instances (routine_id, scheduled_for, status, created_at, updated_at) VALUES (?, ?, 'PENDING', ?, ?)`,
		[routineId, scheduledFor, now, now],
	);
	const [created] = await ctx.db.select<any>(`SELECT * FROM routine_instances WHERE id = ?`, [
		Number(result.lastInsertId),
	]);
	return created;
};

export const rescheduleInstance = async (
	ctx: LocalContext,
	routineId: number,
	instanceId: number,
	date: string,
	time: string | null,
) => {
	const [existing] = await ctx.db.select<any>(`SELECT * FROM routine_instances WHERE id = ? AND routine_id = ?`, [
		instanceId,
		routineId,
	]);
	if (!existing) throw new LocalHttpError(404, 'Task instance not found for routine');
	const keepTime = time ?? String(existing.scheduled_for).slice(11);
	const now = iso(ctx);
	await ctx.db.execute(`UPDATE routine_instances SET scheduled_for = ?, updated_at = ? WHERE id = ?`, [
		`${date}T${keepTime}`,
		now,
		instanceId,
	]);
	const [updated] = await ctx.db.select<any>(`SELECT * FROM routine_instances WHERE id = ?`, [instanceId]);
	return updated;
};

// ── expansion / scheduling ────────────────────────────────────────────────────

const PATTERN_SQL = `
	SELECT rp.*, r.title, r.description, r.routine_category, r.approximately_time, r.created_at, r.updated_at
	FROM routine_patterns rp JOIN routines r ON r.id = rp.routine_id
	WHERE r.deleted_at IS NULL AND r.archived_at IS NULL
		AND rp.frequency IS NOT NULL AND rp.frequency NOT IN ('', 'NONE')`;

const INSTANCE_SQL = `
	SELECT ri.* FROM routine_instances ri JOIN routines r ON r.id = ri.routine_id
	WHERE r.deleted_at IS NULL AND r.archived_at IS NULL
		AND ri.scheduled_for >= ? AND ri.scheduled_for < ?`;

const ORPHAN_SQL = `
	SELECT ri.*, r.title, r.description, r.routine_category, r.created_at, r.updated_at,
		rp.frequency AS pattern_frequency, COALESCE(rp.duration_min, r.approximately_time) AS duration_min, rp.reminder_min
	FROM routine_instances ri
	JOIN routines r ON r.id = ri.routine_id
	LEFT JOIN routine_patterns rp ON rp.routine_id = ri.routine_id
	WHERE r.deleted_at IS NULL AND r.archived_at IS NULL
		AND ri.scheduled_for >= ? AND ri.scheduled_for < ?
		AND (rp.id IS NULL OR rp.frequency IS NULL OR rp.frequency = '' OR rp.frequency = 'NONE')`;

const PLAIN_SQL = `
	SELECT r.id AS routine_id, r.title, r.description, r.routine_category, r.created_at, r.updated_at,
		r.scheduled_date, r.scheduled_time, r.approximately_time
	FROM routines r
	LEFT JOIN routine_patterns rp ON rp.routine_id = r.id
	WHERE r.deleted_at IS NULL AND r.archived_at IS NULL
		AND (rp.id IS NULL OR rp.frequency IS NULL OR rp.frequency = '' OR rp.frequency = 'NONE')
		AND NOT EXISTS (SELECT 1 FROM routine_instances ri2 WHERE ri2.routine_id = r.id)`;

const statusPriority = (status: string): number =>
	status === 'COMPLETED' ? 3 : status === 'SKIPPED' ? 2 : 1;

const buildInstanceMap = (rows: any[]): Map<string, any> => {
	const map = new Map<string, any>();
	for (const row of rows) {
		const date = String(row.scheduled_for).slice(0, 10);
		const key = `${row.routine_id}|${date}`;
		const existing = map.get(key);
		if (!existing || statusPriority(row.status) > statusPriority(existing.status)) map.set(key, row);
	}
	return map;
};

export const expandRange = async (ctx: LocalContext, from: string, to: string): Promise<RoutineEntryJson[]> => {
	const rangeStart = `${from}T00:00:00`;
	const rangeEnd = `${addDays(to, 1)}T00:00:00`;

	const patternRows = await ctx.db.select<any>(PATTERN_SQL);
	const instanceRows = await ctx.db.select<any>(INSTANCE_SQL, [rangeStart, rangeEnd]);
	const orphanRows = await ctx.db.select<any>(ORPHAN_SQL, [rangeStart, rangeEnd]);
	const plainRows = await ctx.db.select<any>(PLAIN_SQL);

	const instanceMap = buildInstanceMap(instanceRows);
	const results: RoutineEntryJson[] = [];

	let day = from;
	while (day <= to) {
		for (const row of patternRows) {
			if (firesOn(patternFromRow(row), day, from)) {
				const inst = instanceMap.get(`${row.routine_id}|${day}`) ?? null;
				results.push(buildPatternEntry(row, day, inst, resolveCategory));
			}
		}
		day = addDays(day, 1);
	}

	for (const row of orphanRows) results.push(buildOrphanEntry(row, resolveCategory));

	const today = localDate(ctx.now());
	for (const row of plainRows) {
		const day2 = row.scheduled_date ?? today;
		if (day2 >= from && day2 <= to) results.push(buildPlainEntry(row, day2, resolveCategory));
	}

	results.sort((a, b) => (a.scheduled_for ?? '').localeCompare(b.scheduled_for ?? ''));
	return results;
};

export const materializeDueInstances = async (ctx: LocalContext): Promise<number> => {
	const now = localDateTime(ctx.now());
	const rows = await ctx.db.select<any>(
		`SELECT rp.* FROM routine_patterns rp
		 JOIN routines r ON r.id = rp.routine_id
		 WHERE r.deleted_at IS NULL AND r.archived_at IS NULL
			AND rp.frequency IS NOT NULL AND rp.frequency NOT IN ('', 'NONE')`,
	);
	let created = 0;
	for (const row of rows) {
		const pattern = patternFromRow(row);
		const [latest] = await ctx.db.select<any>(
			`SELECT scheduled_for FROM routine_instances WHERE routine_id = ? ORDER BY scheduled_for DESC LIMIT 1`,
			[row.routine_id],
		);
		const next = nextInstance(pattern, latest ? latest.scheduled_for : null, now);
		if (!next) continue;
		if (pattern.occurrences != null) {
			const [{ n }] = await ctx.db.select<{ n: number }>(
				`SELECT COUNT(*) AS n FROM routine_instances WHERE routine_id = ?`,
				[row.routine_id],
			);
			if (Number(n) >= pattern.occurrences) continue;
		}
		const nextDate = next.slice(0, 10);
		const [existing] = await ctx.db.select<any>(
			`SELECT id FROM routine_instances WHERE routine_id = ? AND scheduled_for >= ? AND scheduled_for < ?`,
			[row.routine_id, `${nextDate}T00:00:00`, `${addDays(nextDate, 1)}T00:00:00`],
		);
		if (existing) continue;
		const nowIso = iso(ctx);
		await ctx.db.execute(
			`INSERT INTO routine_instances (routine_id, scheduled_for, status, created_at, updated_at) VALUES (?, ?, 'PENDING', ?, ?)`,
			[row.routine_id, next, nowIso, nowIso],
		);
		created++;
	}
	return created;
};

export { instanceJson };
