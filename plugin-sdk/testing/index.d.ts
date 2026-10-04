/** A QuickJSWASMModule from `quickjs-emscripten-core`; typed as `any` here so this package does not
 * need that library's types as a hard dependency of its own public surface. */
export type QuickJSWASMModuleLike = any;

export interface TestTask {
	id?: number;
	title: string;
	description?: string | null;
	status_id?: number | null;
	project_category_id?: number | null;
	priority?: string | null;
	approximately_time?: number;
	common_time?: number;
	start_time?: number;
	expired_at?: string | null;
	category_tasks_sequence_id?: number | null;
}

export interface TestStatus {
	id?: number;
	name: string;
	type?: 'default' | 'active' | 'completed' | 'hidden' | 'archived';
	color?: string | null;
}

export interface TestCategory {
	id?: number;
	title: string;
	code?: string | null;
}

export interface TestRoutine {
	id?: number;
	title: string;
	description?: string | null;
	scheduledDate?: string | null;
	/** Only used together with `scheduledDate`. */
	scheduledTime?: string | null;
	/** Only used together with `scheduledDate`. */
	frequency?: 'DAILY' | 'WEEKLY' | 'MONTHLY' | 'YEARLY' | null;
	createdAt?: string;
	updatedAt?: string;
}

export interface TestRoutineInstance {
	routineId: number;
	date: string;
	time?: string | null;
	status?: 'PENDING' | 'COMPLETED' | 'SKIPPED';
}

export interface TestPage {
	title: string;
	/** Defaults to a slug made from the title. */
	slug?: string;
	type?: 'plain' | 'context' | 'person' | 'meeting';
	parent_id?: number | null;
	/** Markdown; managed sections are `<!-- tmgr:section id="x" owner="plugin:your.id" -->` ... `<!-- /tmgr:section -->`. */
	body?: string;
	properties?: Record<string, unknown>;
	author_kind?: 'user' | 'persona' | 'plugin';
	author_id?: string;
}

export interface CreateTestHostOptions {
	/** The plugin's manifest.json, as a plain object (validated with the app's own `parseManifest`). */
	manifest: Record<string, unknown>;
	/** The plugin's main.js source. Mutually exclusive with `mainPath`. */
	code?: string;
	/** A path to the plugin's main.js source, read with `fs.readFileSync`. Mutually exclusive with `code`. */
	mainPath?: string;
	tasks?: TestTask[];
	statuses?: TestStatus[];
	categories?: TestCategory[];
	/** Backs `tmgr.pages.*` (API 1.5): versions, `page_conflict`, managed sections and append by heading. */
	pages?: TestPage[];
	/** Local workspaces only, like `tmgr.routines.*` itself. */
	routines?: TestRoutine[];
	routineInstances?: TestRoutineInstance[];
	/** Saved plugin settings, as `tmgr.settings.get()` would return them. */
	settings?: Record<string, unknown>;
	/** Fixed clock (ms since epoch) used for `tmgr.alarms`, timers and `fireAlarms`. Defaults to `Date.now()`. */
	now?: number;
	/** Whether `tmgr.ui.setTrayTitle` succeeds, as if the user picked this plugin in Settings. Defaults to `true`. */
	trayTitleOwner?: boolean;
	dnd?: { active: boolean; until: string | null } | (() => { active: boolean; until: string | null });
	/** What `tmgr.files.pick()` resolves to. Defaults to `null` (the user cancelled). */
	filesPick?: unknown;
	/** What `tmgr.localAccess.requestConnection()` resolves to. Defaults to `{ status: 'cancelled' }`. */
	localAccessConnect?: { status: 'connected'; tokenId: string; prefix: string } | { status: 'cancelled' };
	/** Backs `tmgr.net.fetch`; omit to make network calls reject with PERMISSION_DENIED, as on a computer with no network access granted. */
	fetch?: (request: {
		url: string;
		method: string;
		headers: [string, string][];
		body: string | null;
	}) => Promise<{ status: number; headers: [string, string][]; body: string }>;
	workspaceCode?: string;
	workspaceName?: string;
	/** CPU budget per synchronous slice of plugin code, passed to the sandbox. */
	cpuMs?: number;
	/** Wall-clock budget per dispatch, passed to the sandbox. */
	wallMs?: number;
	/** An already-loaded QuickJSWASMModule (e.g. Jest's require-based loader). Defaults to loading the
	 * app's own `@jitl/quickjs-wasmfile-release-sync` variant directly, which plain Node handles fine. */
	quickjs?: QuickJSWASMModuleLike | Promise<QuickJSWASMModuleLike>;
}

