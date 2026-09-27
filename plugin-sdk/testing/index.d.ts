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
	/** Saved plugin settings, as `tmgr.settings.get()` would return them. */
	settings?: Record<string, unknown>;
	/** Fixed clock (ms since epoch) used for `tmgr.alarms`, timers and `fireAlarms`. Defaults to `Date.now()`. */
	now?: number;
	/** Whether `tmgr.ui.setTrayTitle` succeeds, as if the user picked this plugin in Settings. Defaults to `true`. */
	trayTitleOwner?: boolean;
	dnd?: { active: boolean; until: string | null } | (() => { active: boolean; until: string | null });
	/** What `tmgr.files.pick()` resolves to. Defaults to `null` (the user cancelled). */
	filesPick?: unknown;
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
	storage: Record<string, string>;
	agentWork: Record<number, Record<string, unknown>[]>;
	attachments: Record<number, unknown[]>;
	files: Record<string, string>;
	alarms: Record<string, { name: string; scheduledAtMs: number; periodMinutes: number | null }>;
	notifications: Record<string, unknown>[];
	statusBar: Record<string, Record<string, unknown>>;
	trayItems: Record<string, Record<string, unknown>>;
	trayTitle: string | null;
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
