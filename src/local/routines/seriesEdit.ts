import { parseJson, toJson } from '../serialize';
import {
	LocalHttpError,
	type BatchStatement,
	type LocalContext,
	type SqlValue,
} from '../types';
import { addDays, localDate } from './recurrence';
import { routineJson } from './serialize';
import {
	createdOn,
	fromSegment,
	frozenUntil,
	governing,
	liveVersion,
	normalizeDays,
	type Version,
} from './series';
import {
	destroyRoutine,
	iso,
	loadRoutineRow,
	normalizeDateField,
	numberOrNull,
	resolveTimeInput,
	updateRoutine,
} from './service';

const SCOPES = ['single', 'following', 'all'];

interface RecurrenceReq {
	frequency?: string;
	interval?: number;
	daysOfWeek?: string[];
	dayOfFrequency?: number;
	month?: number;
	time?: string;
	durationMin?: number;
	reminderMin?: number;
	dtstart?: string;
	dtend?: string;
}

interface Req {
	title?: string;
	description?: string;
	routineCategory?: string;
	isRecurring?: boolean;
	recurrence?: RecurrenceReq;
	scheduledDate?: string;
	scheduledTime?: string;
	approximatelyTime?: number;
	priority?: string;
	unscheduled?: boolean;
	settings?: unknown[];
	scope?: string;
	occurrenceDate?: string;
	opId?: string;
	baseVersion?: number;
}

const str = (v: unknown): string | undefined =>
	typeof v === 'string' ? v : undefined;

const parseRecurrence = (raw: any): RecurrenceReq | undefined => {
	if (!raw || typeof raw !== 'object' || Array.isArray(raw)) return undefined;
	const num = (v: unknown) => numberOrNull(v) ?? undefined;
	const days = raw.days_of_week ?? raw.daysOfWeek;
	return {
		frequency: str(raw.frequency),
		interval: num(raw.interval),
		daysOfWeek: Array.isArray(days) ? days : undefined,
		dayOfFrequency: num(raw.day_of_frequency ?? raw.dayOfFrequency),
		month: num(raw.month),
		time:
			resolveTimeInput(raw.time ?? raw.scheduled_time ?? raw.scheduledTime) ??
			undefined,
		durationMin: num(raw.duration_min ?? raw.durationMin),
		reminderMin: num(raw.reminder_min ?? raw.reminderMin),
		dtstart:
			normalizeDateField(raw.start_date ?? raw.start_at ?? raw.dtstart) ??
			undefined,
		dtend:
			normalizeDateField(raw.end_date ?? raw.end_at ?? raw.dtend) ?? undefined,
	};
};

const parseReq = (body: any): Req => ({
	title: str(body?.title),
	description: str(body?.description),
	routineCategory: str(body?.routine_category),
	isRecurring:
		typeof body?.is_recurring === 'boolean' ? body.is_recurring : undefined,
	recurrence: parseRecurrence(body?.recurrence),
	scheduledDate: normalizeDateField(body?.scheduled_date) ?? undefined,
	scheduledTime: resolveTimeInput(body?.scheduled_time) ?? undefined,
	approximatelyTime: numberOrNull(body?.approximately_time) ?? undefined,
	priority: body?.priority ? String(body.priority) : undefined,
	unscheduled: body?.unscheduled === true,
	settings: Array.isArray(body?.settings) ? body.settings : undefined,
	scope: str(body?.scope),
	occurrenceDate: normalizeDateField(body?.occurrence_date) ?? undefined,
	opId: str(body?.op_id),
	baseVersion: numberOrNull(body?.base_version) ?? undefined,
});

export const isRecurringPattern = (p: any): boolean =>
	!!p?.frequency &&
	String(p.frequency).trim() !== '' &&
	String(p.frequency).toUpperCase() !== 'NONE';

const day10 = (v: any): string | null => (v ? String(v).slice(0, 10) : null);

const t8 = (t: string | null | undefined): string | null =>
	t == null ? null : t.length === 5 ? `${t}:00` : t;

const normText = (v: string | null | undefined): string => v ?? '';

const normCat = (v: string | null | undefined): string =>
	v == null || v === '' ? 'none' : v;

const timeOf = (scheduledFor: string): string => scheduledFor.slice(11, 19);

interface SeriesCtx {
	routine: any;
	pattern: any;
	version: number;
	segments: any[];
	today: string;
	timeZone: string | undefined;
}

