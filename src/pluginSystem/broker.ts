import type { Permission, PluginManifest } from './manifest';
import type { Routine, RoutineEntry, RoutineInstance } from './routines';

export type PluginErrorCode =
	| 'UNKNOWN_METHOD'
	| 'INVALID_PARAMS'
	| 'PERMISSION_DENIED'
	| 'NOT_DECLARED'
	| 'WORKSPACE_CHANGED'
	| 'NOT_RUNNING'
	| 'RATE_LIMITED'
	| 'NOT_SUPPORTED'
	| 'HOST_ERROR'
	| 'page_conflict';

export class PluginError extends Error {
	constructor(
		readonly code: PluginErrorCode,
		message: string,
		/** For `page_conflict`: the page as it is now, handed to the plugin as `error.current`. */
		readonly data?: unknown,
	) {
		super(message);
	}
}

export interface TaskQuery {
	statusId: number | null;
	categoryId: number | null;
	search: string | null;
	page: number;
	perPage: number;
	updatedSince?: string;
	dueBefore?: string;
	dueAfter?: string;
	statusType?: string;
	priority?: string;
	sort?: 'due' | 'updated' | 'created';
	direction?: 'asc' | 'desc';
}

/** What the host lets plugins do with workspace data. The real one goes through the app's axios. */
export interface DataApi {
	listTasks(query: TaskQuery): Promise<unknown>;
	getTask(id: number): Promise<unknown>;
	createTask(fields: Record<string, unknown>): Promise<unknown>;
	updateTask(id: number, fields: Record<string, unknown>): Promise<unknown>;
	listStatuses(): Promise<unknown>;
	listCategories(): Promise<unknown>;
	createStatus(fields: Record<string, unknown>): Promise<unknown>;
	updateStatus(id: number, patch: Record<string, unknown>): Promise<unknown>;
	reorderStatuses(ids: number[]): Promise<unknown>;
	createCategory(fields: Record<string, unknown>): Promise<unknown>;
	updateCategory(id: number, patch: Record<string, unknown>): Promise<unknown>;
	startTimer(taskId: number): Promise<unknown>;
	stopTimer(taskId: number): Promise<unknown>;
	listComments(taskId: number): Promise<unknown>;
	addComment(taskId: number, text: string): Promise<unknown>;
	reactToComment(commentId: number, emoji: string): Promise<unknown>;
	listRelations(taskId: number): Promise<unknown>;
	relateTask(taskId: number, otherId: number, type: string): Promise<unknown>;
	unrelateTask(taskId: number, otherId: number, type: string): Promise<unknown>;
	storageGet(key: string): Promise<unknown>;
	storageSet(key: string, json: string): Promise<unknown>;
	storageDelete(key: string): Promise<unknown>;
	storageKeys(): Promise<unknown>;
	listAttachments(taskId: number): Promise<unknown>;
	readAttachment(fileId: number): Promise<unknown>;
	taskDataGet(taskId: number, key: string): Promise<unknown>;
	taskDataSet(taskId: number, key: string, json: string): Promise<unknown>;
	taskDataDelete(taskId: number, key: string): Promise<unknown>;
	taskDataGetMany(taskIds: number[], key: string): Promise<unknown>;
	pagesSearch(q: string, opts: { type?: string; limit?: number }): Promise<unknown>;
	pagesTree(): Promise<unknown>;
	pagesGet(idOrSlug: number | string): Promise<any>;
	pagesCreate(fields: Record<string, unknown>): Promise<unknown>;
	pagesUpdate(id: number, fields: Record<string, unknown>): Promise<unknown>;
	pagesAppend(id: number, fields: Record<string, unknown>): Promise<unknown>;
	pagesSetSection(
		id: number,
		sectionId: string,
		markdown: string,
		summary?: string,
		heading?: string,
	): Promise<unknown>;
	pageDataGet(pageId: number, key: string): Promise<unknown>;
	pageDataSet(pageId: number, key: string, json: string): Promise<unknown>;
	pageDataDelete(pageId: number, key: string): Promise<unknown>;
	pageDataGetMany(pageIds: number[], key: string): Promise<unknown>;
	listAgentWork(taskId: number): Promise<unknown>;
	startAgentWork(
		taskId: number,
		fields: {
			agent?: string;
			model: string | null;
			sessionId: string | null;
			branch: string | null;
		},
	): Promise<unknown>;
	updateAgentWork(runId: number, patch: Record<string, unknown>): Promise<unknown>;
	finishAgentWork(runId: number, patch: Record<string, unknown>): Promise<unknown>;
	listRoutines(from: string, to: string): Promise<RoutineEntry[]>;
	getRoutine(id: number): Promise<Routine>;
	listRoutineInstances(id: number): Promise<RoutineInstance[]>;
	createRoutine(fields: {
		title: string;
		description: string | null;
		date: string | null;
		time: string | null;
	}): Promise<Routine>;
	updateRoutine(id: number, patch: Record<string, unknown>): Promise<Routine>;
	completeRoutine(id: number, date: string): Promise<RoutineInstance>;
	skipRoutine(id: number, date: string): Promise<RoutineInstance>;
	convertRoutine(
		id: number,
		fields: { categoryId: number | null; statusId: number | null },
	): Promise<unknown>;
}

export interface PluginWorkspace {
	id: number;
	code: string;
	name: string;
	kind: 'local' | 'cloud';
	/** The creator of a shared workspace, who alone turns plugins on there. */
	ownerId?: number;
}

export type RegistrationKind =
	| 'event'
	| 'command'
	| 'badges'
	| 'page'
	| 'section';

export interface StatusBarItem {
	text: string;
	tooltip: string | null;
	command: string | null;
}

export interface TrayItemSpec {
	title: string;
	items: {
		title: string;
		taskId: number | null;
		command: string | null;
		args: unknown;
	}[];
}

export type ViewBadgeTone = 'default' | 'info' | 'warning' | 'danger';

export interface ViewBadgeSpec {
	count: number | null;
	text: string | null;
	tone: ViewBadgeTone;
}

export interface NotifyPayload {
	message: string;
	title: string | null;
	taskId: number | null;
	command: string | null;
	args: unknown;
	urgency: 'normal' | 'high';
}

/** What a plugin asks for; the host turns it into an absolute time and persists it. */
export interface AlarmSpec {
	scheduledAtMs: number;
	periodMinutes: number | null;
}

export interface AlarmInfo {
	name: string;
	scheduledAt: string;
}

