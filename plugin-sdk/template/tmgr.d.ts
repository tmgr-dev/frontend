/**
 * Types for TMGR plugin authors (API 1.2). A plugin's main.js runs in a sandbox where `tmgr` and
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

type RoutineStatus = 'PENDING' | 'COMPLETED' | 'SKIPPED';

/** One occurrence of a routine in a date range, from `tmgr.routines.list`. Local workspaces only. */
interface RoutineEntry {
	routineId: number;
	/** null for a virtual occurrence (nothing completed/skipped yet) or an undated note. */
	instanceId: number | null;
	title: string;
	description: string | null;
	/** YYYY-MM-DD. An undated note (no scheduled date) always appears on today. */
	date: string;
	/** "HH:mm", or null. */
	time: string | null;
	status: RoutineStatus;
	completed: boolean;
	/** Whether this routine repeats (a frequency other than NONE). */
	recurring: boolean;
	/** DAILY | WEEKLY | MONTHLY | YEARLY | null. */
	frequency: string | null;
	/** True exactly when `instanceId` is null. */
	virtual: boolean;
}

/** One routine row. Recurrence is per occurrence in `list` (`recurring`, `frequency`), not here. */
interface Routine {
	id: number;
	title: string;
	description: string | null;
	scheduledDate: string | null;
	scheduledTime: string | null;
	createdAt: string;
	updatedAt: string;
}

interface RoutineInstance {
	id: number;
	routineId: number;
	date: string;
	time: string | null;
	status: RoutineStatus;
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
	  }
	/** Needs the alarms permission. */
	| { type: 'alarm'; name: string; scheduledAt: string }
	/** No permission needed. Delivered once, right after this plugin starts, if it registered a handler by then. */
	| { type: 'app.started' }
	/** No permission needed. Delivered when the host activates a different workspace and this plugin starts there. */
	| { type: 'workspace.switched'; from: number | null; to: number }
	/** Needs routines:read. Local workspaces only — never delivered in a shared workspace. */
	| { type: 'routine.created'; workspaceId: number; routineId: number; routine: Routine }
	| {
			type: 'routine.updated';
			workspaceId: number;
			routineId: number;
			routine?: Routine;
			instance?: RoutineInstance;
	  }
	/** `taskId` is set when the routine was deleted by being converted into a task. */
	| { type: 'routine.deleted'; workspaceId: number; routineId: number; taskId?: number };