const loadSeries = async (
	ctx: LocalContext,
	routine: any,
	pattern: any,
): Promise<SeriesCtx> => {
	const segments = await ctx.db.select<any>(
		`SELECT * FROM routine_pattern_segments WHERE routine_id = ? ORDER BY valid_from, id`,
		[routine.id],
	);
	return {
		routine,
		pattern,
		version: Number(pattern.series_version ?? 0),
		segments,
		today: localDate(ctx.now(), ctx.timeZone?.()),
		timeZone: ctx.timeZone?.(),
	};
};

const segVersions = (sc: SeriesCtx): Version[] => sc.segments.map(fromSegment);

const liveOf = (sc: SeriesCtx): Version =>
	liveVersion(sc.routine, sc.pattern, segVersions(sc), false, sc.timeZone);

const anchorOf = (sc: SeriesCtx): string | null =>
	day10(sc.pattern.start_at) ?? createdOn(sc.routine, sc.timeZone);

const firstGoverned = (sc: SeriesCtx): string | null => {
	let first: string | null = null;
	for (const s of sc.segments)
		if (first == null || s.valid_from < first) first = s.valid_from;
	return first ?? anchorOf(sc);
};

const opSeen = async (
	ctx: LocalContext,
	routineId: number,
	opId: string | undefined,
): Promise<boolean> => {
	if (!opId) return false;
	const rows = await ctx.db.select<any>(
		`SELECT 1 AS x FROM routine_ops WHERE routine_id = ? AND op_id = ?`,
		[routineId, opId],
	);
	return rows.length > 0;
};

// ── detection ────────────────────────────────────────────────────────────────

interface Detected {
	title: boolean;
	description: boolean;
	duration: boolean;
	time: boolean;
	recurrence: boolean;
	end: boolean;
	other: boolean;
}

const normInterval = (v: number | null | undefined): number =>
	v == null ? 1 : Math.max(1, v);

const sameSet = (a: string[], b: string[]): boolean => {
	const sa = new Set(a);
	const sb = new Set(b);
	return sa.size === sb.size && [...sa].every((x) => sb.has(x));
};

const detect = (
	req: Req,
	base: Version,
	liveEnd: string | null,
	single: boolean,
): Detected => {
	const d: Detected = {
		title: req.title != null && req.title !== base.title,
		description:
			req.description != null &&
			normText(req.description) !== normText(base.description),
		other:
			req.routineCategory != null &&
			normCat(req.routineCategory) !== normCat(base.routineCategory),
		end: false,
		duration:
			req.approximatelyTime != null &&
			req.approximatelyTime !== base.durationMin,
		time: false,
		recurrence: req.isRecurring === false && base.recurring,
	};
	const topTime = single ? t8(req.scheduledTime) : null;
	d.time = topTime != null && topTime !== t8(base.scheduledTime);
	const r = req.recurrence;
	if (r) {
		d.recurrence ||=
			r.frequency != null &&
			r.frequency.trim().toUpperCase() !== (base.frequency ?? '').toUpperCase();
		d.recurrence ||=
			r.interval != null &&
			normInterval(r.interval) !== normInterval(base.interval);
		d.recurrence ||=
			r.daysOfWeek != null &&
			!sameSet(normalizeDays(r.daysOfWeek), normalizeDays(base.daysOfWeek));
		d.recurrence ||=
			r.dayOfFrequency != null && r.dayOfFrequency !== base.dayOfFrequency;
		d.recurrence ||= r.month != null && r.month !== base.month;
		d.time ||= r.time != null && t8(r.time) !== t8(base.scheduledTime);
		d.duration ||= r.durationMin != null && r.durationMin !== base.durationMin;
		d.other ||= r.reminderMin != null && r.reminderMin !== base.reminderMin;
		d.end = r.dtend != null && r.dtend !== liveEnd;
	}
	return d;
};

const isVersioned = (d: Detected): boolean =>
	d.title ||
	d.description ||
	d.duration ||
	d.time ||
	d.recurrence ||
	d.end ||
	d.other;

const occurrenceLevel = (d: Detected): boolean =>
	d.title || d.description || d.duration || d.time;

// ── statement builders ───────────────────────────────────────────────────────

const stmt = (sql: string, params: SqlValue[] = []): BatchStatement => ({
	sql,
	params,
});