export interface BrokerDeps {
	manifest: PluginManifest;
	workspace: PluginWorkspace;
	currentWorkspaceId: () => number | null;
	api: DataApi;
	settings: () => Record<string, unknown>;
	notify: (payload: NotifyPayload) => void;
	setStatusBarItem: (id: string, item: StatusBarItem | null) => void;
	setViewBadge: (viewId: string, badge: ViewBadgeSpec | null) => void;
	refresh: (kind: 'badges' | 'page' | 'section', id: string) => void;
	register: (kind: RegistrationKind, id: string) => void;
	log: (level: 'info' | 'warn' | 'error', message: string) => void;
	now: () => number;
	/** Plain http to this computer only; the Rust side checks the address again. */
	fetch?: (request: FetchRequest) => Promise<FetchResponse>;
	/** Bound to this plugin: its own export folder, and a file picker titled with its name. */
	files?: {
		export: (path: string, content: string) => Promise<{ path: string }>;
		reveal: (path: string) => Promise<void>;
		pick: () => Promise<unknown>;
	};
	/** Bound to this plugin and workspace; the host owns scheduling and persistence. */
	alarms?: {
		create: (name: string, spec: AlarmSpec) => AlarmInfo;
		clear: (name: string) => void;
		list: () => AlarmInfo[];
	};
	dnd?: () => { active: boolean; until: string | null };
	/** Bound to this plugin; requires machine consent in shared workspaces, like fetch and files. */
	tray?: {
		setItem: (itemId: string, item: TrayItemSpec | null) => void;
		setTitle: (text: string | null) => void;
		isTitleOwner: () => boolean;
	};
	/** Only offered when the manifest declares `companion` and the workspace is local. */
	localAccess?: {
		requestConnection: (opts: {
			label?: string;
			permissions?: string[];
		}) => Promise<LocalConnectionResult>;
	};
}

export type LocalConnectionResult =
	| { status: 'connected'; tokenId: string; prefix: string }
	| { status: 'cancelled' };

export interface FetchRequest {
	url: string;
	method: string;
	headers: [string, string][];
	body: string | null;
}

export interface FetchResponse {
	status: number;
	headers: [string, string][];
	body: string;
}

/** `null` means the event needs no permission. */
export const PLUGIN_EVENTS: Record<string, Permission | null> = {
	'task.created': 'tasks:read',
	'task.updated': 'tasks:read',
	'task.deleted': 'tasks:read',
	'task.statusChanged': 'tasks:read',
	'timer.started': 'time:read',
	'timer.stopped': 'time:read',
	'comment.created': 'comments:read',
	'comment.updated': 'comments:read',
	'comment.deleted': 'comments:read',
	'comment.reactionChanged': 'comments:read',
	'task.relationChanged': 'relations:read',
	'routine.created': 'routines:read',
	'routine.updated': 'routines:read',
	'routine.deleted': 'routines:read',
	'page.created': 'pages:read',
	'page.updated': 'pages:read',
	'page.deleted': 'pages:read',
	'page.restored': 'pages:read',
	'page.moved': 'pages:read',
	alarm: 'alarms',
	'app.started': null,
	'workspace.switched': null,
};

const invalid = (message: string): never => {
	throw new PluginError('INVALID_PARAMS', message);
};

const id = (value: unknown, field = 'id'): number =>
	Number.isSafeInteger(value) && (value as number) > 0
		? (value as number)
		: invalid(`${field} must be a positive integer`);

const optionalId = (value: unknown, field: string): number | null =>
	value === undefined || value === null ? null : id(value, field);

const string = (
	value: unknown,
	field: string,
	max: number,
	allowEmpty = false,
): string =>
	typeof value === 'string' &&
	value.length <= max &&
	(allowEmpty || value.trim() !== '')
		? value
		: invalid(`${field} must be a string of at most ${max} characters`);

const TRAY_TEXT_MAX = 60;
const TRAY_RAW_MAX = 1000;

const graphemes = (value: string): string[] =>
	typeof Intl !== 'undefined' && 'Segmenter' in Intl
		? Array.from(
				new (Intl as any).Segmenter(undefined, { granularity: 'grapheme' }).segment(value),
				(part: any) => part.segment as string,
		  )
		: [...value];

/** At most `max` graphemes: a longer text keeps `max - 1` of them and a trailing "…". */
export const truncateGraphemes = (value: string, max = TRAY_TEXT_MAX): string => {
	const parts = graphemes(value);
	return parts.length <= max ? value : `${parts.slice(0, max - 1).join('')}…`;
};

const trayText = (value: unknown, field: string): string =>
	truncateGraphemes(string(value, field, TRAY_RAW_MAX));

const VIEW_BADGE_TONES: ViewBadgeTone[] = ['default', 'info', 'warning', 'danger'];
const VIEW_BADGE_TEXT_MAX = 4;

const viewBadge = (value: unknown): ViewBadgeSpec | null => {
	if (value === null) return null;
	if (!value || typeof value !== 'object' || Array.isArray(value))
		return invalid('badge must be an object or null');
	const b = value as Params;
	const hasCount = b.count !== undefined;
	const hasText = b.text !== undefined;
	if (hasCount === hasText) invalid('badge needs exactly one of count and text');
	if (b.tone !== undefined && !VIEW_BADGE_TONES.includes(b.tone as ViewBadgeTone))
		invalid(`tone must be one of ${VIEW_BADGE_TONES.join(', ')}`);
	const tone = (b.tone as ViewBadgeTone | undefined) ?? 'default';
	if (hasCount) {
		if (!Number.isSafeInteger(b.count) || (b.count as number) < 0)
			invalid('count must be a non-negative integer');
		return b.count === 0 ? null : { count: b.count as number, text: null, tone };
	}
	if (
		typeof b.text !== 'string' ||
		b.text.length === 0 ||
		b.text.length > VIEW_BADGE_TEXT_MAX
	)
		invalid(`text must be 1 to ${VIEW_BADGE_TEXT_MAX} characters`);
	return { count: null, text: b.text as string, tone };
};

const PRIORITIES = ['low', 'medium', 'high', 'urgent'];

const ISO_DATETIME = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d{1,9})?(Z|[+-]\d{2}:\d{2})$/;

/** Any offset is accepted, but stored/compared as UTC like Java's Instant ("…T12:00:00Z"). */
const isoDateTime = (value: unknown, field: string): string => {
	const ms = typeof value === 'string' && ISO_DATETIME.test(value) ? Date.parse(value) : NaN;
	if (Number.isNaN(ms)) invalid(`${field} must be an ISO 8601 date-time`);
	return new Date(ms).toISOString().replace(/\.000Z$/, 'Z');
};

const TASK_FIELDS: Record<string, (value: unknown) => unknown> = {
	title: (v) => string(v, 'title', 500),
	description: (v) =>
		v === null ? null : string(v, 'description', 20_000, true),
	status_id: (v) => id(v, 'status_id'),
	project_category_id: (v) => optionalId(v, 'project_category_id'),
	priority: (v) => (PRIORITIES.includes(v as string) ? v : invalid('priority')),
	approximately_time: (v) =>
		Number.isSafeInteger(v) && (v as number) >= 0
			? v
			: invalid('approximately_time must be seconds'),
	expired_at: (v) => (v === null ? null : isoDateTime(v, 'expired_at')),
};

const STATUS_TYPES = ['default', 'active', 'completed', 'hidden', 'archived'];

