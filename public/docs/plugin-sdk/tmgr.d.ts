/**
 * Types for TMGR plugin authors (API 1.8). A plugin's main.js runs in a sandbox where `tmgr` and
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
	| 'HOST_ERROR'
	/** `pages.update` with a stale `version`; the error's `current` is the page as it is now. */
	| 'page_conflict';

type TmgrPageType = 'plain' | 'context' | 'person' | 'meeting';

interface TmgrPageAuthor {
	kind: 'user' | 'persona' | 'plugin';
	id: string | number | null;
	name: string | null;
}

interface TmgrPageSummary {
	id: number;
	title: string;
	slug: string;
	type: TmgrPageType;
	parent_id: number | null;
	position: number;
	pinned: boolean;
	updated_at: string;
}

/** `owner` is `system`, `agents` (any persona), `persona:<uuid>`, `plugin:<id>` or `user:<id>`. */
interface TmgrPageSection {
	id: string;
	owner: string;
	heading: string | null;
}

interface TmgrPage extends TmgrPageSummary {
	workspace_id: number;
	/** Markdown, up to 1 MB. Managed sections sit between `<!-- tmgr:section id="x" owner="y" -->` markers. */
	body: string;
	properties: Record<string, unknown>;
	/** Grows by one on every write; `pages.update` needs the version you read. */
	version: number;
	author: TmgrPageAuthor;
	updated_by: TmgrPageAuthor;
	created_at: string;
	backlinks: TmgrPageSummary[];
	sections: TmgrPageSection[];
}

interface TmgrPageSearchHit {
	id: number;
	slug: string;
	title: string;
	type: TmgrPageType;
	snippet: string;
	updated_at: string;
}

interface TmgrPageConflictError extends Error {
	name: 'page_conflict';
	current: TmgrPage;
}