const SEG_COLS = [
	'valid_from',
	'valid_to',
	'anchor_date',
	'is_recurring',
	'frequency',
	'interval',
	'day_of_frequency',
	'month',
	'days_of_week',
	'scheduled_time',
	'duration_min',
	'reminder_min',
	'title',
	'description',
	'routine_category',
];

const segParams = (s: any): SqlValue[] =>
	SEG_COLS.map((c) => (s[c] === undefined ? null : s[c]));

const segInsert = (routineId: number, s: any, now: string): BatchStatement =>
	stmt(
		`INSERT INTO routine_pattern_segments (routine_id, ${SEG_COLS.map(
			(c) => `"${c}"`,
		).join(', ')}, created_at) VALUES (?, ${SEG_COLS.map(() => '?').join(
			', ',
		)}, ?)`,
		[routineId, ...segParams(s), now],
	);

const segUpdate = (s: any): BatchStatement =>
	stmt(
		`UPDATE routine_pattern_segments SET ${SEG_COLS.map(
			(c) => `"${c}" = ?`,
		).join(', ')} WHERE id = ?`,
		[...segParams(s), s.id],
	);

const snapshotOf = (
	sc: SeriesCtx,
	from: string,
	to: string,
	recurring: boolean,
): any => ({
	valid_from: from,
	valid_to: to,
	anchor_date: anchorOf(sc),
	is_recurring: recurring ? 1 : 0,
	frequency: sc.pattern.frequency,
	interval: sc.pattern.interval,
	day_of_frequency: sc.pattern.day_of_frequency,
	month: sc.pattern.month,
	days_of_week: sc.pattern.days_of_week,
	scheduled_time: sc.pattern.scheduled_time,
	duration_min: sc.routine.approximately_time ?? sc.pattern.duration_min,
	reminder_min: sc.pattern.reminder_min,
	title: sc.routine.title,
	description: sc.routine.description,
	routine_category: sc.routine.routine_category,
});

const snapshotsOf = (sc: SeriesCtx, from: string, to: string): any[] => {
	if (from > to) return [];
	const liveEnd = day10(sc.pattern.end_at);
	const out: any[] = [];
	const recurringTo = liveEnd == null || liveEnd > to ? to : liveEnd;
	if (recurringTo >= from) out.push(snapshotOf(sc, from, recurringTo, true));
	if (liveEnd != null && liveEnd < to) {
		const next = addDays(liveEnd, 1);
		out.push(snapshotOf(sc, next > from ? next : from, to, false));
	}
	return out;
};

const deleteSegmentsFrom = (id: number, d: string) =>
	stmt(
		`DELETE FROM routine_pattern_segments WHERE routine_id = ? AND valid_from >= ?`,
		[id, d],
	);

const deleteSchedulerPendingFrom = (id: number, d: string) =>
	stmt(
		`DELETE FROM routine_instances WHERE routine_id = ? AND UPPER(status) = 'PENDING'
		 AND is_override = 0 AND moved_from IS NULL AND scheduled_for >= ?`,
		[id, `${d}T00:00:00`],
	);

const deleteAllPendingFrom = (id: number, d: string) =>
	stmt(
		`DELETE FROM routine_instances WHERE routine_id = ? AND UPPER(status) = 'PENDING'
		 AND COALESCE(moved_from, substr(scheduled_for, 1, 10)) >= ?`,
		[id, d],
	);

const bumpVersion = (
	id: number,
	opId: string | undefined,
	now: string,
): BatchStatement[] => [
	stmt(
		`UPDATE routine_patterns SET series_version = series_version + 1 WHERE routine_id = ?`,
		[id],
	),
	...(opId
		? [
				stmt(
					`INSERT OR IGNORE INTO routine_ops (routine_id, op_id, created_at) VALUES (?, ?, ?)`,
					[id, opId, now],
				),
			]
		: []),
];

const recordOp = (
	id: number,
	opId: string | undefined,
	now: string,
): BatchStatement[] =>
	opId
		? [
				stmt(
					`INSERT OR IGNORE INTO routine_ops (routine_id, op_id, created_at) VALUES (?, ?, ?)`,
					[id, opId, now],
				),
			]
		: [];

