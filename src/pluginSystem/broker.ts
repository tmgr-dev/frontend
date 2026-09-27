import type { Permission, PluginManifest } from './manifest';

export type PluginErrorCode =
	| 'UNKNOWN_METHOD'
	| 'INVALID_PARAMS'
	| 'PERMISSION_DENIED'
	| 'NOT_DECLARED'
	| 'WORKSPACE_CHANGED'
	| 'NOT_RUNNING'
	| 'RATE_LIMITED'
	| 'NOT_SUPPORTED'
	| 'HOST_ERROR';

export class PluginError extends Error {
	constructor(readonly code: PluginErrorCode, message: string) {
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

export interface BrokerDeps {
	manifest: PluginManifest;
	workspace: PluginWorkspace;
	currentWorkspaceId: () => number | null;
	api: DataApi;
	settings: () => Record<string, unknown>;
	notify: (message: string) => void;
	setStatusBarItem: (id: string, item: StatusBarItem | null) => void;
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
}

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

export const PLUGIN_EVENTS: Record<string, Permission> = {
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
			run: (p) => api.getTask(id(p.id)).then(stripRelations),
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
			run: (p) => api.updateTask(id(p.id), taskFields(p.patch)).then(stripRelations),
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
				deps.notify(string(p.message, 'message', 300));
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
					const permission = PLUGIN_EVENTS[target];
					if (!permission) invalid(`unknown event ${target}`);
					if (!granted.has(permission)) {
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