const statusType = (value: unknown): string =>
	STATUS_TYPES.includes(value as string)
		? (value as string)
		: invalid(`type must be one of ${STATUS_TYPES.join(', ')}`);

const HEX_COLOR = /^#[0-9a-fA-F]{6}$/;

const hexColor = (value: unknown): string =>
	typeof value === 'string' && HEX_COLOR.test(value)
		? value
		: invalid('color must be #rrggbb');

const CATEGORY_CODE = /^[A-Z][A-Z0-9]{0,9}$/;

const categoryCode = (value: unknown): string =>
	typeof value === 'string' && CATEGORY_CODE.test(value)
		? value
		: invalid('code must match ^[A-Z][A-Z0-9]{0,9}$');

const idArrayMax = (value: unknown, field: string, max: number): number[] => {
	if (!Array.isArray(value) || value.length === 0 || value.length > max)
		invalid(`${field} must be an array of up to ${max} ids`);
	return (value as unknown[]).map((v) => id(v, field));
};

const idArray = (value: unknown, field: string): number[] => idArrayMax(value, field, 100);

const stringArray = (value: unknown, field: string, max: number): string[] => {
	if (!Array.isArray(value) || value.length > max)
		invalid(`${field} must be an array of up to ${max} strings`);
	return (value as unknown[]).map((v) => string(v, field, 60));
};

const statusPatch = (patch: unknown): Record<string, unknown> => {
	if (!patch || typeof patch !== 'object' || Array.isArray(patch))
		invalid('patch must be an object');
	const p = patch as Params;
	const out: Record<string, unknown> = {};
	if (p.name != null) out.name = string(p.name, 'name', 100);
	if (p.type != null) out.type = statusType(p.type);
	if (p.color != null) out.color = hexColor(p.color);
	return out;
};

const categoryPatch = (patch: unknown): Record<string, unknown> => {
	if (!patch || typeof patch !== 'object' || Array.isArray(patch))
		invalid('patch must be an object');
	const p = patch as Params;
	const out: Record<string, unknown> = {};
	if (p.title != null) out.title = string(p.title, 'title', 100);
	if (p.code != null) out.code = categoryCode(p.code);
	return out;
};

const AGENT_NAME = /^[a-z0-9._-]{1,40}$/;

/** The plugin's own agent label; the host namespaces it under the plugin's identity before sending it. */
const agentName = (value: unknown): string =>
	typeof value === 'string' && AGENT_NAME.test(value)
		? value
		: invalid('agent must match ^[a-z0-9._-]{1,40}$');

const HTTPS_URL = /^https:\/\/.+/;

const httpsUrl = (value: unknown): string =>
	typeof value === 'string' && value.length <= 512 && HTTPS_URL.test(value)
		? value
		: invalid('prUrl must be an https url of at most 512 characters');

const COMMIT_SHA = /^[0-9a-f]{7,40}$/i;

const commitsField = (value: unknown): { sha: string; message: string | null }[] => {
	if (!Array.isArray(value) || value.length > 200)
		invalid('commits must be an array of at most 200 items');
	return (value as unknown[]).map((commit) => {
		if (!commit || typeof commit !== 'object') invalid('each commit must be an object');
		const sha = (commit as Params).sha;
		if (typeof sha !== 'string' || !COMMIT_SHA.test(sha))
			invalid('commit sha must be 7-40 hex characters');
		const message = (commit as Params).message;
		return {
			sha,
			message: message == null ? null : string(message, 'commit message', 500, true),
		};
	});
};

const nonNegativeInt = (value: unknown, field: string): number | null =>
	value == null
		? null
		: Number.isSafeInteger(value) && (value as number) >= 0
		  ? (value as number)
		  : invalid(`${field} must be a non-negative integer`);

const testsField = (
	value: unknown,
): { passed: number | null; failed: number | null; command: string | null } => {
	if (!value || typeof value !== 'object') invalid('tests must be an object');
	const v = value as Params;
	return {
		passed: nonNegativeInt(v.passed, 'passed'),
		failed: nonNegativeInt(v.failed, 'failed'),
		command: v.command == null ? null : string(v.command, 'command', 500, true),
	};
};

const FINISH_STATUSES = ['succeeded', 'failed', 'cancelled'];

const agentWorkProgress = (patch: unknown, includeBranch: boolean): Record<string, unknown> => {
	if (!patch || typeof patch !== 'object' || Array.isArray(patch))
		invalid('patch must be an object');
	const p = patch as Params;
	const out: Record<string, unknown> = {};
	if (includeBranch && p.branch != null) out.branch = string(p.branch, 'branch', 255);
	if (p.summary != null) out.summary = string(p.summary, 'summary', 10_000, true);
	if (p.prUrl != null) out.prUrl = httpsUrl(p.prUrl);
	if (p.commits != null) out.commits = commitsField(p.commits);
	if (p.tests != null) out.tests = testsField(p.tests);
	return out;
};

const taskDataKey = (value: unknown): string => string(value, 'key', 200);

const PAGE_TYPES = ['plain', 'context', 'person', 'meeting'];
const MAX_PAGE_BYTES = 1_048_576;
const SECTION_ID = /^[A-Za-z0-9][A-Za-z0-9_-]{0,63}$/;

const pageRef = (value: unknown): number | string =>
	typeof value === 'string' ? string(value, 'idOrSlug', 200) : id(value, 'idOrSlug');

const pageBody = (value: unknown, field: string): string => {
	const text = string(value, field, MAX_PAGE_BYTES, true);
	return byteLength(text) <= MAX_PAGE_BYTES ? text : invalid(`${field} is larger than 1 MB`);
};

const pageProperties = (value: unknown): Record<string, unknown> => {
	if (!value || typeof value !== 'object' || Array.isArray(value))
		invalid('properties must be an object');
	return value as Record<string, unknown>;
};

const pageType = (value: unknown): string =>
	PAGE_TYPES.includes(value as string)
		? (value as string)
		: invalid(`type must be one of ${PAGE_TYPES.join(', ')}`);

const pageSummary = (value: unknown): string => string(value, 'summary', 255, true);

const pagePatch = (p: Params): Record<string, unknown> => {
	const out: Record<string, unknown> = {};
	if (p.title !== undefined) out.title = string(p.title, 'title', 255);
	if (p.body !== undefined) out.body = pageBody(p.body, 'body');
	if (p.properties !== undefined) out.properties = pageProperties(p.properties);
	if (p.summary != null) out.summary = pageSummary(p.summary);
	return out;
};

const RELATION_TYPES = [
	'blocks',
	'is blocked by',
	'relates to',
	'duplicates',
	'is duplicated by',
	'depends on',
	'is dependency of',
] as const;

const relationType = (value: unknown): string =>
	typeof value === 'string' &&
	(RELATION_TYPES as readonly string[]).includes(value)
		? value
		: invalid(`type must be one of ${RELATION_TYPES.join(', ')}`);

const ROUTINE_DATE = /^\d{4}-\d{2}-\d{2}$/;

