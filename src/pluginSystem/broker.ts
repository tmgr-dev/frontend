import type { Permission, PluginManifest } from './manifest';

export type PluginErrorCode =
	| 'UNKNOWN_METHOD'
	| 'INVALID_PARAMS'
	| 'PERMISSION_DENIED'
	| 'NOT_DECLARED'
	| 'WORKSPACE_CHANGED'
	| 'NOT_RUNNING'
	| 'RATE_LIMITED'
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
}

/** What the host lets plugins do with workspace data. The real one goes through the app's axios. */
export interface DataApi {
	listTasks(query: TaskQuery): Promise<unknown>;
	getTask(id: number): Promise<unknown>;
	createTask(fields: Record<string, unknown>): Promise<unknown>;
	updateTask(id: number, fields: Record<string, unknown>): Promise<unknown>;
	listStatuses(): Promise<unknown>;
	listCategories(): Promise<unknown>;
	startTimer(taskId: number): Promise<unknown>;
	stopTimer(taskId: number): Promise<unknown>;
	listComments(taskId: number): Promise<unknown>;
	addComment(taskId: number, text: string): Promise<unknown>;
	storageGet(key: string): Promise<unknown>;
	storageSet(key: string, json: string): Promise<unknown>;
	storageDelete(key: string): Promise<unknown>;
	storageKeys(): Promise<unknown>;
	listAttachments(taskId: number): Promise<unknown>;
	readAttachment(fileId: number): Promise<unknown>;
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
};

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
			run: (p) =>
				api.listTasks({
					statusId: optionalId(p.statusId, 'statusId'),
					categoryId: optionalId(p.categoryId, 'categoryId'),
					search: p.search == null ? null : string(p.search, 'search', 200),
					page: p.page == null ? 1 : id(p.page, 'page'),
					perPage:
						p.perPage == null ? 50 : Math.min(100, id(p.perPage, 'perPage')),
				}),
		},
		'tasks.get': {
			permission: 'tasks:read',
			run: (p) => api.getTask(id(p.id)),
		},
		'tasks.create': {
			permission: 'tasks:write',
			write: true,
			run: (p) => {
				const fields = taskFields(p);
				if (!('title' in fields)) invalid('title is required');
				return api.createTask(fields);
			},
		},
		'tasks.update': {
			permission: 'tasks:write',
			write: true,
			run: (p) => api.updateTask(id(p.id), taskFields(p.patch)),
		},
		'statuses.list': {
			permission: 'statuses:read',
			run: () => api.listStatuses(),
		},
		'categories.list': {
			permission: 'categories:read',
			run: () => api.listCategories(),
		},
		'time.start': {
			permission: 'time:write',
			write: true,
			run: (p) => api.startTimer(id(p.taskId, 'taskId')),
		},
		'time.stop': {
			permission: 'time:write',
			write: true,
			run: (p) => api.stopTimer(id(p.taskId, 'taskId')),
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
				if (json.length > MAX_VALUE_BYTES)
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