interface TmgrTask {
	id: number;
	title: string;
	description: string | null;
	status_id: number | null;
	project_category_id: number | null;
	category: { code: string | null; title?: string } | null;
	category_tasks_sequence_id: number | null;
	/** The key the title is stamped with ("TM-242: …" for any code, "TM-242 …" or "MOBILE-APP-24 …" only in the task's own category), else "CODE-N" from the category's code and category_tasks_sequence_id, else null. */
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

/**
 * API 1.6: what a command listed in `contributes.menus["task/card"]` receives as `args` when the user picks
 * it in a task's "…" menu. `workspaceId` is the workspace the task is open in (null when unknown).
 */
interface TaskMenuCommandArgs {
	taskId: number;
	workspaceId: number | null;
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
	totals: {
		agent_seconds: number;
		human_seconds: number;
		human_timer_running: boolean;
	};
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
	| {
			type: 'task.created' | 'task.updated';
			workspaceId: number;
			taskId: number;
			task: TmgrTask;
			changed?: string[];
	  }
	| { type: 'task.deleted'; workspaceId: number; taskId: number }
	| {
			type: 'task.statusChanged';
			workspaceId: number;
			taskId: number;
			statusId: number;
			task?: TmgrTask;
	  }
	| {
			type: 'timer.started' | 'timer.stopped';
			workspaceId: number;
			taskId: number;
			task: TmgrTask;
	  }
	/** `author` may be missing in a shared workspace until the server names the writer (TM-296). */
	| {
			type: 'comment.created' | 'comment.updated';
			workspaceId: number;
			taskId: number;
			comment: Record<string, unknown> & { author?: TmgrCommentAuthor | null };
	  }
	| { type: 'comment.deleted'; workspaceId: number; commentId: number }
	/** No `reacted`/`users`: those are actor-relative. Read this plugin's own state via comments.list. */
	| {
			type: 'comment.reactionChanged';
			workspaceId: number;
			commentId: number;
			taskId?: number;
			reactions: { emoji: string; count: number }[];
	  }
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
	/**
	 * API 1.5, needs pages:read. `changedSections` lists the managed sections whose text changed in this write;
	 * it is empty for events from shared workspaces, which carry no body. Your own writes are not delivered back.
	 */
	| {
			type: 'page.created' | 'page.updated' | 'page.restored' | 'page.moved';
			workspaceId: number;
			pageId: number;
			slug: string | null;
			title: string | null;
			parentId: number | null;
			version: number | null;
			author: {
				kind: 'user' | 'persona' | 'plugin';
				id: string | number | null;
			} | null;
			changedSections: string[];
	  }
	| { type: 'page.deleted'; workspaceId: number; pageId: number }
	/** Needs routines:read. Local workspaces only — never delivered in a shared workspace. */
	| {
			type: 'routine.created';
			workspaceId: number;
			routineId: number;
			routine: Routine;
	  }
	| {
			type: 'routine.updated';
			workspaceId: number;
			routineId: number;
			routine?: Routine;
			instance?: RoutineInstance;
	  }
	/** `taskId` is set when the routine was deleted by being converted into a task. */
	| {
			type: 'routine.deleted';
			workspaceId: number;
			routineId: number;
			taskId?: number;
	  };

/** API 1.8. The closed set of icon names; any other name is dropped (the text of a button or badge stays). */
type TmgrIconName =
	| 'check'
	| 'x'
	| 'plus'
	| 'minus'
	| 'alert-triangle'
	| 'info'
	| 'circle-help'
	| 'clock'
	| 'calendar'
	| 'user'
	| 'users'
	| 'star'
	| 'heart'
	| 'flag'
	| 'tag'
	| 'link'
	| 'external-link'
	| 'copy'
	| 'trash'
	| 'edit'
	| 'pencil'
	| 'search'
	| 'filter'
	| 'settings'
	| 'refresh'
	| 'play'
	| 'pause'
	| 'download'
	| 'upload'
	| 'git-branch'
	| 'git-pull-request'
	| 'bug'
	| 'zap'
	| 'lock';

type TmgrTone = 'default' | 'muted' | 'success' | 'warning' | 'danger';
type TmgrColor =
	'gray' | 'green' | 'yellow' | 'red' | 'blue' | 'purple' | 'orange';

/** Gap between a stack's children (API 1.3). */
type TmgrGap = 'none' | 'sm' | 'md' | 'lg';

/** The only things a plugin can draw. Unknown nodes are dropped by the host. */
type TmgrNode =
	| string
	| number
	| {
			type: 'stack';
			direction?: 'row' | 'column';
			children: TmgrNode[];
			/** API 1.3. Defaults to 'md'. */
			gap?: TmgrGap;
			/** API 1.3. Cross-axis alignment; defaults to stretch. */
			align?: 'start' | 'center' | 'end' | 'stretch';
			/** API 1.3. Main-axis distribution; defaults to start. */
			justify?: 'start' | 'between' | 'end';
			/** API 1.3. Lets this stack grow to fill the remaining space of its parent row. */
			grow?: boolean;
	  }
	/** API 1.3. A solid card on the theme's surface tokens, with a border and rounded corners. Cards nest up to 3 deep; deeper ones render as a stack. */
	| {
			type: 'card';
			children: TmgrNode[];
			/** Defaults to 'default'. */
			tone?: 'default' | 'muted' | 'raised';
			/** Defaults to 'md'. */
			padding?: 'sm' | 'md' | 'lg';
			/** A 3px accent stripe in this color. */
			accent?: TmgrColor;
			/** Makes the whole card a clickable/focusable button; its aria-label is taken from its content. */
			onClick?: { command: string; args?: unknown };
			grow?: boolean;
	  }
	/** API 1.3. Equal-width columns: `repeat(columns, minmax(minWidth, 1fr))`. */
	| {
			type: 'grid';
			/** Clamped to 1–6. */
			columns: number;
			/** Pixels, clamped to 160–480. Defaults to 200. */
			minWidth?: number;
			/** Defaults to 'md'. */
			gap?: 'sm' | 'md' | 'lg';
			children: TmgrNode[];
	  }
	/** API 1.3. A "⋯" dropdown of actions, each validated like a button. Dropped if it ends up with no valid items. */
	| {
			type: 'menu';
			label?: string;
			icon?: 'more';
			items: {
				text: string;
				command: string;
				args?: unknown;
				confirm?: string;
			}[];
	  }
	| { type: 'heading'; text: string; level?: 1 | 2 | 3 }
	/**
	 * API 1.7: text may be up to 20000 chars (other strings stay at 1000) and newlines show as line breaks.
	 * `spans` (up to 200) mixes styles inside one text; the joined span texts replace `text`.
	 */
	| {
			type: 'text';
			text: string;
			tone?: TmgrTone;
			spans?: {
				text: string;
				bold?: boolean;
				italic?: boolean;
				code?: boolean;
				strike?: boolean;
			}[];
	  }
	/**
	 * API 1.7. Renders Markdown (up to 20000 chars): headings, paragraphs, bold/italic/strikethrough, code,
	 * lists, task lists, tables, quotes and rules. Raw HTML shows as text and images show their alt text.
	 * Only https links to manifest links.allowedDomains are clickable (needs links:open); others are plain text.
	 */
	| { type: 'markdown'; text: string }
	/** API 1.7. Monospace block with a copy button (up to 20000 chars). `wrap` defaults to true; false scrolls sideways. */
	| { type: 'code'; text: string; lang?: string; wrap?: boolean }
	| {
			type: 'badge';
			text: string;
			color?: TmgrColor;
			/** API 1.3. */
			command?: string;
			args?: unknown;
			/** API 1.8. */
			icon?: TmgrIconName;
			/** API 1.8. Plain text, up to 200 chars. */
			tooltip?: string;
	  }
	/** API 1.8. A small inline icon. */
	| { type: 'icon'; name: TmgrIconName; tone?: TmgrTone }
	| {
			type: 'stat';
			label: string;
			value: string | number;
			tone?: TmgrTone;
			/** API 1.3. */
			command?: string;
			args?: unknown;
	  }
	| { type: 'progress'; value: number; color?: TmgrColor }
	| { type: 'list'; items: TmgrNode[] }
	| {
			type: 'table';
			columns: {
				key: string;
				title: string;
				/** API 1.10. */
				align?: 'left' | 'center' | 'right';
				/** API 1.10. 'xs' 4rem, 'sm' 7rem, 'md' 12rem, 'lg' 18rem. */
				width?: 'xs' | 'sm' | 'md' | 'lg';
				/** API 1.10. Clicking the header sorts the rows in the app; nothing is sent to the plugin. */
				sortable?: boolean;
			}[];
			rows: {
				taskId?: number;
				cells: Record<string, TmgrNode>;
				/** API 1.10. Sort values by column key; without one the cell text is used. */
				sort?: Record<string, string | number>;
				/** API 1.10. Tints the row and adds a colored left edge. */
				color?: TmgrColor;
				/** API 1.10. Runs when the row is clicked (not when a control inside it is). */
				onClick?: { command: string; args?: unknown };
				/** API 1.10. Nodes shown under the row when the user opens its arrow. */
				expand?: TmgrNode[];
			}[];
	  }
	/** API 1.10. A vertical list of events, at most 200. `at` is shown as given (at most 64 characters). */
	| {
			type: 'timeline';
			items: { at: string; title?: string; text: string; tone?: TmgrColor }[];
	  }
	/** API 1.10. Chat bubbles, at most 200. `text` is plain text; line breaks are kept. */
	| {
			type: 'messages';
			items: {
				author: string;
				text: string;
				at?: string;
				side?: 'left' | 'right';
			}[];
	  }
	/** API 1.10. A small line chart of up to 365 finite numbers. `label` is read out by screen readers. */
	| { type: 'sparkline'; values: number[]; color?: TmgrColor; label?: string }
	/** API 1.12. A bar or line chart. Up to 365 labels, 8 series and 365 values per series; `null` is a gap, as is any value beyond 1e15 in absolute size. Series with a blank name are dropped. `height` is 120 to 400 px (default 200). `label` is read out by screen readers. */
	| {
			type: 'chart';
			kind?: 'bar' | 'line';
			labels?: Array<string | number>;
			series: Array<{
				name: string;
				values: Array<number | null>;
				color?: TmgrColor;
			}>;
			height?: number;
			label?: string;
	  }
	/** API 1.10. A placeholder for "nothing here". */
	| { type: 'empty'; title: string; text?: string; icon?: TmgrIconName }
	/** API 1.10. A loading placeholder. `lines` is 1 to 12 (default 3). */
	| {
			type: 'skeleton';
			variant?: 'text' | 'card' | 'table';
			lines?: number;
	  }
	/**
	 * confirm (≤200 chars) asks the user, naming your plugin, before the command runs.
	 * variant and size (both API 1.3) style the button; both default to the plain look.
	 */
	| {
			type: 'button';
			text: string;
			command: string;
			args?: unknown;
			confirm?: string;
			/** 'danger', 'success' and 'outline' need API 1.8. */
			variant?:
				'default' | 'primary' | 'ghost' | 'danger' | 'success' | 'outline';
			size?: 'sm' | 'md';
			/** API 1.8. */
			icon?: TmgrIconName;
			/** API 1.8. Shows only the icon; `text` stays as the accessible label and tooltip. Needs `icon`. */
			iconOnly?: boolean;
			/** API 1.8. A disabled or loading button does not run its command. */
			disabled?: boolean;
			loading?: boolean;
			/** API 1.8. Plain text, up to 200 chars. */
			tooltip?: string;
	  }
	/** API 1.8. Buttons side by side (up to 12); other children are dropped. */
	| { type: 'buttonGroup'; children: Extract<TmgrNode, { type: 'button' }>[] }
	/**
	 * API 1.8. With `title` a disclosure header hides the children until opened (`open` sets the start state).
	 * With `visible` only the first N children show, plus an "N more" / "Show less" button. The state stays in the app.
	 */
	| {
			type: 'collapse';
			title?: string;
			open?: boolean;
			visible?: number;
			children: TmgrNode[];
	  }
	/** API 1.8. Up to 12 tabs with unique string ids (≤64 chars). The selected tab stays in the app; `selected` is the start tab. */
	| {
			type: 'tabs';
			tabs: {
				id: string;
				title: string;
				badge?: string;
				children: TmgrNode[];
			}[];
			selected?: string;
	  }
	/**
	 * API 1.8. A 2 to 4 option switch controlled by the plugin: a click runs `command` with `{ ...args, value }`,
	 * where `value` is one of the option values. Render the new `value` when the command ends.
	 */
	| {
			type: 'segmented';
			options: { value: string; text: string }[];
			value: string;
			command: string;
			args?: unknown;
	  }
	/**
	 * API 1.9. A single-line text field. The typed text (at most 500 characters) reaches `command` only as `args.value`,
	 * after the user types ('change', after `debounce` ms, 150 to 2000, default 300) or presses Enter ('submit').
	 * `name` is your own identifier (at most 64 characters of A-Z a-z 0-9 _ . -). It is never shown or sent.
	 */
	| {
			type: 'input';
			name: string;
			placeholder?: string;
			value?: string;
			command: string;
			args?: unknown;
			mode?: 'change' | 'submit';
			debounce?: number;
	  }
	/** API 1.9. A drop-down of up to 100 options. Choosing one runs `command` with `{ ...args, value }`, the option's own value. */
	| {
			type: 'select';
			name: string;
			options: { value: string; text: string }[];
			value?: string;
			placeholder?: string;
			command: string;
			args?: unknown;
	  }
	/** API 1.9. A checkbox. Toggling it runs `command` with `{ ...args, value }`, where `value` is a boolean. */
	| {
			type: 'checkbox';
			name: string;
			label: string;
			checked?: boolean;
			command: string;
			args?: unknown;
	  }
	/**
	 * API 1.11. A multi-line text field with its own Send button. Nothing is sent while the user types: Send, or
	 * Cmd+Enter / Ctrl+Enter, runs `command` with `{ ...args, value }`, the text cut to 5000 characters.
	 * Unsent text stays when you render again; after sending, your next `value` is shown. `rows` is 2 to 12 (default 4),
	 * `submitText` at most 40 characters (default "Send").
	 */
	| {
			type: 'textarea';
			name: string;
			value?: string;
			placeholder?: string;
			rows?: number;
			submitText?: string;
			command: string;
			args?: unknown;
	  }
	/**
	 * API 1.11. A date picker. Choosing a date in `min`..`max` runs `command` with `{ ...args, value: 'YYYY-MM-DD' }`;
	 * clearing it sends `value: null`. Dates must be real calendar dates in years 1000 to 9999; invalid ones are dropped.
	 */
	| {
			type: 'date';
			name: string;
			value?: string | null;
			min?: string;
			max?: string;
			command: string;
			args?: unknown;
	  }
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
	workspace: {
		current(): Promise<{
			id: number;
			code: string;
			name: string;
			kind: 'local' | 'cloud';
		}>;
	};
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
	time: {
		start(taskId: number): Promise<TmgrTask>;
		stop(taskId: number): Promise<TmgrTask>;
	};
	comments: {
		/**
		 * Needs comments:read. Each comment carries `author` (kind 'user' | 'plugin' | 'persona' | …),
		 * or `null` in a shared workspace when the server does not yet say who wrote it.
		 */
		list(
			taskId: number,
		): Promise<
			(Record<string, unknown> & { author: TmgrCommentAuthor | null })[]
		>;
		/** Needs comments:write. Written with this plugin as the author; a body `author` field is ignored. */
		add(
			taskId: number,
			text: string,
		): Promise<Record<string, unknown> & { author: TmgrCommentAuthor }>;
		/** Needs comments:write. Toggles the emoji reaction for this plugin; returns the comment's reactions. */
		react(
			commentId: number,
			emoji: string,
		): Promise<{ reactions: TmgrReaction[]; taskId?: number }>;
	};
	/** Per plugin and workspace, JSON values up to 256 KB, 5 MB in total. */
	storage: {
		get<T = unknown>(key: string): Promise<T | null>;
		set(key: string, value: unknown): Promise<void>;
		delete(key: string): Promise<void>;
		keys(): Promise<string[]>;
	};
	/**
	 * Per-task JSON values up to 64 KB, sharing the 5 MB / 1000 key quota with `storage`. In shared
	 * workspaces the calls reject with NOT_SUPPORTED: the server has the endpoints, but the desktop
	 * app does not use them there yet.
	 */
	taskData: {
		/** Needs tasks:read. */
		get<T = unknown>(taskId: number, key: string): Promise<T | null>;
		/** Needs tasks:write. */
		set(taskId: number, key: string, value: unknown): Promise<void>;
		/** Needs tasks:write. */
		delete(taskId: number, key: string): Promise<void>;
		/** Needs tasks:read. Up to 500 task ids in one call; meant for `ui.provideBadges` (one call per batch). */
		getMany<T = unknown>(
			taskIds: number[],
			key: string,
		): Promise<Record<number, T | null>>;
	};
	/**
	 * API 1.5, needs `engines.tmgr` `^1.5`. Pages of the workspace the plugin runs in; writes are made as
	 * this plugin (author kind "plugin"). Shared workspaces do not accept plugin writes to pages yet.
	 */
	pages: {
		/** Needs pages:read. `limit` is at most 50. */
		search(
			q: string,
			opts?: { type?: TmgrPageType; limit?: number },
		): Promise<TmgrPageSearchHit[]>;
		/** Needs pages:read. Every live page without bodies. */
		tree(): Promise<TmgrPageSummary[]>;
		/** Needs pages:read. By id or slug. */
		get(idOrSlug: number | string): Promise<TmgrPage>;
		/** Needs pages:write. */
		create(fields: {
			title: string;
			type?: TmgrPageType;
			parentId?: number | null;
			body?: string;
			properties?: Record<string, unknown>;
		}): Promise<TmgrPage>;
		/** Needs pages:write. Rejects with a `page_conflict` error (see TmgrPageConflictError) when `version` is stale. */
		update(
			id: number,
			fields: {
				version: number;
				title?: string;
				body?: string;
				properties?: Record<string, unknown>;
				summary?: string;
			},
		): Promise<TmgrPage>;
		/**
		 * Needs pages:write. Adds markdown at the end of the page, or at the end of the `##` section named
		 * `heading` (created when `createHeading` is set). No version needed.
		 */
		append(
			id: number,
			fields: {
				markdown: string;
				heading?: string;
				createHeading?: boolean;
				summary?: string;
			},
		): Promise<TmgrPage>;
		/**
		 * Needs pages:sections. Replaces the text of a managed section whose owner is `plugin:<this plugin's id>`;
		 * any other existing section rejects with PERMISSION_DENIED (sections owned by `agents` are for personas).
		 * A section that does not exist is created at the end of the page, owned by this plugin, under a `##`
		 * heading (`heading`, default the section id; one line, at most 200 characters). The id must match
		 * `[a-z0-9][a-z0-9-]{0,63}`. A context page refuses new sections.
		 */
		setSection(
			id: number,
			sectionId: string,
			markdown: string,
			opts?: { summary?: string; heading?: string },
		): Promise<TmgrPage>;
	};
	/**
	 * API 1.5. Per-page JSON values up to 64 KB, with their own 5 MB / 1000 key quota per
	 * plugin. Local workspaces only. Removed when the page is permanently deleted.
	 */
	pageData: {
		/** Needs pages:read. */
		get<T = unknown>(pageId: number, key: string): Promise<T | null>;
		/** Needs pages:write. */
		set(pageId: number, key: string, value: unknown): Promise<void>;
		/** Needs pages:write. */
		delete(pageId: number, key: string): Promise<void>;
		/** Needs pages:read. Up to 500 page ids in one call. */
		getMany<T = unknown>(
			pageIds: number[],
			key: string,
		): Promise<Record<number, T | null>>;
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
			fields: {
				agent?: string;
				model?: string;
				sessionId?: string;
				branch?: string;
			},
		): Promise<TmgrAgentWorkRun>;
		/**
		 * Needs agent_work:write. Only the run's own starting actor may update it, and only within its
		 * own agent namespace; 409 once it is finished.
		 */
		update(
			runId: number,
			patch: TmgrAgentWorkProgress,
		): Promise<TmgrAgentWorkRun>;
		/** Needs agent_work:write. Closes the run; `branch` cannot be changed here. */
		finish(
			runId: number,
			patch: { status: 'succeeded' | 'failed' | 'cancelled' } & Omit<
				TmgrAgentWorkProgress,
				'branch'
			>,
		): Promise<TmgrAgentWorkRun>;
	};
	files: {
		/** files:export. Text into <workspace>/exports/plugins/<plugin id>/<path>; path is relative, up to 5 segments. */
		export(path: string, content: string): Promise<{ path: string }>;
		reveal(path: string): Promise<void>;
		/** files:attachments. Attachments of tasks in this workspace, up to 5 MB. */
		list(taskId: number): Promise<
			{
				id: number;
				name: string;
				mimeType: string | null;
				size: number | null;
				createdAt: string;
			}[]
		>;
		read(fileId: number): Promise<{
			name: string;
			mimeType: string | null;
			size: number;
			base64: string;
			text: string | null;
		}>;
		/** files:pick. The user picks a file in a system dialog; null when they cancel. */
		pick(): Promise<{
			name: string;
			size: number;
			base64: string;
			text: string | null;
		} | null>;
	};
	/** Only origins listed in manifest network.allowedOrigins, all on this computer (http://localhost:<port>). */
	net: {
		fetch(
			url: string,
			init?: {
				method?: 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE';
				headers?: Record<string, string>;
				body?: string;
			},
		): Promise<{
			status: number;
			ok: boolean;
			headers: Record<string, string>;
			text(): Promise<string>;
			json<T = unknown>(): Promise<T>;
		}>;
	};
	/** task.* needs tasks:read, timer.* needs time:read, comment.* needs comments:read. Own writes are not delivered. */
	events: {
		on<T extends TmgrEvent['type']>(
			type: T,
			handler: (event: Extract<TmgrEvent, { type: T }>) => unknown,
		): Promise<void>;
	};
	/**
	 * The command id must be declared in contributes.commands and start with the plugin id. A command with
	 * `"deepLink": true` in its manifest entry can also be run from `tmgr://plugin/<id>/command/<local id>`
	 * links; needs the deeplinks permission, and the user confirms the first time per command and version.
	 * API 1.6, needs `engines.tmgr` `^1.6` and menus:task: a command can also be listed as an item of the
	 * task "…" menu with `"contributes": { "menus": { "task/card": [{ "command": "<id>", "title": "..." }] } }`
	 * (at most 3 items, title at most 40 characters, the command must be declared in contributes.commands).
	 * Its handler then receives a TaskMenuCommandArgs: `tmgr.commands.register(id, (args) => { const { taskId } = args as TaskMenuCommandArgs; ... })`.
	 * The item is shown only while the plugin runs and has registered the command.
	 */
	commands: {
		register(id: string, handler: (args: unknown) => unknown): Promise<void>;
	};
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
			provider: (tasks: TmgrTask[]) =>
				| Record<
						number,
						| {
								text: string;
								color?: TmgrColor;
								tooltip?: string;
								priority?: number;
								key?: string;
						  }
						| {
								text: string;
								color?: TmgrColor;
								tooltip?: string;
								priority?: number;
								key?: string;
						  }[]
				  >
				| Promise<
						Record<
							number,
							| {
									text: string;
									color?: TmgrColor;
									tooltip?: string;
									priority?: number;
									key?: string;
							  }
							| {
									text: string;
									color?: TmgrColor;
									tooltip?: string;
									priority?: number;
									key?: string;
							  }[]
						>
				  >,
		): Promise<void>;
		providePage(
			id: string,
			render: (props: unknown) => TmgrNode | Promise<TmgrNode>,
		): Promise<void>;
		provideTaskSection(
			id: string,
			render: (task: TmgrTask) => TmgrNode | Promise<TmgrNode>,
		): Promise<void>;
		setStatusBarItem(
			id: string,
			item: { text: string; tooltip?: string; command?: string } | null,
		): Promise<void>;
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
		 * item's title are non-empty; longer than 60 characters (graphemes, so an emoji counts as one) they
		 * are shortened with a trailing "…" (input over 1000 UTF-16 units is refused); at most 10 items; each item's args at most 4 KB of JSON.
		 * Rust shows the section as a submenu after the app's own tray items; clicking an item opens the
		 * task or runs the command, under the same rules as ui.notify. null removes the section.
		 */
		setTrayItem(
			id: string,
			item: {
				title: string;
				items: {
					title: string;
					taskId?: number;
					command?: string;
					args?: unknown;
				}[];
			} | null,
		): Promise<void>;
		/**
		 * Needs tray. Only the one plugin chosen in Settings -> Plugins for the menu bar text may call this;
		 * others get PERMISSION_DENIED. text is trimmed, must be at most 12 characters, a single line, and
		 * free of control characters. null clears it. Cleared automatically when this plugin stops.
		 */
		setTrayTitle(text: string | null): Promise<void>;
		/**
		 * API 1.4, needs `engines.tmgr` `^1.4` and views:badge (no user consent). Shows a counter on this
		 * plugin's own sidebar item for a view declared in contributes.views; null clears it.
		 * Give exactly one of `count` (safe integer >= 0; 0 clears; drawn as "99+" above 99) or `text`
		 * (1 to 4 UTF-16 units, e.g. "!" or "new"). `tone` defaults to 'default'.
		 * A viewId this plugin did not declare, both/neither of count and text, or any other bad value
		 * rejects with INVALID_PARAMS; a missing permission rejects with PERMISSION_DENIED.
		 * At most one applied change per view per second: calls inside that window are held and the last one
		 * wins, no error. The badge is cleared when the plugin stops, is disabled or removed, or crashes, and
		 * is not restored on app restart: set it again on 'app.started'. Works in shared workspaces too.
		 */
		setViewBadge(
			viewId: string,
			badge: {
				count?: number;
				text?: string;
				tone?: 'default' | 'info' | 'warning' | 'danger';
			} | null,
		): Promise<void>;
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
		update(
			id: number,
			patch: { title?: string; description?: string | null },
		): Promise<Routine>;
		/** Needs routines:write. Idempotent: completing an already-completed occurrence just returns it. `date` defaults to today. */
		complete(id: number, options?: { date?: string }): Promise<RoutineInstance>;
		/** Needs routines:write. Idempotent, like `complete`. */
		skip(id: number, options?: { date?: string }): Promise<RoutineInstance>;
		/**
		 * Needs routines:write AND tasks:write (missing tasks:write rejects with PERMISSION_DENIED).
		 * Creates a task from the routine (with a `key`, like `tasks.create`) and removes the routine.
		 */
		convertToTask(
			id: number,
			options?: { categoryId?: number; statusId?: number },
		): Promise<TmgrTask>;
	};
	/**
	 * Only available when the manifest declares a `companion` section. Asks the workspace owner to
	 * connect an external program you install yourself: it opens a dialog where they pick or create a
	 * persona and issue it a token. The token itself is never given to the plugin, only its id and
	 * prefix. Rejects with NOT_SUPPORTED in a shared (cloud) workspace, PERMISSION_DENIED without a
	 * `companion` manifest section, and RATE_LIMITED while a request from this plugin is already pending.
	 */
	localAccess: {
		requestConnection(opts?: {
			label?: string;
			permissions?: string[];
		}): Promise<
			| { status: 'connected'; tokenId: string; prefix: string }
			| { status: 'cancelled' }
		>;
	};
};