type TmgrTone = 'default' | 'muted' | 'success' | 'warning' | 'danger';
type TmgrColor = 'gray' | 'green' | 'yellow' | 'red' | 'blue' | 'purple' | 'orange';

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
	/** confirm (≤200 chars) asks the user, naming your plugin, before the command runs. */
	| { type: 'button'; text: string; command: string; args?: unknown; confirm?: string }
	| { type: 'taskLink'; taskId: number; text: string }
	| { type: 'divider' }
	/** Monospace text with a copy-to-clipboard button. */
	| { type: 'copyable'; text: string; label?: string }
	/** Needs links:open, and the link's host must be in manifest links.allowedDomains; otherwise it renders as plain text. */
	| { type: 'link'; url: string; text?: string }
	/** Relative time, e.g. "5 min ago" / "in 2 h"; at is an ISO 8601 timestamp. */
	| { type: 'timeAgo'; at: string }
	/** Like timeAgo, but colored: red once past, yellow within 24 h. */
	| { type: 'dueTime'; at: string }
	/** A two-column definition list, up to 50 rows. */
	| { type: 'keyValue'; items: { key: string; value: TmgrNode }[] };

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
	/**
	 * The command id must be declared in contributes.commands and start with the plugin id. A command with
	 * `"deepLink": true` in its manifest entry can also be run from `tmgr://plugin/<id>/command/<local id>`
	 * links; needs the deeplinks permission, and the user confirms the first time per command and version.
	 */
	commands: { register(id: string, handler: (args: unknown) => unknown): Promise<void> };
	/** Needs the alarms permission. Host-scheduled: fires even if the plugin was not running when it was due, coalesced into one event. */
	alarms: {
		/** name is 1-60 characters of letters, digits, ".", "_" or "-". Delay/period are minutes, from 1 to 525600 (one year); `when` is at most a year ahead. */
		create(
			name: string,
			spec:
				| { delayMinutes: number }
				| { periodMinutes: number; delayMinutes?: number }
				| { when: string },
		): Promise<{ name: string; scheduledAt: string }>;
		clear(name: string): Promise<void>;
		list(): Promise<{ name: string; scheduledAt: string }[]>;
		/** At most 10 alarms per plugin per workspace. Persisted; cleared when the plugin is uninstalled. */
	};
	ui: {
		/**
		 * Called with the visible board cards; return one badge or an array (up to 5 kept, the rest dropped) by task id.
		 * `priority` orders several badges together (higher first, default 0). `key` (≤40, [a-z0-9_-]) lets a
		 * contributes.boardFilters entry target this particular badge.
		 */
		provideBadges(
			id: string,
			provider: (
				tasks: TmgrTask[],
			) =>
				| Record<
						number,
						| { text: string; color?: TmgrColor; tooltip?: string; priority?: number; key?: string }
						| { text: string; color?: TmgrColor; tooltip?: string; priority?: number; key?: string }[]
				  >
				| Promise<
						Record<
							number,
							| { text: string; color?: TmgrColor; tooltip?: string; priority?: number; key?: string }
							| { text: string; color?: TmgrColor; tooltip?: string; priority?: number; key?: string }[]
						>
				  >,
		): Promise<void>;
		providePage(id: string, render: (props: unknown) => TmgrNode | Promise<TmgrNode>): Promise<void>;
		provideTaskSection(id: string, render: (task: TmgrTask) => TmgrNode | Promise<TmgrNode>): Promise<void>;
		setStatusBarItem(id: string, item: { text: string; tooltip?: string; command?: string } | null): Promise<void>;
		/**
		 * Needs notifications. title is at most 80 characters, message at most 300, args at most 4 KB of JSON.
		 * The shown title always names this plugin: "<plugin name>: <title>", or just the plugin name without
		 * one. command must be declared in contributes.commands. At most 5 notifications per plugin per minute.
		 * Clicking opens the task (taskId) or runs command with args, only while this plugin still runs in
		 * that same run and workspace; the plugin is told nothing about the click besides the command running.
		 */
		notify(
			message: string,
			options?: {
				title?: string;
				taskId?: number;
				command?: string;
				args?: unknown;
				urgency?: 'normal' | 'high';
			},
		): Promise<void>;
		/** No permission needed, read-only. While active, this plugin's notifications are not shown. */
		dnd(): Promise<{ active: boolean; until: string | null }>;
		/**
		 * Needs tray. id must be declared in contributes.trayItems (at most 5 per plugin). title and each
		 * item's title are at most 60 characters; at most 10 items; each item's args at most 4 KB of JSON.
		 * Rust shows the section as a submenu after the app's own tray items; clicking an item opens the
		 * task or runs the command, under the same rules as ui.notify. null removes the section.
		 */
		setTrayItem(
			id: string,
			item: {
				title: string;
				items: { title: string; taskId?: number; command?: string; args?: unknown }[];
			} | null,
		): Promise<void>;
		/**
		 * Needs tray. Only the one plugin chosen in Settings -> Plugins for the menu bar text may call this;
		 * others get PERMISSION_DENIED. text is trimmed, must be at most 12 characters, a single line, and
		 * free of control characters. null clears it. Cleared automatically when this plugin stops.
		 */
		setTrayTitle(text: string | null): Promise<void>;
		/** Ask the host to draw badges, a page or a section again. */
		refresh(kind: 'badges' | 'page' | 'section', id: string): Promise<void>;
	};
	/**
	 * Daily routines and notes. Needs `engines.tmgr` `^1.2`. Local workspaces only: every call throws
	 * NOT_SUPPORTED in a shared (cloud) workspace.
	 */
	routines: {
		/** Needs routines:read. from/to are YYYY-MM-DD, from <= to, at most 92 days apart. */
		list(range: { from: string; to: string }): Promise<RoutineEntry[]>;
		/** Needs routines:read. */
		get(id: number): Promise<Routine>;
		/** Needs routines:read. */
		instances(id: number): Promise<RoutineInstance[]>;
		/**
		 * Needs routines:write. `time` is only accepted together with `date`. No `date` makes an undated
		 * note, which always shows up on today in `list`.
		 */
		create(fields: {
			title: string;
			description?: string | null;
			date?: string;
			time?: string | null;
		}): Promise<Routine>;
		/** Needs routines:write. At least one field. */
		update(id: number, patch: { title?: string; description?: string | null }): Promise<Routine>;
		/** Needs routines:write. Idempotent: completing an already-completed occurrence just returns it. `date` defaults to today. */
		complete(id: number, options?: { date?: string }): Promise<RoutineInstance>;
		/** Needs routines:write. Idempotent, like `complete`. */
		skip(id: number, options?: { date?: string }): Promise<RoutineInstance>;
		/**
		 * Needs routines:write AND tasks:write (missing tasks:write rejects with PERMISSION_DENIED).
		 * Creates a task from the routine (with a `key`, like `tasks.create`) and removes the routine.
		 */
		convertToTask(id: number, options?: { categoryId?: number; statusId?: number }): Promise<TmgrTask>;
	};
	/**
	 * Only available when the manifest declares a `companion` section. Asks the workspace owner to
	 * connect an external program you install yourself: it opens a dialog where they pick or create a
	 * persona and issue it a token. The token itself is never given to the plugin, only its id and
	 * prefix. Rejects with NOT_SUPPORTED in a shared (cloud) workspace, PERMISSION_DENIED without a
	 * `companion` manifest section, and RATE_LIMITED while a request from this plugin is already pending.
	 */
	localAccess: {
		requestConnection(opts?: { label?: string; permissions?: string[] }): Promise<
			{ status: 'connected'; tokenId: string; prefix: string } | { status: 'cancelled' }
		>;
	};
};