const splitAt = (sc: SeriesCtx, d: string, now: string): BatchStatement[] => {
	const id = Number(sc.routine.id);
	const dayBefore = addDays(d, -1);
	let frozenAfter: string | null = null;
	for (const s of sc.segments) {
		if (s.valid_from >= d) continue;
		const to =
			s.valid_to == null || s.valid_to > dayBefore ? dayBefore : s.valid_to;
		if (frozenAfter == null || to > frozenAfter) frozenAfter = to;
	}
	const snapshots =
		frozenAfter == null || frozenAfter < dayBefore
			? snapshotsOf(
					sc,
					frozenAfter == null
						? (firstGoverned(sc) ?? d)
						: addDays(frozenAfter, 1),
					dayBefore,
				)
			: [];
	return [
		deleteSegmentsFrom(id, d),
		stmt(
			`UPDATE routine_pattern_segments SET valid_to = ? WHERE routine_id = ?
			 AND valid_from < ? AND (valid_to IS NULL OR valid_to >= ?)`,
			[dayBefore, id, d, d],
		),
		...snapshots.map((s) => segInsert(id, s, now)),
	];
};

const overlaySegments = (
	sc: SeriesCtx,
	req: Req,
	d: string,
	det: Detected,
	now: string,
): BatchStatement[] => {
	const id = Number(sc.routine.id);
	const save: any[] = [];
	const overlay: any[] = [];
	for (const original of sc.segments) {
		const s = { ...original };
		if (s.valid_from < d) {
			if (s.valid_to == null || s.valid_to >= d) {
				const tail = { ...s, id: undefined, valid_from: d };
				s.valid_to = addDays(d, -1);
				save.push(s);
				overlay.push(tail);
			}
		} else {
			overlay.push(s);
		}
	}
	const r = req.recurrence;
	const duration = req.approximatelyTime ?? r?.durationMin;
	const time = r?.time;
	for (const s of overlay) {
		if (req.title != null) s.title = req.title;
		if (req.description != null) s.description = req.description;
		if (req.routineCategory != null) s.routine_category = req.routineCategory;
		if (duration != null) s.duration_min = duration;
		if (time != null) s.scheduled_time = time;
		if (r?.reminderMin != null) s.reminder_min = r.reminderMin;
		if (det.recurrence && r) {
			if (r.frequency != null) s.frequency = r.frequency.trim().toUpperCase();
			if (r.interval != null) s.interval = r.interval;
			if (r.daysOfWeek != null)
				s.days_of_week = toJson(normalizeDays(r.daysOfWeek));
			if (r.dayOfFrequency != null) s.day_of_frequency = r.dayOfFrequency;
			if (r.month != null) s.month = r.month;
		}
		if (det.recurrence) s.anchor_date = d;
		save.push(s);
	}
	return save.map((s) => (s.id ? segUpdate(s) : segInsert(id, s, now)));
};

const mergedPattern = (pattern: any, r: RecurrenceReq | undefined) => ({
	frequency: r?.frequency
		? r.frequency.trim().toUpperCase()
		: pattern.frequency,
	interval: r?.interval ?? pattern.interval,
	day_of_frequency: r?.dayOfFrequency ?? pattern.day_of_frequency,
	month: r?.month ?? pattern.month,
	days_of_week: r?.daysOfWeek
		? toJson(normalizeDays(r.daysOfWeek))
		: pattern.days_of_week,
	start_at: r?.dtstart ?? pattern.start_at,
	end_at: r?.dtend ?? pattern.end_at,
	scheduled_time: r?.time ?? pattern.scheduled_time,
	duration_min: r?.durationMin ?? pattern.duration_min,
	reminder_min: r?.reminderMin ?? pattern.reminder_min,
});

const patternUpdate = (
	id: number,
	merged: ReturnType<typeof mergedPattern>,
	now: string,
): BatchStatement =>
	stmt(
		`UPDATE routine_patterns SET frequency = ?, interval = ?, day_of_frequency = ?, month = ?,
		 days_of_week = ?, start_at = ?, end_at = ?, scheduled_time = ?, duration_min = ?,
		 reminder_min = ?, updated_at = ? WHERE routine_id = ?`,
		[
			merged.frequency,
			merged.interval,
			merged.day_of_frequency,
			merged.month,
			merged.days_of_week,
			merged.start_at,
			merged.end_at,
			merged.scheduled_time,
			merged.duration_min,
			merged.reminder_min,
			now,
			id,
		],
	);

