/**
 * Types for TMGR plugin authors (API 1.1). A plugin's main.js runs in a sandbox where `tmgr` and
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
	/** The call exists but this workspace cannot do it yet (e.g. a shared workspace on an older server). */
	| 'NOT_SUPPORTED'
	| 'HOST_ERROR';

interface TmgrTask {
	id: number;
	title: string;
	description: string | null;
	status_id: number | null;
	project_category_id: number | null;
	category: { code: string | null; title?: string } | null;
	category_tasks_sequence_id: number | null;
	/** "CODE-N" from the category's code and category_tasks_sequence_id, or null without a category. */
	key: string | null;
	priority: string | null;
	/** Seconds tracked before the current run. */
	common_time: number;
	/** Estimate in seconds. */
	approximately_time: number;
	/** Unix seconds when the running timer started, 0 when stopped. */
	start_time: number;
	/** ISO 8601 date-time, or null when no deadline is set. */
	expired_at: string | null;
	[field: string]: unknown;
}

interface TmgrTaskFields {
	title?: string;
	description?: string | null;
	status_id?: number;
	project_category_id?: number | null;
	priority?: 'low' | 'medium' | 'high' | 'urgent';
	approximately_time?: number;
	/** ISO 8601 date-time (e.g. "2026-10-01T12:00:00Z"), or null to clear it. */
	expired_at?: string | null;
}

interface TmgrStatusFields {
	name: string;
	type: 'default' | 'active' | 'completed' | 'hidden' | 'archived';
	color?: string;
}

interface TmgrStatusPatch {
	name?: string;
	type?: 'default' | 'active' | 'completed' | 'hidden' | 'archived';
	color?: string;
}

interface TmgrCategoryFields {
	title: string;
	code?: string;
}

interface TmgrCategoryPatch {
	title?: string;
	code?: string;
}

interface TmgrAgentWorkCommit {
	sha: string;
	message: string | null;
}

interface TmgrAgentWorkTests {
	passed: number | null;
	failed: number | null;
	command: string | null;
}

interface TmgrAgentWorkRun {
	id: number;
	task_id: number;
	workspace_id: number;
	user_id: number;
	agent: string;
	model: string | null;
	session_id: string | null;
	branch: string | null;
	status: 'running' | 'succeeded' | 'failed' | 'cancelled' | 'abandoned';
	started_at: string;
	ended_at: string | null;
	duration_seconds: number;
	summary: string | null;
	pr_url: string | null;
	commits: TmgrAgentWorkCommit[];
	tests: TmgrAgentWorkTests | null;
	version: number;
}

interface TmgrAgentWorkOverview {
	runs: TmgrAgentWorkRun[];
	totals: { agent_seconds: number; human_seconds: number; human_timer_running: boolean };
}

interface TmgrAgentWorkProgress {
	branch?: string;
	summary?: string;
	prUrl?: string;
	commits?: TmgrAgentWorkCommit[];
	tests?: TmgrAgentWorkTests;
}

/**
 * Who wrote a comment. More kinds may appear later: treat an unknown kind like any non-user author.
 * `owner` is the accountable person behind a non-user author, when known.
 */
interface TmgrCommentAuthor {
	kind: 'user' | 'plugin' | 'persona' | 'companion' | 'agent' | (string & {});
	id: string;
	name: string;
	owner?: { id: string; name: string };
}

interface TmgrReaction {
	emoji: string;
	count: number;
	reacted: boolean;
	users?: { id: number; name: string }[];
}

interface TmgrTaskRelation {
	taskId: number;
	otherTaskId: number;
	type: string;
}

