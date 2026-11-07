import { getActivityFeed } from '@/actions/tmgr/dashboard';
import type { Activity } from '@/types/dashboard';
import {
	dayRange,
	inRange,
	parseTimestamp,
	type DayRange,
	type DayTask,
} from './daySummary';
import { listLocalWorkspaces, localContext } from './runtime';
import type { LocalWorkspace } from './types';

type Metadata = Record<string, unknown>;

export interface CollectDayOptions {
	now: Date;
	localUserId: number;
	cloud: { id: number; name: string }[];
	cloudUserId: number | null;
}

const SNIPPETS = 3;
const FEED_PAGE = 100;
const FEED_MAX_PAGES = 5;
const CLOUD_TYPES = new Set([
	'task_created',
	'task_status_changed',
	'comment_created',
	'task_timer_stopped',
]);

const TASKS_SQL = `SELECT t.id, t.title, t.created_at, t.updated_at,
	s.name AS status, c.code AS category_code, t.category_tasks_sequence_id AS seq
	FROM tasks t
	LEFT JOIN statuses s ON s.id = t.status_id
	LEFT JOIN categories c ON c.id = t.project_category_id
	WHERE t.deleted_at IS NULL AND (
		(t.created_at >= ? AND t.created_at < ?)
		OR (t.updated_at >= ? AND t.updated_at < ?)
		OR t.id IN (SELECT task_id FROM comments WHERE deleted_at IS NULL AND created_at >= ? AND created_at < ?)
	)`;

const COMMENTS_SQL = `SELECT task_id, message, created_at, author_kind FROM comments
	WHERE deleted_at IS NULL AND created_at >= ? AND created_at < ?
	ORDER BY id`;

type LocalTaskRow = {
	id: number;
	title: string;
	created_at: string;
	updated_at: string;
	status: string | null;
	category_code: string | null;
	seq: number | null;
};

type LocalCommentRow = {
	task_id: number;
	message: string;
	created_at: string;
	author_kind: string | null;
};

const pad = (value: number) => String(value).padStart(2, '0');

const localDay = (date: Date) =>
	`${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;

const metadataOf = (raw: unknown): Metadata => {
	let value: unknown = raw;
	if (typeof raw === 'string') {
		try {
			value = JSON.parse(raw);
		} catch {
			value = null;
		}
	}
	return value && typeof value === 'object' ? (value as Metadata) : {};
};

const textOf = (value: unknown): string =>
	typeof value === 'string' ? value : '';

const collectLocal = async (
	workspace: LocalWorkspace,
	localUserId: number,
	range: DayRange,
): Promise<DayTask[]> => {
	const ctx = await localContext(workspace, {
		id: localUserId,
		name: '',
		email: '',
	});
	const from = range.from.toISOString();
	const to = range.to.toISOString();
	const comments = (
		await ctx.db.select<LocalCommentRow>(COMMENTS_SQL, [from, to])
	).filter(
		(c) => (c.author_kind ?? 'user') === 'user' && inRange(c.created_at, range),
	);
	const rows = await ctx.db.select<LocalTaskRow>(TASKS_SQL, [
		from,
		to,
		from,
		to,
		from,
		to,
	]);
	const notesByTask = new Map<number, LocalCommentRow[]>();
	for (const note of comments) {
		const list = notesByTask.get(note.task_id) ?? [];
		list.push(note);
		notesByTask.set(note.task_id, list);
	}
	return rows.flatMap((row): DayTask[] => {
		const createdToday = inRange(row.created_at, range);
		const touchedToday = inRange(row.updated_at, range);
		const notes = notesByTask.get(row.id) ?? [];
		if (!createdToday && !touchedToday && !notes.length) return [];
		return [
			{
				workspace: workspace.name,
				key:
					row.category_code && row.seq
						? `${row.category_code}-${row.seq}`
						: `#${row.id}`,
				title: row.title,
				status: row.status ?? '',
				createdToday,
				touchedToday,
				comments: notes.length,
				commentSnippets: notes
					.slice(0, SNIPPETS)
					.map((note) => note.message.trim()),
				source: 'local',
			},
		];
	});
};