const taskFieldStatements = (
	sc: SeriesCtx,
	req: Req,
	versioned: boolean,
	now: string,
): BatchStatement[] => {
	const sets: string[] = [];
	const params: SqlValue[] = [];
	const add = (col: string, v: SqlValue) => {
		sets.push(`${col} = ?`);
		params.push(v);
	};
	if (versioned) {
		if (req.title != null) add('title', req.title);
		if (req.description != null) add('description', req.description);
		if (req.routineCategory != null)
			add('routine_category', req.routineCategory);
		if (req.approximatelyTime != null)
			add('approximately_time', req.approximatelyTime);
	}
	if (req.priority) add('priority', req.priority);
	if (req.settings) add('settings', toJson(req.settings)!);
	if (req.unscheduled) {
		sets.push('scheduled_date = NULL', 'scheduled_time = NULL');
	}
	add('updated_at', now);
	return [
		stmt(`UPDATE routines SET ${sets.join(', ')} WHERE id = ?`, [
			...params,
			sc.routine.id,
		]),
	];
};

const upsertInstanceStatements = (
	id: number,
	date: string,
	time: string | null,
	now: string,
): BatchStatement[] => {
	const start = `${date}T00:00:00`;
	const end = `${addDays(date, 1)}T00:00:00`;
	const scheduledFor = `${date}T${time ?? '00:00:00'}`;
	return [
		stmt(
			`UPDATE routine_instances SET scheduled_for = ?, updated_at = ? WHERE id = (
				SELECT id FROM routine_instances WHERE routine_id = ? AND scheduled_for >= ? AND scheduled_for < ? ORDER BY id LIMIT 1)`,
			[scheduledFor, now, id, start, end],
		),
		stmt(
			`INSERT INTO routine_instances (routine_id, scheduled_for, status, created_at, updated_at)
			 SELECT ?, ?, 'PENDING', ?, ? WHERE NOT EXISTS (
				SELECT 1 FROM routine_instances WHERE routine_id = ? AND scheduled_for >= ? AND scheduled_for < ?)`,
			[id, scheduledFor, now, now, id, start, end],
		),
	];
};

const scheduleStatements = (
	sc: SeriesCtx,
	req: Req,
	schedule: boolean,
	now: string,
): BatchStatement[] => {
	const id = Number(sc.routine.id);
	if (req.unscheduled)
		return [
			stmt(
				`DELETE FROM routine_instances WHERE routine_id = ? AND UPPER(status) = 'PENDING'`,
				[id],
			),
		];
	if (schedule && req.scheduledDate)
		return upsertInstanceStatements(
			id,
			req.scheduledDate,
			req.scheduledTime ?? null,
			now,
		);
	return [];
};

const legacyPattern = (
	sc: SeriesCtx,
	req: Req,
	now: string,
): BatchStatement[] => {
	const id = Number(sc.routine.id);
	if (req.isRecurring === false)
		return [stmt(`DELETE FROM routine_patterns WHERE routine_id = ?`, [id])];
	if (req.recurrence)
		return [patternUpdate(id, mergedPattern(sc.pattern, req.recurrence), now)];
	return [];
};

const occurrenceRow = async (
	ctx: LocalContext,
	routineId: number,
	d: string,
): Promise<any | null> => {
	const [moved] = await ctx.db.select<any>(
		`SELECT * FROM routine_instances WHERE routine_id = ? AND moved_from = ? ORDER BY id LIMIT 1`,
		[routineId, d],
	);
	if (moved) return moved;
	const [row] = await ctx.db.select<any>(
		`SELECT * FROM routine_instances WHERE routine_id = ? AND moved_from IS NULL
		 AND scheduled_for >= ? AND scheduled_for < ? ORDER BY id LIMIT 1`,
		[routineId, `${d}T00:00:00`, `${addDays(d, 1)}T00:00:00`],
	);
	return row ?? null;
};

// ── scoped update ────────────────────────────────────────────────────────────

const conflict = (version: number) =>
	new LocalHttpError(
		409,
		`Routine was changed elsewhere; series_version is ${version}`,
	);

const run = async (
	ctx: LocalContext,
	statements: BatchStatement[],
	version: number,
	guarded: boolean,
	routineId: number,
	opId: string | undefined,
) => {
	const result = await ctx.db.batch(statements);
	if (result.failedAt == null || !guarded) return;
	if (await opSeen(ctx, routineId, opId)) return;
	throw conflict(version);
};

const guardStatement = (id: number, version: number): BatchStatement => ({
	sql: `UPDATE routine_patterns SET series_version = series_version WHERE routine_id = ? AND series_version = ?`,
	params: [id, version],
	expectChanges: true,
});