/** Live, mutable state the plugin's calls read and write; inspect it directly in assertions. */
export interface TestHostState {
	tasks: Record<string, unknown>[];
	statuses: Record<string, unknown>[];
	categories: Record<string, unknown>[];
	comments: Record<number, Record<string, unknown>[]>;
	taskData: Record<string, string>;
	pages: Record<string, unknown>[];
	/** `tmgr.pageData`, keyed `<pageId>:<key>`, values as JSON text. */
	pageData: Record<string, string>;
	storage: Record<string, string>;
	agentWork: Record<number, Record<string, unknown>[]>;
	routines: Record<string, unknown>[];
	routineInstances: Record<string, unknown>[];
	attachments: Record<number, unknown[]>;
	files: Record<string, string>;
	alarms: Record<string, { name: string; scheduledAtMs: number; periodMinutes: number | null }>;
	notifications: Record<string, unknown>[];
	statusBar: Record<string, Record<string, unknown>>;
	trayItems: Record<string, Record<string, unknown>>;
	trayTitle: string | null;
	/** Badges set with `tmgr.ui.setViewBadge`, by view id. Applied immediately (no 1 s coalescing); a null or 0 badge removes the key. */
	viewBadges: Record<string, { count: number | null; text: string | null; tone: 'default' | 'info' | 'warning' | 'danger' }>;
	log: { at: number; level: 'info' | 'warn' | 'error'; message: string }[];
}

export interface CardBadge {
	text: string;
	color: string;
	tooltip: string | null;
	priority: number;
	key?: string;
}

export interface TestHost {
	manifest: Record<string, unknown>;
	tmgr: TestHostState;
	/** Which ids the plugin has registered so far, by kind. */
	registered: Record<'event' | 'command' | 'badges' | 'page' | 'section', Set<string>>;
	/** Every broker method call the plugin made, in order: `[method, params]`. */
	calls: [string, unknown][];
	/** Dispatches an event to every handler the plugin registered for `event.type`. */
	emit(event: { type: string; [key: string]: unknown }): Promise<unknown>;
	runCommand(id: string, args?: unknown): Promise<unknown>;
	/** The task "…" menu items the app would show for this plugin (API 1.6): declared in `contributes.menus["task/card"]`, `menus:task` granted and the command registered. */
	taskMenuItems(): { command: string; title: string }[];
	/** Clicks a task menu item like the app does: refuses a command that is not one of `taskMenuItems()` or a `taskId` that is not a positive integer (`NOT_DECLARED`), then runs the command with `{ taskId, workspaceId }`. */
	clickTaskMenu(command: string, taskId: number): Promise<unknown>;
	/** Renders a declarative page, sanitised the same way the app sanitises it. */
	renderPage(id: string, props?: unknown): Promise<unknown>;
	/** Renders a declarative task panel section, sanitised the same way the app sanitises it. */
	renderSection(id: string, task: unknown): Promise<unknown>;
	/** Calls every registered badge provider for these tasks and merges the result, like the app does. */
	badges(tasks: { id: number }[]): Promise<Record<number, CardBadge[]>>;
	/** Advances the clock to `atTime` (or uses the current one) and fires every alarm now due. */
	fireAlarms(atTime?: number): Promise<unknown[]>;
	dispose(): void;
}

export function createTestHost(options: CreateTestHostOptions): Promise<TestHost>;