type TmgrEvent =
	/** `changed` (task.updated only) names the fields the write's request body set. */
	| { type: 'task.created' | 'task.updated'; workspaceId: number; taskId: number; task: TmgrTask; changed?: string[] }
	| { type: 'task.deleted'; workspaceId: number; taskId: number }
	| { type: 'task.statusChanged'; workspaceId: number; taskId: number; statusId: number; task?: TmgrTask }
	| { type: 'timer.started' | 'timer.stopped'; workspaceId: number; taskId: number; task: TmgrTask }
	/** `author` may be missing in a shared workspace until the server names the writer (TM-296). */
	| { type: 'comment.created' | 'comment.updated'; workspaceId: number; taskId: number; comment: Record<string, unknown> & { author?: TmgrCommentAuthor | null } }
	| { type: 'comment.deleted'; workspaceId: number; commentId: number }
	/** No `reacted`/`users`: those are actor-relative. Read this plugin's own state via comments.list. */
	| { type: 'comment.reactionChanged'; workspaceId: number; commentId: number; taskId?: number; reactions: { emoji: string; count: number }[] }
	/** `relationType` is the fixed name (e.g. "blocks") when known, otherwise the numeric relation type id. */
	| {
			type: 'task.relationChanged';
			workspaceId: number;
			taskId: number;
			otherTaskId: number;
			relationType: string | number;
			change: 'added' | 'removed';
	  };

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
		/**
		 * Needs tasks:read. perPage is at most 100. `search` matches the title and description
		 * substring. `updatedSince`/`dueBefore`/`dueAfter` are ISO 8601 date-times, compared to
		 * `expired_at` including the time (a task with no due date never matches `dueBefore`/`dueAfter`).
		 * `sort` defaults to newest first; `direction` defaults to "asc". Any of `updatedSince`,
		 * `dueBefore`, `dueAfter`, `statusType`, `priority` or `sort` rejects with NOT_SUPPORTED in a
		 * shared workspace until the server supports it.
		 */
		list(query?: {
			statusId?: number;
			categoryId?: number;
			search?: string;
			page?: number;
			perPage?: number;
			updatedSince?: string;
			dueBefore?: string;
			dueAfter?: string;
			statusType?: 'default' | 'active' | 'completed' | 'hidden' | 'archived';
			priority?: 'low' | 'medium' | 'high' | 'urgent';
			sort?: 'due' | 'updated' | 'created';
			direction?: 'asc' | 'desc';
		}): Promise<{
			items: TmgrTask[];
			total: number;
		}>;
		get(id: number): Promise<TmgrTask>;
		/** Needs tasks:write. */
		create(fields: TmgrTaskFields & { title: string }): Promise<TmgrTask>;
		update(id: number, patch: TmgrTaskFields): Promise<TmgrTask>;
		/** Needs relations:read. */
		relations(taskId: number): Promise<TmgrTaskRelation[]>;
		/**
		 * Needs relations:write. `type` is one of: blocks, is blocked by, relates to, duplicates,
		 * is duplicated by, depends on, is dependency of.
		 */
		relate(taskId: number, otherId: number, type: string): Promise<unknown>;
		unrelate(taskId: number, otherId: number, type: string): Promise<unknown>;
	};
	statuses: {
		/** Needs statuses:read. */
		list(): Promise<{ id: number; name: string; type: string }[]>;
		/** Needs statuses:write. Cannot delete a status. */
		create(fields: TmgrStatusFields): Promise<unknown>;
		/** Needs statuses:write. `type` rejects with NOT_SUPPORTED in a shared workspace (the server ignores it there). */
		update(id: number, patch: TmgrStatusPatch): Promise<unknown>;
		/** Needs statuses:write. The full ordered list of status ids. */
		reorder(ids: number[]): Promise<unknown>;
	};
	categories: {
		/** Needs categories:read. */
		list(): Promise<{ id: number; title: string; code: string | null }[]>;
		/** Needs categories:write. Cannot delete a category. */
		create(fields: TmgrCategoryFields): Promise<unknown>;
		/** Needs categories:write. `code` rejects with NOT_SUPPORTED in a shared workspace (the server ignores it there). */
		update(id: number, patch: TmgrCategoryPatch): Promise<unknown>;
	};
	/** Needs time:write. */
	time: { start(taskId: number): Promise<TmgrTask>; stop(taskId: number): Promise<TmgrTask> };
	comments: {
		/**
		 * Needs comments:read. Each comment carries `author` (kind 'user' | 'plugin' | 'persona' | …),
		 * or `null` in a shared workspace when the server does not yet say who wrote it.
		 */
		list(taskId: number): Promise<(Record<string, unknown> & { author: TmgrCommentAuthor | null })[]>;
		/** Needs comments:write. Written with this plugin as the author; a body `author` field is ignored. */
		add(taskId: number, text: string): Promise<Record<string, unknown> & { author: TmgrCommentAuthor }>;
		/** Needs comments:write. Toggles the emoji reaction for this plugin; returns the comment's reactions. */
		react(commentId: number, emoji: string): Promise<{ reactions: TmgrReaction[]; taskId?: number }>;
	};
	/** Per plugin and workspace, JSON values up to 256 KB, 5 MB in total. */
	storage: {
		get<T = unknown>(key: string): Promise<T | null>;
		set(key: string, value: unknown): Promise<void>;
		delete(key: string): Promise<void>;
		keys(): Promise<string[]>;
	};
	/**
	 * Per-task JSON values up to 64 KB, sharing the 5 MB / 1000 key quota with `storage`. No
	 * permission needed beyond the task existing. Not available in shared workspaces yet.
	 */
	taskData: {
		get<T = unknown>(taskId: number, key: string): Promise<T | null>;
		set(taskId: number, key: string, value: unknown): Promise<void>;
		delete(taskId: number, key: string): Promise<void>;
		/** Up to 500 task ids in one call; meant for `ui.provideBadges` (one call per batch). */
		getMany<T = unknown>(taskIds: number[], key: string): Promise<Record<number, T | null>>;
	};
	agentWork: {
		/** Needs agent_work:read. Newest first, with agent time next to time tracked on the task timer. */
		list(taskId: number): Promise<TmgrAgentWorkOverview>;
		/**
		 * Needs agent_work:write. Opens a work run under this plugin's own identity: the stored agent
		 * is "plugin:<pluginId>/<agent>" ("plugin:<pluginId>" when `agent` is omitted), so a plugin can
		 * never claim to be a bare agent name. A still-running run of the same (namespaced) agent on
		 * the task is closed as abandoned; this never touches a run started by another plugin or by the
		 * human user.
		 */
		start(
			taskId: number,
			fields: { agent?: string; model?: string; sessionId?: string; branch?: string },
		): Promise<TmgrAgentWorkRun>;
		/**
		 * Needs agent_work:write. Only the run's own starting actor may update it, and only within its
		 * own agent namespace; 409 once it is finished.
		 */
		update(runId: number, patch: TmgrAgentWorkProgress): Promise<TmgrAgentWorkRun>;
		/** Needs agent_work:write. Closes the run; `branch` cannot be changed here. */
		finish(
			runId: number,
			patch: { status: 'succeeded' | 'failed' | 'cancelled' } & Omit<TmgrAgentWorkProgress, 'branch'>,
		): Promise<TmgrAgentWorkRun>;
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