export const updateRoutineScoped = async (
	ctx: LocalContext,
	row: any,
	body: any,
) => {
	const req = parseReq(body);
	if (req.scope != null && !SCOPES.includes(req.scope))
		throw new LocalHttpError(422, `Unknown scope ${req.scope}`);
	if (req.scope === 'single' && !req.occurrenceDate)
		throw new LocalHttpError(
			422,
			'occurrence_date is required for scope=single',
		);
	const [pattern] = await ctx.db.select<any>(
		`SELECT * FROM routine_patterns WHERE routine_id = ?`,
		[row.id],
	);
	if (!isRecurringPattern(pattern)) return updateRoutine(ctx, row, body);
	if (await opSeen(ctx, row.id, req.opId)) return loadRoutineRow(ctx, row.id);

	const sc = await loadSeries(ctx, row, pattern);
	const id = Number(row.id);
	const now = iso(ctx);
	const scope = req.scope ?? 'following';
	const d = req.occurrenceDate ?? sc.today;
	const versions = segVersions(sc);
	const live = liveOf(sc);
	const gov = governing(versions, live, d);
	const base = scope === 'all' || !gov ? live : gov;
	const single = scope === 'single';
	const det = detect(req, base, live.validTo, single);

	if (single && (det.recurrence || det.end))
		throw new LocalHttpError(
			422,
			'The recurrence cannot be changed for a single occurrence',
		);
	const movesDate = req.scheduledDate != null && req.scheduledDate !== d;
	const versioned = single
		? occurrenceLevel(det) || movesDate
		: isVersioned(det);

	if (!versioned) {
		await run(
			ctx,
			[
				...taskFieldStatements(sc, req, false, now),
				...scheduleStatements(sc, req, !single, now),
			],
			sc.version,
			false,
			id,
			req.opId,
		);
		return loadRoutineRow(ctx, id);
	}
	if (req.baseVersion != null && req.baseVersion !== sc.version)
		throw conflict(sc.version);

	const statements: BatchStatement[] = [guardStatement(id, sc.version)];
	if (scope === 'all') {
		statements.push(
			...taskFieldStatements(sc, req, true, now),
			stmt(`DELETE FROM routine_pattern_segments WHERE routine_id = ?`, [id]),
			...legacyPattern(sc, req, now),
			...bumpVersion(id, req.opId, now),
			...scheduleStatements(sc, req, true, now),
		);
	} else if (single) {
		statements.push(
			...(await singleStatements(ctx, sc, req, d, base, now)),
			...bumpVersion(id, req.opId, now),
		);
	} else {
		statements.push(...followingStatements(sc, req, d, det, now));
	}
	await run(ctx, statements, sc.version, true, id, req.opId);
	return loadRoutineRow(ctx, id);
};

const followingStatements = (
	sc: SeriesCtx,
	req: Req,
	d: string,
	det: Detected,
	now: string,
): BatchStatement[] => {
	const id = Number(sc.routine.id);
	const frozen = frozenUntil(segVersions(sc));
	const overlay = req.isRecurring !== false && frozen != null && d <= frozen;
	const first = firstGoverned(sc);
	if (!overlay && (first == null || d <= first)) {
		return [
			...taskFieldStatements(sc, req, true, now),
			deleteSegmentsFrom(id, d),
			...legacyPattern(sc, req, now),
			...bumpVersion(id, req.opId, now),
			...scheduleStatements(sc, req, true, now),
		];
	}
	const out: BatchStatement[] = overlay
		? overlaySegments(sc, req, d, det, now)
		: splitAt(sc, d, now);
	if (req.isRecurring === false) {
		out.push(
			stmt(
				`UPDATE routine_patterns SET end_at = ?, updated_at = ? WHERE routine_id = ?`,
				[addDays(d, -1), now, id],
			),
			deleteSchedulerPendingFrom(id, d),
		);
	} else {
		const merged = mergedPattern(sc.pattern, req.recurrence);
		const oldStart = sc.pattern.start_at ?? null;
		if (det.recurrence) merged.start_at = d;
		else if (oldStart != null) merged.start_at = oldStart;
		out.push(patternUpdate(id, merged, now), deleteSchedulerPendingFrom(id, d));
	}
	out.push(
		...bumpVersion(id, req.opId, now),
		...taskFieldStatements(sc, req, true, now),
		...scheduleStatements(sc, req, true, now),
	);
	return out;
};