const collectCloud = async (
	workspace: { id: number; name: string },
	userId: number,
	range: DayRange,
): Promise<DayTask[]> => {
	const yesterday = new Date(range.from);
	yesterday.setDate(yesterday.getDate() - 1);
	const activities: Activity[] = [];
	for (let page = 1; page <= FEED_MAX_PAGES; page++) {
		const result = await getActivityFeed(workspace.id, {
			page,
			per_page: FEED_PAGE,
			filters: {
				user_id: userId,
				date_from: localDay(yesterday),
				date_to: localDay(range.from),
			},
		});
		if (result.error || !result.data) {
			throw new Error(`activity feed failed for ${workspace.name}`);
		}
		const items = result.data.data;
		activities.push(...items);
		if (items.length < FEED_PAGE) break;
		const oldest = Math.min(...items.map((a) => parseTimestamp(a.created_at)));
		if (oldest < range.from.getTime()) break;
	}
	const tasks = new Map<number, DayTask>();
	for (const activity of activities) {
		const meta = metadataOf(activity.metadata);
		const taskId =
			activity.subject_type === 'comment'
				? Number(meta.task_id ?? NaN)
				: activity.subject_id;
		const actorId =
			activity.user?.id ??
			(activity as Activity & { user_id?: number }).user_id;
		if (
			!CLOUD_TYPES.has(activity.type) ||
			taskId == null ||
			!Number.isFinite(taskId) ||
			actorId !== userId ||
			!inRange(activity.created_at, range)
		)
			continue;
		let task = tasks.get(taskId);
		if (!task) {
			task = {
				workspace: workspace.name,
				key: `#${taskId}`,
				title: '',
				status: '',
				createdToday: false,
				touchedToday: true,
				comments: 0,
				commentSnippets: [],
				source: 'cloud',
			};
			tasks.set(taskId, task);
		}
		if (!task.title)
			task.title =
				activity.subject_type === 'comment'
					? textOf(meta.task_title)
					: activity.subject_name || textOf(meta.task_title);
		if (activity.type === 'task_created') task.createdToday = true;
		if (activity.type === 'task_status_changed' && !task.status)
			task.status = textOf(meta.new_status_name);
		if (activity.type === 'comment_created') {
			task.comments++;
			const snippet = textOf(meta.comment).trim();
			if (snippet && task.commentSnippets.length < SNIPPETS)
				task.commentSnippets.push(snippet);
		}
		if (activity.type === 'task_timer_stopped') {
			const seconds = Number(meta.time_added) || 0;
			if (seconds > 0)
				task.minutes = (task.minutes ?? 0) + Math.round(seconds / 60);
		}
	}
	return [...tasks.values()].map((task) => ({
		...task,
		title: task.title || task.key,
	}));
};

export const collectDay = async (
	opts: CollectDayOptions,
): Promise<{ tasks: DayTask[]; skipped: string[] }> => {
	const range = dayRange(opts.now);
	const skipped: string[] = [];
	const cloudUserId = opts.cloudUserId;
	const locals = (await listLocalWorkspaces()).filter((w) => !w.demo);
	const localTasks = await Promise.all(
		locals.map((workspace) =>
			collectLocal(workspace, opts.localUserId, range).catch(() => {
				skipped.push(workspace.name);
				return [] as DayTask[];
			}),
		),
	);
	const cloudTasks =
		cloudUserId === null
			? []
			: await Promise.all(
					opts.cloud.map((workspace) =>
						collectCloud(workspace, cloudUserId, range).catch(() => {
							skipped.push(workspace.name);
							return [] as DayTask[];
						}),
					),
			  );
	return {
		tasks: [...localTasks.flat(), ...cloudTasks.flat()],
		skipped,
	};
};