const routineDate = (value: unknown, field: string): string => {
	if (typeof value !== 'string' || !ROUTINE_DATE.test(value)) invalid(`${field} must be YYYY-MM-DD`);
	const [y, m, d] = (value as string).split('-').map(Number);
	const parsed = new Date(Date.UTC(y, m - 1, d));
	if (parsed.getUTCFullYear() !== y || parsed.getUTCMonth() !== m - 1 || parsed.getUTCDate() !== d) {
		invalid(`${field} must be a real date`);
	}
	return value as string;
};

const ROUTINE_TIME = /^([01]\d|2[0-3]):[0-5]\d$/;

const routineTime = (value: unknown): string =>
	typeof value === 'string' && ROUTINE_TIME.test(value) ? value : invalid('time must be HH:mm');

const todayLocal = (nowMs: number): string => {
	const d = new Date(nowMs);
	return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
};

/** Local routine ids start above this; the local PUT/PATCH tasks/:id would edit a routine with one. */
const ROUTINE_ID_BASE = 1_000_000_000;

const routineIdRefused = (): never =>
	invalid('id is a routine; use tmgr.routines');

const taskFields = (patch: unknown) => {
	if (!patch || typeof patch !== 'object' || Array.isArray(patch))
		invalid('patch must be an object');
	const fields: Record<string, unknown> = {};
	for (const [key, value] of Object.entries(patch as Record<string, unknown>)) {
		if (Object.prototype.hasOwnProperty.call(TASK_FIELDS, key)) {
			fields[key] = TASK_FIELDS[key](value);
		}
	}
	return fields;
};

const MAX_VALUE_BYTES = 256 * 1024;
const MAX_TASK_DATA_VALUE_BYTES = 64 * 1024;
const byteLength = (value: string): number => new TextEncoder().encode(value).byteLength;
const MAX_EXPORT_BYTES = 5 * 1024 * 1024;
const EXPORT_SEGMENT = /^[\p{L}\p{N} ._()-]{1,100}$/u;
// Data and documents only: nothing Finder would run or follow on a double-click.
const EXPORT_EXTENSIONS = new Set([
	'md',
	'txt',
	'csv',
	'tsv',
	'json',
	'html',
	'xml',
	'yaml',
	'yml',
	'log',
	'ics',
]);

/** A relative path of at most 5 plain segments: no `..`, no absolute paths, no backslashes. */
const exportPath = (value: unknown): string => {
	const path = string(value, 'path', 300);
	const segments = path.split('/');
	if (
		segments.length > 5 ||
		segments.some(
			(segment) =>
				!EXPORT_SEGMENT.test(segment) || segment === '.' || segment === '..',
		)
	) {
		invalid('path must be relative, like "reports/week 39.md"');
	}
	const extension = segments[segments.length - 1]
		.split('.')
		.pop()!
		.toLowerCase();
	if (
		!segments[segments.length - 1].includes('.') ||
		!EXPORT_EXTENSIONS.has(extension)
	) {
		invalid(`exports may be ${[...EXPORT_EXTENSIONS].join(', ')} files`);
	}
	return path;
};

const TRAY_ITEM_LIMIT = 10;
const TRAY_TITLE_MAX = 12;
const CONTROL_CHARS = /[\u0000-\u001f\u007f\u2028\u2029]/;

const ALARM_NAME = /^[A-Za-z0-9._-]{1,60}$/;
const ONE_YEAR_MS = 365 * 24 * 60 * 60_000;

const minutesValue = (value: unknown, field: string): number => {
	if (
		typeof value !== 'number' ||
		!Number.isFinite(value) ||
		value < 1 ||
		value > ONE_YEAR_MS / 60_000
	)
		invalid(`${field} must be a number of minutes, from 1 to ${ONE_YEAR_MS / 60_000}`);
	return value as number;
};

/** Exactly one of delayMinutes, periodMinutes (with an optional delayMinutes), or when. */
const alarmSpec = (p: Params, now: number): AlarmSpec => {
	const hasDelay = p.delayMinutes !== undefined && p.delayMinutes !== null;
	const hasPeriod = p.periodMinutes !== undefined && p.periodMinutes !== null;
	const hasWhen = p.when !== undefined && p.when !== null;
	if (hasWhen) {
		if (hasDelay || hasPeriod)
			invalid('when cannot be combined with delayMinutes or periodMinutes');
		const t = Date.parse(string(p.when, 'when', 40));
		if (!Number.isFinite(t)) invalid('when must be an ISO date string');
		if (t <= now) invalid('when must be in the future');
		if (t - now > ONE_YEAR_MS) invalid('when must be at most a year ahead');
		return { scheduledAtMs: t, periodMinutes: null };
	}
	if (hasPeriod) {
		const period = minutesValue(p.periodMinutes, 'periodMinutes');
		const delay = hasDelay ? minutesValue(p.delayMinutes, 'delayMinutes') : period;
		return { scheduledAtMs: now + delay * 60_000, periodMinutes: period };
	}
	if (hasDelay) {
		const delay = minutesValue(p.delayMinutes, 'delayMinutes');
		return { scheduledAtMs: now + delay * 60_000, periodMinutes: null };
	}
	return invalid('provide delayMinutes, periodMinutes, or when');
};

type Params = Record<string, any>;
interface Method {
	permission?: Permission;
	write?: boolean;
	run: (params: Params) => unknown;
}

/** Tokens refill continuously; a burst up to `burst`, then `perSecond`. */
const bucket = (perSecond: number, burst: number, now: () => number) => {
	let tokens = burst;
	let last = now();
	return () => {
		const t = now();
		tokens = Math.min(burst, tokens + ((t - last) / 1000) * perSecond);
		last = t;
		if (tokens < 1) return false;
		tokens -= 1;
		return true;
	};
};

/**
 * The single gate between a plugin and the app. The plugin is identified by the broker instance the host
 * created for it, never by anything in the message. Every call: known method → declared permission →
 * validated params → same workspace → rate limit.
 */