const singleStatements = async (
	ctx: LocalContext,
	sc: SeriesCtx,
	req: Req,
	d: string,
	base: Version,
	now: string,
): Promise<BatchStatement[]> => {
	const id = Number(sc.routine.id);
	const found = await occurrenceRow(ctx, id, d);
	const r = req.recurrence;
	const newTime = t8(r?.time ?? req.scheduledTime);
	const newDuration = req.approximatelyTime ?? r?.durationMin;
	const current = found ? String(found.scheduled_for).slice(0, 10) : d;
	const target = req.scheduledDate ?? current;
	const timeChanged = newTime != null && newTime !== t8(base.scheduledTime);
	const move = timeChanged || target !== current;

	let scheduledFor: string;
	let movedFrom: string | null = found?.moved_from ?? null;
	if (move) {
		const kept = found ? timeOf(found.scheduled_for) : '00:00:00';
		const time = timeChanged
			? newTime
			: kept !== '00:00:00'
				? kept
				: (t8(sc.pattern.scheduled_time) ?? '00:00:00');
		scheduledFor = `${target}T${time}`;
		if (movedFrom == null) movedFrom = d;
	} else {
		scheduledFor = found
			? String(found.scheduled_for)
			: `${d}T${t8(base.scheduledTime) ?? '00:00:00'}`;
	}
	let title: string | null = found?.title ?? null;
	let description: string | null = found?.description ?? null;
	let duration: number | null = found?.duration_min ?? null;
	if (req.title != null) title = req.title === base.title ? null : req.title;
	if (req.description != null)
		description =
			normText(req.description) === normText(base.description)
				? null
				: req.description;
	if (newDuration != null)
		duration = newDuration === base.durationMin ? null : newDuration;

	const out = taskFieldStatements(sc, req, false, now);
	if (found) {
		out.push(
			stmt(
				`UPDATE routine_instances SET scheduled_for = ?, moved_from = ?, title = ?, description = ?,
				 duration_min = ?, is_override = 1, updated_at = ? WHERE id = ?`,
				[scheduledFor, movedFrom, title, description, duration, now, found.id],
			),
		);
	} else {
		out.push(
			stmt(
				`INSERT INTO routine_instances (routine_id, scheduled_for, status, moved_from, title, description,
				 duration_min, is_override, created_at, updated_at) VALUES (?, ?, 'PENDING', ?, ?, ?, ?, 1, ?, ?)`,
				[id, scheduledFor, movedFrom, title, description, duration, now, now],
			),
		);
	}
	return out;
};

// ── scoped delete ────────────────────────────────────────────────────────────

const destroyReplayable = async (ctx: LocalContext, id: number) => {
	const gone = await ctx.db.select<any>(
		`SELECT 1 AS x FROM routines WHERE id = ? AND deleted_at IS NOT NULL`,
		[id],
	);
	if (gone.length) return;
	return destroyRoutine(ctx, id);
};

