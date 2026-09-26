/**
 * Types for TMGR plugin authors (API 1.0). A plugin's main.js runs in a sandbox where `tmgr` and
 * `console` are the only globals: no DOM, no fetch, no timers. Every call returns a Promise and may
 * reject with an Error whose `name` is one of PluginErrorCode.
 */

type PluginErrorCode =
	| 'UNKNOWN_METHOD'
	| 'INVALID_PARAMS'
	| 'PERMISSION_DENIED'
	| 'NOT_DECLARED'
	| 'WORKSPACE_CHANGED'
	| 'RATE_LIMITED'
	| 'HOST_ERROR';

interface TmgrTask {
	id: number;
	title: string;
	description: string | null;
	status_id: number | null;
	project_category_id: number | null;
	category: { code: string | null; title?: string } | null;
	category_tasks_sequence_id: number | null;
	priority: string | null;
	/** Seconds tracked before the current run. */
	common_time: number;
	/** Estimate in seconds. */
	approximately_time: number;
	/** Unix seconds when the running timer started, 0 when stopped. */
	start_time: number;
	[field: string]: unknown;
}

interface TmgrTaskFields {
	title?: string;
	description?: string | null;
	status_id?: number;
	project_category_id?: number | null;
	priority?: 'low' | 'medium' | 'high' | 'urgent';
	approximately_time?: number;
}

type TmgrEvent =
	| { type: 'task.created' | 'task.updated'; workspaceId: number; taskId: number; task: TmgrTask }
	| { type: 'task.deleted'; workspaceId: number; taskId: number }
	| { type: 'task.statusChanged'; workspaceId: number; taskId: number; statusId: number; task?: TmgrTask }
	| { type: 'timer.started' | 'timer.stopped'; workspaceId: number; taskId: number; task: TmgrTask }
	| { type: 'comment.created' | 'comment.updated'; workspaceId: number; taskId: number; comment: Record<string, unknown> }
	| { type: 'comment.deleted'; workspaceId: number; commentId: number };

type TmgrTone = 'default' | 'muted' | 'success' | 'warning' | 'danger';
type TmgrColor = 'gray' | 'green' | 'yellow' | 'red' | 'blue';

/** The only things a plugin can draw. Unknown nodes are dropped by the host. */
type TmgrNode =
	| string
	| number
	| { type: 'stack'; direction?: 'row' | 'column'; children: TmgrNode[] }
	| { type: 'heading'; text: string; level?: 1 | 2 | 3 }
	| { type: 'text'; text: string; tone?: TmgrTone }
	| { type: 'badge'; text: string; color?: TmgrColor }
	| { type: 'stat'; label: string; value: string | number; tone?: TmgrTone }
	| { type: 'progress'; value: number; color?: TmgrColor }
	| { type: 'list'; items: TmgrNode[] }
	| {
			type: 'table';
			columns: { key: string; title: string }[];
			rows: { taskId?: number; cells: Record<string, TmgrNode> }[];
	  }
	| { type: 'button'; text: string; command: string; args?: unknown }
	| { type: 'taskLink'; taskId: number; text: string }
	| { type: 'divider' };

declare const tmgr: {
	workspace: { current(): Promise<{ id: number; code: string; name: string; kind: 'local' | 'cloud' }> };
	settings: { get(): Promise<Record<string, unknown>> };
	tasks: {
		/** Needs tasks:read. perPage is at most 100. */
		list(query?: { statusId?: number; categoryId?: number; search?: string; page?: number; perPage?: number }): Promise<{
			items: TmgrTask[];
			total: number;
		}>;
		get(id: number): Promise<TmgrTask>;
		/** Needs tasks:write. */
		create(fields: TmgrTaskFields & { title: string }): Promise<TmgrTask>;
		update(id: number, patch: TmgrTaskFields): Promise<TmgrTask>;
	};
	/** Needs statuses:read. */
	statuses: { list(): Promise<{ id: number; name: string; type: string }[]> };
	/** Needs categories:read. */
	categories: { list(): Promise<{ id: number; title: string; code: string | null }[]> };
	/** Needs time:write. */
	time: { start(taskId: number): Promise<TmgrTask>; stop(taskId: number): Promise<TmgrTask> };
	comments: {
		/** Needs comments:read. */
		list(taskId: number): Promise<Record<string, unknown>[]>;
		/** Needs comments:write. */
		add(taskId: number, text: string): Promise<Record<string, unknown>>;
	};
	/** Per plugin and workspace, JSON values up to 256 KB, 5 MB in total. */
	storage: {
		get<T = unknown>(key: string): Promise<T | null>;
		set(key: string, value: unknown): Promise<void>;
		delete(key: string): Promise<void>;
		keys(): Promise<string[]>;
	};
	files: {
		/** files:export. Text into <workspace>/exports/plugins/<plugin id>/<path>; path is relative, up to 5 segments. */
		export(path: string, content: string): Promise<{ path: string }>;
		reveal(path: string): Promise<void>;
		/** files:attachments. Attachments of tasks in this workspace, up to 5 MB. */
		list(taskId: number): Promise<{ id: number; name: string; mimeType: string | null; size: number | null; createdAt: string }[]>;
		read(fileId: number): Promise<{ name: string; mimeType: string | null; size: number; base64: string; text: string | null }>;
		/** files:pick. The user picks a file in a system dialog; null when they cancel. */
		pick(): Promise<{ name: string; size: number; base64: string; text: string | null } | null>;
	};
	/** Only origins listed in manifest network.allowedOrigins, all on this computer (http://localhost:<port>). */
	net: {
		fetch(
			url: string,
			init?: { method?: 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE'; headers?: Record<string, string>; body?: string },
		): Promise<{ status: number; ok: boolean; headers: Record<string, string>; text(): Promise<string>; json<T = unknown>(): Promise<T> }>;
	};
	/** task.* needs tasks:read, timer.* needs time:read, comment.* needs comments:read. Own writes are not delivered. */
	events: { on<T extends TmgrEvent['type']>(type: T, handler: (event: Extract<TmgrEvent, { type: T }>) => unknown): Promise<void> };
	/** The command id must be declared in contributes.commands and start with the plugin id. */
	commands: { register(id: string, handler: (args: unknown) => unknown): Promise<void> };
	ui: {
		/** Called with the visible board cards; return badges by task id. */
		provideBadges(
			id: string,
			provider: (tasks: TmgrTask[]) => Record<number, { text: string; color?: TmgrColor; tooltip?: string }> | Promise<Record<number, { text: string; color?: TmgrColor; tooltip?: string }>>,
		): Promise<void>;
		providePage(id: string, render: (props: unknown) => TmgrNode | Promise<TmgrNode>): Promise<void>;
		provideTaskSection(id: string, render: (task: TmgrTask) => TmgrNode | Promise<TmgrNode>): Promise<void>;
		setStatusBarItem(id: string, item: { text: string; tooltip?: string; command?: string } | null): Promise<void>;
		/** Needs notifications. */
		notify(message: string): Promise<void>;
		/** Ask the host to draw badges, a page or a section again. */
		refresh(kind: 'badges' | 'page' | 'section', id: string): Promise<void>;
	};
};