export const createBroker = (deps: BrokerDeps) => {
	const { manifest, api } = deps;
	const granted = new Set<Permission>(manifest.permissions);
	const reads = bucket(50, 100, deps.now);
	const writes = bucket(10, 20, deps.now);
	let pendingConnection = false;

	/** `relationTypeWithTask` is a local implementation detail; only relations:read may see it. */
	const stripRelations = (task: unknown): unknown => {
		if (granted.has('relations:read') || !task || typeof task !== 'object') return task;
		const { relationTypeWithTask, ...rest } = task as Record<string, unknown>;
		return rest;
	};
	const declared = {
		command: new Set(manifest.contributes.commands.map((c) => c.id)),
		badges: new Set(manifest.contributes.boardCardBadges.map((b) => b.id)),
		page: new Set(manifest.contributes.views.map((v) => v.id)),
		section: new Set(manifest.contributes.taskPanelSections.map((s) => s.id)),
		statusBar: new Set(manifest.contributes.statusBarItems.map((s) => s.id)),
		trayItem: new Set(manifest.contributes.trayItems.map((t) => t.id)),
	};
	const mustDeclare = (set: Set<string>, value: string) => {
		if (!set.has(value))
			throw new PluginError(
				'NOT_DECLARED',
				`${value} is not declared in the manifest`,
			);
	};

	const needFiles = () => {
		if (!deps.files)
			throw new PluginError('HOST_ERROR', 'file access is not available');
		return deps.files;
	};

	const needAlarms = () => {
		if (!deps.alarms)
			throw new PluginError('HOST_ERROR', 'alarms are not available');
		return deps.alarms;
	};

	const requireLocalWorkspace = () => {
		if (deps.workspace.kind !== 'local') {
			throw new PluginError(
				'NOT_SUPPORTED',
				'routines are not available in shared workspaces yet',
			);
		}
	};

	const needTray = () => {
		if (!deps.tray) {
			throw new PluginError(
				'PERMISSION_DENIED',
				'the menu bar is not available: not allowed on this computer',
			);
		}
		return deps.tray;
	};

	const methods: Record<string, Method> = {
		'workspace.current': { run: () => ({ ...deps.workspace }) },
		'settings.get': { run: () => deps.settings() },
		'tasks.list': {
			permission: 'tasks:read',
			run: (p) => {
				const query: TaskQuery = {
					statusId: optionalId(p.statusId, 'statusId'),
					categoryId: optionalId(p.categoryId, 'categoryId'),
					search: p.search == null ? null : string(p.search, 'search', 200),
					page: p.page == null ? 1 : id(p.page, 'page'),
					perPage:
						p.perPage == null ? 50 : Math.min(100, id(p.perPage, 'perPage')),
				};
				if (p.updatedSince != null) query.updatedSince = isoDateTime(p.updatedSince, 'updatedSince');
				if (p.dueBefore != null) query.dueBefore = isoDateTime(p.dueBefore, 'dueBefore');
				if (p.dueAfter != null) query.dueAfter = isoDateTime(p.dueAfter, 'dueAfter');
				if (p.statusType != null) query.statusType = statusType(p.statusType);
				if (p.priority != null)
					query.priority = PRIORITIES.includes(p.priority) ? p.priority : invalid('priority');
				if (p.sort != null) {
					query.sort = ['due', 'updated', 'created'].includes(p.sort)
						? p.sort
						: invalid('sort must be one of due, updated, created');
				}
				if (p.direction != null) {
					query.direction =
						p.direction === 'asc' || p.direction === 'desc'
							? p.direction
							: invalid('direction must be asc or desc');
				}
				return api.listTasks(query).then((result) => {
					const r = result as { items?: unknown[] };
					return Array.isArray(r?.items)
						? { ...r, items: r.items.map(stripRelations) }
						: result;
				});
			},
		},
		'tasks.get': {
			permission: 'tasks:read',
			run: (p) => {
				const taskId = id(p.id);
				if (deps.workspace.kind === 'local' && taskId > ROUTINE_ID_BASE) routineIdRefused();
				return api.getTask(taskId).then(stripRelations);
			},
		},
		'tasks.create': {
			permission: 'tasks:write',
			write: true,
			run: (p) => {
				const fields = taskFields(p);
				if (!('title' in fields)) invalid('title is required');
				return api.createTask(fields).then(stripRelations);
			},
		},
		'tasks.update': {
			permission: 'tasks:write',
			write: true,
			run: (p) => {
				const taskId = id(p.id);
				if (deps.workspace.kind === 'local' && taskId > ROUTINE_ID_BASE) routineIdRefused();
				return api.updateTask(taskId, taskFields(p.patch)).then(stripRelations);
			},
		},
		'statuses.list': {
			permission: 'statuses:read',
			run: () => api.listStatuses(),
		},
		'categories.list': {
			permission: 'categories:read',
			run: () => api.listCategories(),
		},
		'statuses.create': {
			permission: 'statuses:write',
			write: true,
			run: (p) => {
				const fields: Record<string, unknown> = {
					name: string(p.name, 'name', 100),
					type: statusType(p.type),
				};
				if (p.color != null) fields.color = hexColor(p.color);
				return api.createStatus(fields);
			},
		},
		'statuses.update': {
			permission: 'statuses:write',
			write: true,
			run: (p) => api.updateStatus(id(p.id), statusPatch(p.patch)),
		},
		'statuses.reorder': {
			permission: 'statuses:write',
			write: true,
			run: (p) => api.reorderStatuses(idArray(p.ids, 'ids')),
		},
		'categories.create': {
			permission: 'categories:write',
			write: true,
			run: (p) => {
				const fields: Record<string, unknown> = { title: string(p.title, 'title', 100) };
				if (p.code != null) fields.code = categoryCode(p.code);
				return api.createCategory(fields);
			},
		},
		'categories.update': {
			permission: 'categories:write',
			write: true,
			run: (p) => api.updateCategory(id(p.id), categoryPatch(p.patch)),
		},
		'time.start': {
			permission: 'time:write',
			write: true,
			run: (p) => api.startTimer(id(p.taskId, 'taskId')).then(stripRelations),
		},
		'time.stop': {
			permission: 'time:write',
			write: true,
			run: (p) => api.stopTimer(id(p.taskId, 'taskId')).then(stripRelations),
		},
		'comments.list': {
			permission: 'comments:read',
			run: (p) => api.listComments(id(p.taskId, 'taskId')),
		},
		'comments.add': {
			permission: 'comments:write',
			write: true,
			run: (p) =>
				api.addComment(id(p.taskId, 'taskId'), string(p.text, 'text', 10_000)),
		},
		'comments.react': {
			permission: 'comments:write',
			write: true,
			run: (p) =>
				api.reactToComment(
					id(p.commentId, 'commentId'),
					string(p.emoji, 'emoji', 32),
				),
		},
		'tasks.relations': {
			permission: 'relations:read',
			run: (p) => api.listRelations(id(p.taskId, 'taskId')),
		},
		'tasks.relate': {
			permission: 'relations:write',
			write: true,
			run: (p) =>
				api.relateTask(
					id(p.taskId, 'taskId'),
					id(p.otherId, 'otherId'),
					relationType(p.type),
				),
		},
		'tasks.unrelate': {
			permission: 'relations:write',
			write: true,
			run: (p) =>
				api.unrelateTask(
					id(p.taskId, 'taskId'),
					id(p.otherId, 'otherId'),
					relationType(p.type),
				),
		},
		'taskData.get': {
			permission: 'tasks:read',
			run: async (p) => {
				const json = await api.taskDataGet(id(p.taskId, 'taskId'), taskDataKey(p.key));
				return typeof json === 'string' ? JSON.parse(json) : null;
			},
		},
		'taskData.set': {
			permission: 'tasks:read',
			write: true,
			run: (p) => {
				const json = JSON.stringify(p.value ?? null);
				if (byteLength(json) > MAX_TASK_DATA_VALUE_BYTES) invalid('value is larger than 64 KB');
				return api.taskDataSet(id(p.taskId, 'taskId'), taskDataKey(p.key), json);
			},
		},
		'taskData.delete': {
			permission: 'tasks:read',
			write: true,
			run: (p) => api.taskDataDelete(id(p.taskId, 'taskId'), taskDataKey(p.key)),
		},
		'taskData.getMany': {
			permission: 'tasks:read',
			run: async (p) => {
				const taskIds = idArrayMax(p.taskIds, 'taskIds', 500);
				const raw = (await api.taskDataGetMany(taskIds, taskDataKey(p.key))) as Record<
					string,
					string
				>;
				const result: Record<string, unknown> = {};
				for (const [taskId, json] of Object.entries(raw ?? {})) {
					result[taskId] = typeof json === 'string' ? JSON.parse(json) : null;
				}
				return result;
			},
		},
		'pages.search': {
			permission: 'pages:read',
			run: (p) => {
				const opts: { type?: string; limit?: number } = {};
				if (p.type != null) opts.type = pageType(p.type);
				if (p.limit != null) opts.limit = Math.min(50, id(p.limit, 'limit'));
				return api.pagesSearch(string(p.q, 'q', 200), opts);
			},
		},
		'pages.tree': {
			permission: 'pages:read',
			run: () => api.pagesTree(),
		},
		'pages.get': {
			permission: 'pages:read',
			run: (p) => api.pagesGet(pageRef(p.idOrSlug)),
		},
		'pages.create': {
			permission: 'pages:write',
			write: true,
			run: (p) => {
				const fields: Record<string, unknown> = { title: string(p.title, 'title', 255) };
				if (p.type != null) fields.type = pageType(p.type);
				if (p.parentId != null) fields.parent_id = id(p.parentId, 'parentId');
				if (p.body !== undefined) fields.body = pageBody(p.body, 'body');
				if (p.properties !== undefined) fields.properties = pageProperties(p.properties);
				return api.pagesCreate(fields);
			},
		},
		'pages.update': {
			permission: 'pages:write',
			write: true,
			run: (p) => {
				const pageId = id(p.id);
				const version = id(p.version, 'version');
				return api.pagesUpdate(pageId, { version, ...pagePatch(p) });
			},
		},
		'pages.append': {
			permission: 'pages:write',
			write: true,
			run: (p) => {
				const fields: Record<string, unknown> = {
					markdown: pageBody(p.markdown, 'markdown'),
				};
				if (p.heading != null) fields.heading = string(p.heading, 'heading', 255);
				if (p.createHeading != null) {
					if (typeof p.createHeading !== 'boolean') invalid('createHeading must be a boolean');
					fields.create_heading = p.createHeading;
				}
				if (p.summary != null) fields.summary = pageSummary(p.summary);
				return api.pagesAppend(id(p.id), fields);
			},
		},
		'pages.setSection': {
			permission: 'pages:sections',
			write: true,
			run: async (p) => {
				const pageId = id(p.id);
				const sectionId =
					typeof p.sectionId === 'string' && SECTION_ID.test(p.sectionId)
						? p.sectionId
						: invalid('sectionId must be a section id');
				const markdown = pageBody(p.markdown, 'markdown');
				const summary = p.summary == null ? undefined : pageSummary(p.summary);
				const heading = p.heading == null ? undefined : string(p.heading, 'heading', 200);
				const page = await api.pagesGet(pageId);
				const section = (page?.sections ?? []).find((s: { id: string }) => s.id === sectionId);
				if (section && section.owner !== `plugin:${manifest.id}`)
					throw new PluginError(
						'PERMISSION_DENIED',
						`section ${sectionId} is not owned by plugin:${manifest.id}`,
					);
				return api.pagesSetSection(pageId, sectionId, markdown, summary, heading);
			},
		},
		'pageData.get': {
			permission: 'pages:read',
			run: async (p) => {
				const json = await api.pageDataGet(id(p.pageId, 'pageId'), taskDataKey(p.key));
				return typeof json === 'string' ? JSON.parse(json) : null;
			},
		},
		'pageData.set': {
			permission: 'pages:read',
			write: true,
			run: (p) => {
				const json = JSON.stringify(p.value ?? null);
				if (byteLength(json) > MAX_TASK_DATA_VALUE_BYTES) invalid('value is larger than 64 KB');
				return api.pageDataSet(id(p.pageId, 'pageId'), taskDataKey(p.key), json);
			},
		},
		'pageData.delete': {
			permission: 'pages:read',
			write: true,
			run: (p) => api.pageDataDelete(id(p.pageId, 'pageId'), taskDataKey(p.key)),
		},
		'pageData.getMany': {
			permission: 'pages:read',
			run: async (p) => {
				const pageIds = idArrayMax(p.pageIds, 'pageIds', 500);
				const raw = (await api.pageDataGetMany(pageIds, taskDataKey(p.key))) as Record<
					string,
					string
				>;
				const result: Record<string, unknown> = {};
				for (const [pageId, json] of Object.entries(raw ?? {})) {
					result[pageId] = typeof json === 'string' ? JSON.parse(json) : null;
				}
				return result;
			},
		},
		'agentWork.list': {
			permission: 'agent_work:read',
			run: (p) => api.listAgentWork(id(p.taskId, 'taskId')),
		},
		'agentWork.start': {
			permission: 'agent_work:write',
			write: true,
			run: (p) => {
				const agent = p.agent == null ? undefined : agentName(p.agent);
				const namespace = `plugin:${manifest.id}${agent ? `/${agent}` : ''}`;
				if (namespace.length > 64) {
					invalid(
						`agent namespace "${namespace}" is longer than 64 characters; use a shorter agent label`,
					);
				}
				return api.startAgentWork(id(p.taskId, 'taskId'), {
					agent,
					model: p.model == null ? null : string(p.model, 'model', 128),
					sessionId: p.sessionId == null ? null : string(p.sessionId, 'sessionId', 191),
					branch: p.branch == null ? null : string(p.branch, 'branch', 255),
				});
			},
		},
		'agentWork.update': {
			permission: 'agent_work:write',
			write: true,
			run: (p) => api.updateAgentWork(id(p.runId, 'runId'), agentWorkProgress(p.patch ?? {}, true)),
		},
		'agentWork.finish': {
			permission: 'agent_work:write',
			write: true,
			run: (p) => {
				const patch = (p.patch ?? {}) as Params;
				if (!FINISH_STATUSES.includes(patch.status))
					invalid(`status must be one of ${FINISH_STATUSES.join(', ')}`);
				return api.finishAgentWork(id(p.runId, 'runId'), {
					status: patch.status,
					...agentWorkProgress(patch, false),
				});
			},
		},
		'routines.list': {
			permission: 'routines:read',
			run: (p) => {
				requireLocalWorkspace();
				const from = routineDate(p.from, 'from');
				const to = routineDate(p.to, 'to');
				if (from > to) invalid('from must not be after to');
				const spanDays =
					(Date.parse(`${to}T00:00:00Z`) - Date.parse(`${from}T00:00:00Z`)) / 86_400_000;
				if (spanDays > 92) invalid('from..to must span at most 92 days');
				return api.listRoutines(from, to);
			},
		},
		'routines.get': {
			permission: 'routines:read',
			run: (p) => {
				requireLocalWorkspace();
				return api.getRoutine(id(p.id));
			},
		},
		'routines.instances': {
			permission: 'routines:read',
			run: (p) => {
				requireLocalWorkspace();
				return api.listRoutineInstances(id(p.id));
			},
		},
		'routines.create': {
			permission: 'routines:write',
			write: true,
			run: (p) => {
				requireLocalWorkspace();
				const title = string(p.title, 'title', 500);
				const description =
					p.description == null ? null : string(p.description, 'description', 20_000, true);
				const date = p.date == null ? null : routineDate(p.date, 'date');
				if (p.time != null && date == null) invalid('time requires date');
				const time = p.time == null ? null : routineTime(p.time);
				return api.createRoutine({ title, description, date, time });
			},
		},
		'routines.update': {
			permission: 'routines:write',
			write: true,
			run: (p) => {
				requireLocalWorkspace();
				if (!p.patch || typeof p.patch !== 'object' || Array.isArray(p.patch))
					invalid('patch must be an object');
				const patch = p.patch as Params;
				const fields: Record<string, unknown> = {};
				if ('title' in patch) fields.title = string(patch.title, 'title', 500);
				if ('description' in patch) {
					fields.description =
						patch.description == null ? null : string(patch.description, 'description', 20_000, true);
				}
				if (!Object.keys(fields).length) invalid('patch must include title or description');
				return api.updateRoutine(id(p.id), fields);
			},
		},
		'routines.complete': {
			permission: 'routines:write',
			write: true,
			run: (p) => {
				requireLocalWorkspace();
				const date = p.date == null ? todayLocal(deps.now()) : routineDate(p.date, 'date');
				return api.completeRoutine(id(p.id), date);
			},
		},
		'routines.skip': {
			permission: 'routines:write',
			write: true,
			run: (p) => {
				requireLocalWorkspace();
				const date = p.date == null ? todayLocal(deps.now()) : routineDate(p.date, 'date');
				return api.skipRoutine(id(p.id), date);
			},
		},
		'routines.convertToTask': {
			permission: 'routines:write',
			write: true,
			run: (p) => {
				requireLocalWorkspace();
				if (!granted.has('tasks:write')) {
					throw new PluginError(
						'PERMISSION_DENIED',
						'routines.convertToTask needs tasks:write',
					);
				}
				const categoryId = optionalId(p.categoryId, 'categoryId');
				const statusId = optionalId(p.statusId, 'statusId');
				return api.convertRoutine(id(p.id), { categoryId, statusId });
			},
		},
		'storage.get': {
			run: async (p) => {
				const json = await api.storageGet(string(p.key, 'key', 200));
				return typeof json === 'string' ? JSON.parse(json) : null;
			},
		},
		'storage.keys': { run: () => api.storageKeys() },
		'storage.delete': {
			write: true,
			run: (p) => api.storageDelete(string(p.key, 'key', 200)),
		},
		'storage.set': {
			write: true,
			run: (p) => {
				const json = JSON.stringify(p.value ?? null);
				if (byteLength(json) > MAX_VALUE_BYTES)
					invalid('value is larger than 256 KB');
				return api.storageSet(string(p.key, 'key', 200), json);
			},
		},
		'net.fetch': {
			run: async (p) => {
				let url: URL;
				try {
					url = new URL(string(p.url, 'url', 2000));
				} catch {
					return invalid('url is not a valid URL');
				}
				if (!manifest.network.allowedOrigins.includes(url.origin)) {
					throw new PluginError(
						'PERMISSION_DENIED',
						`${url.origin} is not in network.allowedOrigins`,
					);
				}
				if (!deps.fetch) {
					throw new PluginError(
						'PERMISSION_DENIED',
						'network access is not available: not allowed on this computer',
					);
				}
				const method =
					p.method == null
						? 'GET'
						: string(p.method, 'method', 10).toUpperCase();
				if (!['GET', 'POST', 'PUT', 'PATCH', 'DELETE'].includes(method))
					invalid('method');
				const headers = Object.entries(
					p.headers &&
						typeof p.headers === 'object' &&
						!Array.isArray(p.headers)
						? p.headers
						: {},
				)
					.slice(0, 32)
					.map(
						([name, value]) =>
							[
								string(name, 'header name', 100),
								string(String(value), 'header', 4000, true),
							] as [string, string],
					);
				const body =
					p.body == null ? null : string(p.body, 'body', 1024 * 1024, true);
				return deps.fetch({ url: url.toString(), method, headers, body });
			},
		},
		'files.export': {
			permission: 'files:export',
			write: true,
			run: (p) => {
				const path = exportPath(p.path);
				const content = string(p.content, 'content', MAX_EXPORT_BYTES, true);
				return needFiles().export(path, content);
			},
		},
		'files.reveal': {
			permission: 'files:export',
			run: async (p) => {
				await needFiles().reveal(exportPath(p.path));
			},
		},
		'files.list': {
			permission: 'files:attachments',
			run: (p) => api.listAttachments(id(p.taskId, 'taskId')),
		},
		'files.read': {
			permission: 'files:attachments',
			run: (p) => api.readAttachment(id(p.fileId, 'fileId')),
		},
		'files.pick': {
			permission: 'files:pick',
			run: () => needFiles().pick(),
		},
		'ui.notify': {
			permission: 'notifications',
			run: (p) => {
				const message = string(p.message, 'message', 300);
				const title = p.title == null ? null : string(p.title, 'title', 80);
				const taskId = optionalId(p.taskId, 'taskId');
				const command =
					p.command == null ? null : string(p.command, 'command', 120);
				if (command) mustDeclare(declared.command, command);
				let args: unknown = null;
				if (p.args !== undefined && p.args !== null) {
					if (JSON.stringify(p.args).length > 4096)
						invalid('args must be at most 4 KB of JSON');
					args = p.args;
				}
				const urgency = p.urgency === 'high' ? 'high' : 'normal';
				deps.notify({ message, title, taskId, command, args, urgency });
			},
		},
		'ui.dnd': {
			run: () => deps.dnd?.() ?? { active: false, until: null },
		},
		'alarms.create': {
			permission: 'alarms',
			write: true,
			run: (p) => {
				const name = string(p.name, 'name', 60);
				if (!ALARM_NAME.test(name))
					invalid(
						'name must be 1-60 characters of letters, digits, ".", "_" or "-"',
					);
				return needAlarms().create(name, alarmSpec(p, deps.now()));
			},
		},
		'alarms.clear': {
			permission: 'alarms',
			write: true,
			run: (p) => needAlarms().clear(string(p.name, 'name', 60)),
		},
		'alarms.list': {
			permission: 'alarms',
			run: () => needAlarms().list(),
		},
		'localAccess.requestConnection': {
			run: async (p) => {
				if (!manifest.companion) {
					throw new PluginError(
						'PERMISSION_DENIED',
						'this plugin has no companion section in its manifest',
					);
				}
				if (deps.workspace.kind !== 'local') {
					throw new PluginError(
						'NOT_SUPPORTED',
						'local access is only available in a local workspace',
					);
				}
				if (!deps.localAccess) {
					throw new PluginError('HOST_ERROR', 'local access is not available');
				}
				if (pendingConnection) {
					throw new PluginError(
						'RATE_LIMITED',
						'a connection request is already pending',
					);
				}
				const label = p.label == null ? undefined : string(p.label, 'label', 60);
				const permissions =
					p.permissions == null ? undefined : stringArray(p.permissions, 'permissions', 20);
				pendingConnection = true;
				try {
					return await deps.localAccess.requestConnection({ label, permissions });
				} finally {
					pendingConnection = false;
				}
			},
		},
		'ui.setStatusBarItem': {
			run: (p) => {
				const itemId = string(p.id, 'id', 60);
				mustDeclare(declared.statusBar, itemId);
				if (p.text === null) return deps.setStatusBarItem(itemId, null);
				const command =
					p.command == null ? null : string(p.command, 'command', 120);
				if (command) mustDeclare(declared.command, command);
				return deps.setStatusBarItem(itemId, {
					text: string(p.text, 'text', 60),
					tooltip: p.tooltip == null ? null : string(p.tooltip, 'tooltip', 300),
					command,
				});
			},
		},
		'ui.setTrayItem': {
			permission: 'tray',
			run: (p) => {
				const itemId = string(p.id, 'id', 60);
				mustDeclare(declared.trayItem, itemId);
				const tray = needTray();
				if (p.items === null) return tray.setItem(itemId, null);
				const title = trayText(p.title, 'title');
				if (!Array.isArray(p.items) || p.items.length > TRAY_ITEM_LIMIT)
					invalid(`items must be a list of at most ${TRAY_ITEM_LIMIT}`);
				const items = (p.items as Params[]).map((raw) => {
					const itemTitle = trayText(raw?.title, 'items.title');
					const taskId = optionalId(raw?.taskId, 'items.taskId');
					const command =
						raw?.command == null ? null : string(raw.command, 'items.command', 120);
					if (command) mustDeclare(declared.command, command);
					let args: unknown = null;
					if (raw?.args !== undefined && raw?.args !== null) {
						if (JSON.stringify(raw.args).length > 4096)
							invalid('items.args must be at most 4 KB of JSON');
						args = raw.args;
					}
					return { title: itemTitle, taskId, command, args };
				});
				return tray.setItem(itemId, { title, items });
			},
		},
		'ui.setTrayTitle': {
			permission: 'tray',
			run: (p) => {
				const tray = needTray();
				if (!tray.isTitleOwner()) {
					throw new PluginError(
						'PERMISSION_DENIED',
						`${manifest.name} is not chosen for the menu bar text in Settings`,
					);
				}
				if (p.text === null) return tray.setTitle(null);
				const trimmed = string(p.text, 'text', 60).trim();
				if (!trimmed) invalid('text must not be empty');
				if (/[\r\n]/.test(trimmed)) invalid('text must be a single line');
				if (CONTROL_CHARS.test(trimmed))
					invalid('text must not contain control characters');
				if ([...trimmed].length > TRAY_TITLE_MAX)
					invalid(`text must be at most ${TRAY_TITLE_MAX} characters`);
				return tray.setTitle(trimmed);
			},
		},
		'ui.setViewBadge': {
			permission: 'views:badge',
			run: (p) => {
				if (typeof p.viewId !== 'string' || !declared.page.has(p.viewId))
					invalid('viewId must be a view declared by this plugin');
				deps.setViewBadge(p.viewId as string, viewBadge(p.badge));
			},
		},
		'ui.refresh': {
			run: (p) => {
				const kind = p.kind as 'badges' | 'page' | 'section';
				if (!['badges', 'page', 'section'].includes(kind)) invalid('kind');
				const target = string(p.id, 'id', 60);
				mustDeclare(declared[kind], target);
				return deps.refresh(kind, target);
			},
		},
		log: {
			run: (p) => {
				const level = ['info', 'warn', 'error'].includes(p.level)
					? p.level
					: 'info';
				deps.log(level, String(p.message ?? '').slice(0, 2000));
			},
		},
		register: {
			run: (p) => {
				const kind = p.kind as RegistrationKind;
				const target = string(p.id, 'id', 120);
				if (kind === 'event') {
					if (!Object.prototype.hasOwnProperty.call(PLUGIN_EVENTS, target))
						invalid(`unknown event ${target}`);
					const permission = PLUGIN_EVENTS[target];
					if (permission && !granted.has(permission)) {
						throw new PluginError(
							'PERMISSION_DENIED',
							`${target} needs ${permission}`,
						);
					}
				} else if (
					Object.prototype.hasOwnProperty.call(declared, kind) &&
					kind !== ('statusBar' as string)
				) {
					mustDeclare(declared[kind as keyof typeof declared], target);
					// Badges and sections are handed task snapshots, so they need read access to tasks.
					if (
						(kind === 'badges' || kind === 'section') &&
						!granted.has('tasks:read')
					) {
						throw new PluginError(
							'PERMISSION_DENIED',
							`${kind} needs tasks:read`,
						);
					}
				} else {
					invalid(`unknown registration ${kind}`);
				}
				deps.register(kind, target);
			},
		},
	};

	/** Same workspace and within the rate limit: every call, and anything else that acts for the plugin. */
	const admit = (write: boolean) => {
		if (deps.currentWorkspaceId() !== deps.workspace.id) {
			throw new PluginError(
				'WORKSPACE_CHANGED',
				'the app has left the plugin workspace',
			);
		}
		if (!(write ? writes() : reads())) {
			throw new PluginError(
				'RATE_LIMITED',
				`too many ${write ? 'writes' : 'calls'}`,
			);
		}
	};

	return {
		admit,
		async call(method: string, params: unknown): Promise<unknown> {
			const entry = Object.prototype.hasOwnProperty.call(methods, method)
				? methods[method]
				: undefined;
			if (!entry)
				throw new PluginError('UNKNOWN_METHOD', `unknown method ${method}`);
			if (entry.permission && !granted.has(entry.permission)) {
				throw new PluginError(
					'PERMISSION_DENIED',
					`${method} needs ${entry.permission}`,
				);
			}
			admit(!!entry.write);
			const p =
				params && typeof params === 'object' && !Array.isArray(params)
					? (params as Params)
					: {};
			try {
				return (await entry.run(p)) ?? null;
			} catch (error) {
				if (error instanceof PluginError) throw error;
				throw new PluginError(
					'HOST_ERROR',
					error instanceof Error ? error.message : String(error),
				);
			}
		},
	};
};

export type Broker = ReturnType<typeof createBroker>;