export const deleteRoutineScoped = async (
	ctx: LocalContext,
	id: number,
	scope: string | null,
	occurrenceDate: string | null,
	opId: string | null,
) => {
	if (scope == null || scope === '' || scope === 'all')
		return destroyRoutine(ctx, id);
	const occurrence = normalizeDateField(occurrenceDate);
	const op = opId ?? undefined;
	if (scope === 'single') {
		if (!occurrence)
			throw new LocalHttpError(
				422,
				'occurrence_date is required for scope=single',
			);
		const row = await loadRoutineRow(ctx, id);
		if (!row) return destroyReplayable(ctx, id);
		if (await opSeen(ctx, id, op)) return;
		const now = iso(ctx);
		const start = `${occurrence}T00:00:00`;
		const end = `${addDays(occurrence, 1)}T00:00:00`;
		await ctx.db.batch([
			stmt(
				`UPDATE routine_instances SET status = 'SKIPPED', skipped_at = ?, updated_at = ? WHERE id = (
					SELECT id FROM routine_instances WHERE routine_id = ? AND scheduled_for >= ? AND scheduled_for < ? ORDER BY id LIMIT 1)`,
				[now, now, id, start, end],
			),
			stmt(
				`INSERT INTO routine_instances (routine_id, scheduled_for, status, skipped_at, created_at, updated_at)
				 SELECT ?, ?, 'SKIPPED', ?, ?, ? WHERE NOT EXISTS (
					SELECT 1 FROM routine_instances WHERE routine_id = ? AND scheduled_for >= ? AND scheduled_for < ?)`,
				[id, start, now, now, now, id, start, end],
			),
			...recordOp(id, op, now),
		]);
		return;
	}
	if (scope !== 'following')
		throw new LocalHttpError(422, `Unknown scope ${scope}`);
	const row = await loadRoutineRow(ctx, id);
	if (!row) return destroyReplayable(ctx, id);
	const [pattern] = await ctx.db.select<any>(
		`SELECT * FROM routine_patterns WHERE routine_id = ?`,
		[id],
	);
	if (!isRecurringPattern(pattern)) return destroyReplayable(ctx, id);
	const sc = await loadSeries(ctx, row, pattern);
	const d = occurrence ?? sc.today;
	if (await opSeen(ctx, id, op)) return;
	const first = firstGoverned(sc);
	if (first == null || d <= first) return destroyReplayable(ctx, id);
	const now = iso(ctx);
	const result = await ctx.db.batch([
		guardStatement(id, sc.version),
		...splitAt(sc, d, now),
		stmt(
			`UPDATE routine_patterns SET end_at = ?, updated_at = ? WHERE routine_id = ?`,
			[addDays(d, -1), now, id],
		),
		deleteAllPendingFrom(id, d),
		...bumpVersion(id, op, now),
	]);
	if (result.failedAt != null && !(await opSeen(ctx, id, op)))
		throw conflict(sc.version);
};

// ── views ────────────────────────────────────────────────────────────────────

const timeParts = (t: string | null | undefined) =>
	t ? { hours: Number(t.slice(0, 2)), minutes: Number(t.slice(3, 5)) } : null;

export const recurrenceView = (p: any, routine: any) => {
	if (!isRecurringPattern(p)) return null;
	return {
		frequency: String(p.frequency).toUpperCase(),
		interval: p.interval ?? null,
		days_of_week: normalizeDays(parseJson<string[]>(p.days_of_week, [])),
		day_of_frequency: p.day_of_frequency ?? null,
		month: p.month ?? null,
		time: timeParts(p.scheduled_time),
		duration_min: routine.approximately_time ?? p.duration_min ?? null,
		reminder_min: p.reminder_min ?? null,
	};
};

const shortTime = (t: string): string | null => {
	if (t === '00:00:00') return null;
	return t.endsWith(':00') ? t.slice(0, 5) : t;
};

export const routineView = async (
	ctx: LocalContext,
	row: any,
	date: string | null,
) => {
	const [pattern] = await ctx.db.select<any>(
		`SELECT * FROM routine_patterns WHERE routine_id = ?`,
		[row.id],
	);
	const [{ n }] = await ctx.db.select<{ n: number }>(
		`SELECT COUNT(*) AS n FROM routine_comments WHERE routine_id = ? AND deleted_at IS NULL`,
		[row.id],
	);
	const view: Record<string, any> = {
		...routineJson(row, ctx),
		recurrence: pattern ? recurrenceView(pattern, row) : null,
		comments_count: Number(n),
		series_version: pattern ? Number(pattern.series_version ?? 0) : 0,
	};
	if (date == null) return view;
	if (pattern) {
		const sc = await loadSeries(ctx, row, pattern);
		const v = governing(segVersions(sc), liveOf(sc), date);
		if (v && v.segmentId != null) {
			view.title = v.title;
			view.description = v.description;
			view.routine_category = v.routineCategory;
			view.approximately_time = v.durationMin;
			view.recurrence = v.recurring
				? {
						frequency: v.frequency ? v.frequency.toUpperCase() : null,
						interval: v.interval,
						days_of_week: normalizeDays(v.daysOfWeek),
						day_of_frequency: v.dayOfFrequency,
						month: v.month,
						time: timeParts(v.scheduledTime),
						duration_min: v.durationMin,
						reminder_min: v.reminderMin,
					}
				: null;
		}
	}
	const found = await occurrenceRow(ctx, Number(row.id), date);
	if (found && Number(found.is_override) !== 0) {
		view.occurrence = {
			date: String(found.scheduled_for).slice(0, 10),
			title: found.title ?? null,
			description: found.description ?? null,
			duration_min: found.duration_min ?? null,
			time: shortTime(timeOf(found.scheduled_for)),
			status: found.status,
			moved_from: found.moved_from ?? null,
		};
	}
	return view;
};
