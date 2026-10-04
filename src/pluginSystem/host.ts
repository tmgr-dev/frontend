import type { DomainEvent } from '@/utils/domainEvents';
import {
	createBroker,
	PLUGIN_EVENTS,
	PluginError,
	type AlarmInfo,
	type AlarmSpec,
	type BrokerDeps,
	type DataApi,
	type FetchRequest,
	type FetchResponse,
	type LocalConnectionResult,
	type NotifyPayload,
	type PluginWorkspace,
	type RegistrationKind,
	type TrayItemSpec,
	type ViewBadgeSpec,
} from './broker';
import { taskKey } from './dataApi';
import { createPageEventMapper } from './pageEvents';
import { TASK_MENU_LOCATION, type PluginManifest } from './manifest';
import { resolveTaskMenuItems } from './taskMenu';
import { toRoutine, toRoutineInstance } from './routines';
import {
	startPluginProcess,
	type PluginProcess,
	type WorkerEndpoint,
} from './process';
import {
	isLinkAllowed,
	sanitizeTree,
	COLORS,
	type Color,
	type LinkContext,
	type UiNode,
} from './uiTree';

export type PluginSource = 'builtin' | 'folder' | 'installed';
export type PluginStatus =
	| 'stopped'
	| 'starting'
	| 'running'
	| 'failed'
	| 'crashed'
	| 'blocked';

export interface PluginPackage {
	manifest: PluginManifest;
	code: string;
	source: PluginSource;
	/** HTML pages from the plugin's ui/ folder, by path, for views that open in a window. */
	pages?: Record<string, string>;
	/** Where an installed plugin came from; `verified` when the catalog vouches for its publisher key. */
	origin?: {
		repo: string;
		tag: string;
		sha256: string;
		public_key?: string;
		verified?: boolean;
	};
}

export interface PluginLogLine {
	at: number;
	level: 'info' | 'warn' | 'error';
	message: string;
}

export interface PluginEntry {
	manifest: PluginManifest;
	source: PluginSource;
	origin?: PluginPackage['origin'];
	status: PluginStatus;
	error: string | null;
	log: PluginLogLine[];
}

/** Persisted form of an alarm, keyed by `${storageId}@${workspaceId}` in `deps.alarms`. */
export interface AlarmDef {
	name: string;
	scheduledAtMs: number;
	periodMinutes: number | null;
}

export interface NotificationClickResult {
	type: 'task' | 'command';
	taskId?: number;
	workspaceId?: number;
	/** For a task result: re-check with `isCurrentRun` right before acting on it. */
	pluginId?: string;
	generation?: string;
}

export interface StatusBarEntry {
	pluginId: string;
	pluginName: string;
	itemId: string;
	text: string;
	tooltip: string | null;
	command: string | null;
}

export interface CardBadge {
	pluginId: string;
	badgeId: string;
	text: string;
	color: Color;
	tooltip: string | null;
	priority: number;
	key?: string;
}

export interface TrayMenuItem {
	/** Opaque: the host maps it back to the plugin's taskId/command/args; Rust never sees those. */
	id: string;
	title: string;
}

export interface TrayItemEntry {
	pluginId: string;
	pluginName: string;
	itemId: string;
	title: string;
	items: TrayMenuItem[];
}

export interface ViewBadgeEntry {
	pluginId: string;
	viewId: string;
	count: number | null;
	text: string | null;
	tone: 'default' | 'info' | 'warning' | 'danger';
}

export interface PluginHostState {
	workspace: PluginWorkspace | null;
	safeMode: boolean;
	plugins: Record<string, PluginEntry>;
	statusBar: Record<string, StatusBarEntry>;
	trayItems: Record<string, TrayItemEntry>;
	/** Set by the one plugin chosen in Settings for the menu bar text; null when none is chosen or set. */
	trayTitle: string | null;
	/** Keyed `${pluginId}:${viewId}`; memory only, cleared when the plugin stops. */
	viewBadges: Record<string, ViewBadgeEntry>;
	/** Bumped when plugins start or stop: every badge, page and section is asked again. */
	revision: number;
	/** Per plugin, bumped (throttled) when that plugin asks for its UI to be drawn again. */
	revisions: Record<string, number>;
}

export interface PluginHostDeps {
	state: PluginHostState;
	packages: () => Promise<PluginPackage[]>;
	createEndpoint: () => WorkerEndpoint;
	/** Data access pinned to the workspace the plugin was started in. */
	api: (
		pluginId: string,
		workspace: PluginWorkspace,
		storageId: string,
		pluginName: string,
	) => DataApi;
	subscribe: (handler: (event: DomainEvent) => void) => () => void;
	enabled: {
		get: (pluginId: string, workspaceId: number) => boolean | undefined;
		set: (pluginId: string, workspaceId: number, value: boolean) => void;
	};
	settings: {
		get: (pluginId: string) => Record<string, unknown> | undefined;
		set: (pluginId: string, values: Record<string, unknown>) => void;
	};
	notify: (title: string, message: string) => void;
	/** A plugin's own `tmgr.ui.notify`; separate from `notify` above, which is for host-level messages. */
	notifyPlugin?: (
		pluginId: string,
		pluginName: string,
		payload: NotifyPayload & { token: string | null },
	) => void;
	dnd?: () => { active: boolean; until: string | null };
	/** Persisted alarm definitions, keyed by `${storageId}@${workspaceId}`; the host holds none in memory. */
	alarms?: {
		get: (key: string) => Record<string, AlarmDef> | undefined;
		set: (key: string, defs: Record<string, AlarmDef>) => void;
	};
	currentWorkspaceId: () => number | null;
	fetch?: (request: FetchRequest) => Promise<FetchResponse>;
	files?: (
		pluginId: string,
		workspace: PluginWorkspace,
		pluginName: string,
	) => BrokerDeps['files'];
	windows?: {
		open: (
			key: string,
			html: string,
			title: string,
			generation: string,
		) => Promise<void>;
		close: (pluginId: string) => Promise<void>;
	};
	/** Whether a plugin may reach this computer (network, files) there; a shared workspace needs the member's consent. */
	machineAllowed?: (pluginId: string, workspace: PluginWorkspace) => boolean;
	/** The plugin id chosen in Settings to show text in the menu bar, or null when none is chosen. */
	trayTitleOwner?: () => string | null;
	/** Why a plugin must not run (the signed blocklist); it wins over every other setting. */
	blocked?: (pluginId: string) => string | null;
	/** Opens a link a `link` node was allowed to open; desktop only, so the web build has no-op links. */
	openExternal?: (url: string) => void;
	/** Asks the host to open the connect flow for tmgr.localAccess.requestConnection; undefined disables it. */
	requestLocalConnection?: (
		pluginId: string,
		opts: { label?: string; permissions?: string[] },
	) => Promise<LocalConnectionResult>;
	/** Revokes this plugin's local-access tokens; called on disable and uninstall. */
	revokePluginTokens?: (pluginId: string) => Promise<void>;
	now?: () => number;
	cpuMs?: number;
	wallMs?: number;
}

const FAULT_LIMIT = 3;
const FAULT_WINDOW_MS = 5 * 60_000;
const LOG_LIMIT = 200;
const MESSAGE_LIMIT = 1000;
const REFRESH_THROTTLE_MS = 500;
const MAX_BADGES_PER_TASK = 5;
const BADGE_KEY = /^[a-z0-9_-]{1,40}$/;

const clampPriority = (value: unknown): number => {
	const n = typeof value === 'number' && Number.isFinite(value) ? value : 0;
	return Math.max(-100, Math.min(100, n));
};

const normalizeBadge = (pluginId: string, badgeId: string, badge: any): CardBadge => ({
	pluginId,
	badgeId,
	text: String(badge.text).slice(0, 16),
	color: COLORS.includes(badge.color) ? badge.color : 'gray',
	tooltip: typeof badge.tooltip === 'string' ? badge.tooltip.slice(0, 200) : null,
	priority: clampPriority(badge.priority),
	key: typeof badge.key === 'string' && BADGE_KEY.test(badge.key) ? badge.key : undefined,
});
const MAX_ALARMS = 10;
const NOTIFY_LIMIT = 5;
const NOTIFY_WINDOW_MS = 60_000;
const CLICK_TTL_MS = 60 * 60_000;
const ONE_YEAR_MINUTES = 525_600;
const ONE_YEAR_MS = ONE_YEAR_MINUTES * 60_000;

/** An installed plugin's data belongs to its repository, so its id alone is not enough to key storage or consent. */
export const storageIdOf = (pkg: {
	manifest: { id: string };
	origin?: { repo: string };
}): string =>
	pkg.origin ? `${pkg.manifest.id}@github.com/${pkg.origin.repo}` : pkg.manifest.id;

const isValidAlarmDef = (def: AlarmDef): boolean =>
	Number.isFinite(def.scheduledAtMs) &&
	(def.periodMinutes === null ||
		(Number.isFinite(def.periodMinutes) &&
			def.periodMinutes >= 1 &&
			def.periodMinutes <= ONE_YEAR_MINUTES));

interface Running {
	process: PluginProcess;
	broker: ReturnType<typeof createBroker>;
	registered: Record<RegistrationKind, Set<string>>;
	/** Changes on every start: windows and calls of an earlier run are refused. */
	generation: string;
	/** The member allowed it to reach this computer (always true in a local workspace). */
	machine: boolean;
}

export const createPluginHost = (deps: PluginHostDeps) => {
	const { state } = deps;
	const now = deps.now ?? Date.now;
	const packages = new Map<string, PluginPackage>();
	const running = new Map<string, Running>();
	const faults = new Map<string, number[]>();
	let nextGeneration = 0;
	const refreshTimers = new Map<string, ReturnType<typeof setTimeout>>();
	const inMemoryAlarms = new Map<string, Record<string, AlarmDef>>();
	const alarmsDep = deps.alarms ?? {
		get: (key: string) => inMemoryAlarms.get(key),
		set: (key: string, defs: Record<string, AlarmDef>) =>
			void inMemoryAlarms.set(key, defs),
	};
	const startedThisSession = new Set<string>();
	const lastWorkspaceOf = new Map<string, number>();
	const notifyTimestamps = new Map<string, number[]>();
	let nextClickToken = 1;
	const pendingClicks = new Map<
		string,
		{
			pluginId: string;
			workspaceId: number;
			generation: string;
			taskId: number | null;
			command: string | null;
			args: unknown;
			expiresAt: number;
		}
	>();

	const alarmsFor = (workspace: PluginWorkspace, storageId: string) => {
		const key = `${storageId}@${workspace.id}`;
		return {
			create: (name: string, spec: AlarmSpec): AlarmInfo => {
				if (!Number.isFinite(spec.scheduledAtMs) || spec.scheduledAtMs > now() + ONE_YEAR_MS) {
					throw new PluginError(
						'INVALID_PARAMS',
						'scheduled time must be finite and at most a year ahead',
					);
				}
				const current = alarmsDep.get(key) ?? {};
				// A corrupt entry from before this check existed must not hold a slot forever.
				for (const [existingName, def] of Object.entries(current)) {
					if (!isValidAlarmDef(def)) delete current[existingName];
				}
				if (
					!Object.prototype.hasOwnProperty.call(current, name) &&
					Object.keys(current).length >= MAX_ALARMS
				) {
					throw new PluginError(
						'INVALID_PARAMS',
						`a plugin may have at most ${MAX_ALARMS} alarms per workspace`,
					);
				}
				current[name] = {
					name,
					scheduledAtMs: spec.scheduledAtMs,
					periodMinutes: spec.periodMinutes,
				};
				alarmsDep.set(key, current);
				return { name, scheduledAt: new Date(spec.scheduledAtMs).toISOString() };
			},
			clear: (name: string) => {
				const current = alarmsDep.get(key) ?? {};
				if (!(name in current)) return;
				delete current[name];
				alarmsDep.set(key, current);
			},
			list: (): AlarmInfo[] =>
				Object.values(alarmsDep.get(key) ?? {})
					.filter(isValidAlarmDef)
					.map((def) => ({
						name: def.name,
						scheduledAt: new Date(def.scheduledAtMs).toISOString(),
					})),
		};
	};

	/** Survives a plugin restart within the window: a crash loop must not reset the notification budget. */
	const notifyAllowed = (pluginId: string) => {
		const recent = (notifyTimestamps.get(pluginId) ?? []).filter(
			(t) => now() - t < NOTIFY_WINDOW_MS,
		);
		if (recent.length >= NOTIFY_LIMIT) {
			notifyTimestamps.set(pluginId, recent);
			return false;
		}
		recent.push(now());
		notifyTimestamps.set(pluginId, recent);
		return true;
	};

	const registerClick = (
		pluginId: string,
		workspaceId: number,
		generation: string,
		payload: NotifyPayload,
	): string => {
		for (const [key, entry] of pendingClicks) {
			if (entry.expiresAt < now()) pendingClicks.delete(key);
		}
		const token = `c${nextClickToken++}${Math.random().toString(36).slice(2, 8)}`;
		pendingClicks.set(token, {
			pluginId,
			workspaceId,
			generation,
			taskId: payload.taskId,
			command: payload.command,
			args: payload.args,
			expiresAt: now() + CLICK_TTL_MS,
		});
		return token;
	};

	/** Coalesces a plugin's refresh requests so a chatty plugin cannot flood the UI with re-renders. */
	const bump = (pluginId: string) => {
		if (refreshTimers.has(pluginId)) return;
		refreshTimers.set(
			pluginId,
			setTimeout(() => {
				refreshTimers.delete(pluginId);
				state.revisions[pluginId] = (state.revisions[pluginId] ?? 0) + 1;
			}, REFRESH_THROTTLE_MS),
		);
	};

	const log = (
		pluginId: string,
		level: PluginLogLine['level'],
		message: string,
	): PluginLogLine | undefined => {
		const entry = state.plugins[pluginId];
		if (!entry) return undefined;
		const line: PluginLogLine = { at: now(), level, message: message.slice(0, MESSAGE_LIMIT) };
		entry.log.push(line);
		if (entry.log.length > LOG_LIMIT)
			entry.log.splice(0, entry.log.length - LOG_LIMIT);
		return line;
	};

	const REFUSAL_THROTTLE_MS = 5_000;
	const VIEW_BADGE_WINDOW_MS = 1_000;
	const brokerRefusals = new Map<
		string,
		{ message: string; count: number; at: number; line: PluginLogLine }
	>();

	/** Makes a plugin's broker refusals visible in its log, collapsing identical repeats within 5s. */
	const logBrokerRefusal = (pluginId: string, method: string, error: unknown) => {
		if (!(error instanceof PluginError)) return;
		const message = `${method}: ${error.code} ${error.message}`;
		const previous = brokerRefusals.get(pluginId);
		if (previous && previous.message === message && now() - previous.at < REFUSAL_THROTTLE_MS) {
			previous.count++;
			previous.at = now();
			previous.line.at = now();
			previous.line.message = `${message} (×${previous.count})`.slice(0, MESSAGE_LIMIT);
			return;
		}
		const line = log(pluginId, 'warn', message);
		if (line) brokerRefusals.set(pluginId, { message, count: 1, at: now(), line });
	};

	/** A refused tray item is logged and answered with null: it must not reject the handler that is rendering a page. */
	const callBroker = (
		pluginId: string,
		broker: ReturnType<typeof createBroker>,
		method: string,
		params: unknown,
	) =>
		broker.call(method, params).catch((error) => {
			logBrokerRefusal(pluginId, method, error);
			if (
				method === 'ui.setTrayItem' &&
				error instanceof PluginError &&
				(error.code === 'INVALID_PARAMS' || error.code === 'PERMISSION_DENIED')
			)
				return null;
			throw error;
		});

	const settingsOf = (pluginId: string) => {
		const schema =
			packages.get(pluginId)?.manifest.contributes.settings?.properties ?? {};
		const saved = deps.settings.get(pluginId) ?? {};
		const values: Record<string, unknown> = {};
		for (const [key, property] of Object.entries(schema)) {
			values[key] =
				typeof saved[key] === property.type
					? saved[key]
					: property.default ?? null;
		}
		return values;
	};

	const linkContext = (pluginId: string): LinkContext => {
		const manifest = packages.get(pluginId)?.manifest;
		return {
			allowedDomains: manifest?.links.allowedDomains ?? [],
			linksOpen:
				(manifest?.permissions.includes('links:open') ?? false) &&
				(running.get(pluginId)?.machine ?? false),
		};
	};

	const clearStatusBar = (pluginId: string) => {
		for (const key of Object.keys(state.statusBar)) {
			if (state.statusBar[key].pluginId === pluginId)
				delete state.statusBar[key];
		}
	};

	const badgeTimers = new Map<
		string,
		{ at: number; timer: ReturnType<typeof setTimeout> | null; pending: ViewBadgeSpec | null }
	>();

	const applyViewBadge = (
		key: string,
		pluginId: string,
		viewId: string,
		badge: ViewBadgeSpec | null,
	) => {
		if (badge) state.viewBadges[key] = { pluginId, viewId, ...badge };
		else delete state.viewBadges[key];
	};

	const setViewBadge = (
		pluginId: string,
		generation: string,
		viewId: string,
		badge: ViewBadgeSpec | null,
	) => {
		const key = `${pluginId}:${viewId}`;
		const slot = badgeTimers.get(key);
		if (!slot || now() - slot.at >= VIEW_BADGE_WINDOW_MS) {
			if (slot?.timer) clearTimeout(slot.timer);
			badgeTimers.set(key, { at: now(), timer: null, pending: null });
			return applyViewBadge(key, pluginId, viewId, badge);
		}
		slot.pending = badge;
		if (slot.timer) return;
		slot.timer = setTimeout(() => {
			if (badgeTimers.get(key) !== slot || running.get(pluginId)?.generation !== generation)
				return;
			slot.timer = null;
			slot.at = now();
			applyViewBadge(key, pluginId, viewId, slot.pending);
			slot.pending = null;
		}, VIEW_BADGE_WINDOW_MS - (now() - slot.at));
	};

	const clearViewBadgesOf = (pluginId: string) => {
		for (const [key, slot] of badgeTimers) {
			if (!key.startsWith(`${pluginId}:`)) continue;
			if (slot.timer) clearTimeout(slot.timer);
			badgeTimers.delete(key);
		}
		for (const key of Object.keys(state.viewBadges)) {
			if (state.viewBadges[key].pluginId === pluginId) delete state.viewBadges[key];
		}
	};

	const trayClickTargets = new Map<
		string,
		{
			pluginId: string;
			workspaceId: number;
			generation: string;
			taskId: number | null;
			command: string | null;
			args: unknown;
		}
	>();
	let nextTrayToken = 1;
	let trayTitleSetBy: string | null = null;
	/** A deep link's params for the next window a plugin opens; read once via `deepLink.params`. */
	const windowProps = new Map<string, Record<string, string>>();

	const clearTrayItem = (key: string) => {
		state.trayItems[key]?.items.forEach((item) => trayClickTargets.delete(item.id));
		delete state.trayItems[key];
	};

	const clearTrayItemsOf = (pluginId: string) => {
		for (const key of Object.keys(state.trayItems)) {
			if (state.trayItems[key].pluginId === pluginId) clearTrayItem(key);
		}
	};

	const clearTrayTitleOf = (pluginId: string) => {
		if (trayTitleSetBy === pluginId) {
			state.trayTitle = null;
			trayTitleSetBy = null;
		}
	};

	const setTrayItem = (
		pluginId: string,
		pluginName: string,
		itemId: string,
		generation: string,
		spec: TrayItemSpec | null,
	) => {
		const key = `${pluginId}:${itemId}`;
		clearTrayItem(key);
		if (!spec || !state.workspace) return;
		const workspaceId = state.workspace.id;
		const items = spec.items.map((item) => {
			const token = `t${nextTrayToken++}`;
			trayClickTargets.set(token, {
				pluginId,
				workspaceId,
				generation,
				taskId: item.taskId,
				command: item.command,
				args: item.args,
			});
			return { id: token, title: item.title };
		});
		state.trayItems[key] = { pluginId, pluginName, itemId, title: spec.title, items };
	};

	const setTrayTitle = (pluginId: string, text: string | null) => {
		if (text === null) return clearTrayTitleOf(pluginId);
		state.trayTitle = text;
		trayTitleSetBy = pluginId;
	};

	const stop = (
		pluginId: string,
		status: PluginStatus = 'stopped',
		error: string | null = null,
	) => {
		running.get(pluginId)?.process.stop();
		running.delete(pluginId);
		clearStatusBar(pluginId);
		clearViewBadgesOf(pluginId);
		clearTrayItemsOf(pluginId);
		const pkg = packages.get(pluginId);
		clearTrayTitleOf(pkg ? storageIdOf(pkg) : pluginId);
		for (const key of windowProps.keys()) {
			if (key.startsWith(`${pluginId}/`)) windowProps.delete(key);
		}
		for (const [token, entry] of pendingClicks) {
			if (entry.pluginId === pluginId) pendingClicks.delete(token);
		}
		void deps.windows?.close(pluginId).catch(() => undefined);
		const entry = state.plugins[pluginId];
		if (entry) {
			entry.status = status;
			entry.error = error?.slice(0, MESSAGE_LIMIT) ?? null;
		}
		state.revision++;
	};

	const fault = (pluginId: string, reason: string) => {
		log(pluginId, 'error', reason);
		const recent = [...(faults.get(pluginId) ?? []), now()].filter(
			(t) => now() - t < FAULT_WINDOW_MS,
		);
		faults.set(pluginId, recent);
		if (recent.length >= FAULT_LIMIT && running.has(pluginId)) {
			stop(pluginId, 'crashed', reason);
			const name = state.plugins[pluginId]?.manifest.name ?? pluginId;
			deps.notify(
				`Plugin ${name} was turned off`,
				`It failed ${FAULT_LIMIT} times in 5 minutes: ${reason.slice(0, 300)}`,
			);
		}
	};

	const blockedReason = (pluginId: string) => deps.blocked?.(pluginId) ?? null;

	/** Built-in plugins are on by default only in local workspaces; a shared one runs what its creator turned on. */
	const isEnabled = (pluginId: string, workspace: PluginWorkspace) =>
		deps.enabled.get(pluginId, workspace.id) ??
		(workspace.kind === 'local' &&
			packages.get(pluginId)?.source === 'builtin');

	const start = async (
		pkg: PluginPackage,
		workspace: PluginWorkspace,
		fromActivate = false,
	) => {
		const { manifest } = pkg;
		const pluginId = manifest.id;
		const blocked = blockedReason(pluginId);
		if (blocked) {
			stop(pluginId, 'blocked', blocked);
			return;
		}
		const registered: Running['registered'] = {
			event: new Set(),
			command: new Set(),
			badges: new Set(),
			page: new Set(),
			section: new Set(),
		};
		const machine = deps.machineAllowed?.(pluginId, workspace) ?? true;
		const storageId = storageIdOf(pkg);
		// Fixed for this run: closures below must not ask `running` for "the current" generation later.
		const generation = String(++nextGeneration);
		const broker = createBroker({
			manifest,
			workspace,
			currentWorkspaceId: deps.currentWorkspaceId,
			// An installed plugin's data belongs to its repository: another author reusing the id gets none of it.
			api: deps.api(pluginId, workspace, storageId, manifest.name),
			settings: () => settingsOf(pluginId),
			notify: (payload) => {
				if (!notifyAllowed(pluginId))
					throw new PluginError('RATE_LIMITED', 'too many notifications');
				if (deps.dnd?.().active) return;
				const token =
					payload.taskId != null || payload.command != null
						? registerClick(pluginId, workspace.id, generation, payload)
						: null;
				// Always attributed: a native/toast title never looks like it came from the app itself.
				const title = payload.title
					? `${manifest.name}: ${payload.title}`
					: manifest.name;
				deps.notifyPlugin?.(pluginId, manifest.name, { ...payload, title, token });
			},
			dnd: deps.dnd,
			alarms: alarmsFor(workspace, storageId),
			tray: machine
				? {
						setItem: (itemId, item) =>
							setTrayItem(pluginId, manifest.name, itemId, generation, item),
						setTitle: (text) => setTrayTitle(storageId, text),
						isTitleOwner: () => deps.trayTitleOwner?.() === storageId,
				  }
				: undefined,
			setStatusBarItem: (itemId, item) => {
				const key = `${pluginId}:${itemId}`;
				if (item) {
					state.statusBar[key] = {
						pluginId,
						pluginName: manifest.name,
						itemId,
						...item,
					};
				} else delete state.statusBar[key];
			},
			setViewBadge: (viewId, badge) => setViewBadge(pluginId, generation, viewId, badge),
			refresh: () => bump(pluginId),
			register: (kind, id) => {
				if (registered[kind].has(id)) return;
				registered[kind].add(id);
				if (kind !== 'event' && kind !== 'command') bump(pluginId);
			},
			log: (level, message) => log(pluginId, level, message),
			fetch: machine ? deps.fetch : undefined,
			files: machine
				? deps.files?.(pluginId, workspace, manifest.name)
				: undefined,
			localAccess: deps.requestLocalConnection
				? { requestConnection: (opts) => deps.requestLocalConnection!(pluginId, opts) }
				: undefined,
			now,
		});
		const process = startPluginProcess(pkg.code, {
			endpoint: deps.createEndpoint(),
			call: (method, params) => callBroker(pluginId, broker, method, params),
			onCrash: (reason) => {
				fault(pluginId, reason);
				// The Worker is gone; a dead plugin must not keep looking alive.
				if (running.get(pluginId)?.process === process)
					stop(pluginId, 'crashed', reason);
			},
			onFault: (reason) => fault(pluginId, `background: ${reason}`),
			cpuMs: deps.cpuMs,
			wallMs: deps.wallMs,
		});
		running.set(pluginId, {
			process,
			broker,
			registered,
			generation,
			machine,
		});
		state.plugins[pluginId].status = 'starting';
		state.plugins[pluginId].error = null;
		try {
			await process.ready;
			if (running.get(pluginId)?.process !== process) return;
			state.plugins[pluginId].status = 'running';
			log(pluginId, 'info', `started in ${workspace.name}`);
			if (!startedThisSession.has(pluginId)) {
				startedThisSession.add(pluginId);
				if (registered.event.has('app.started'))
					void dispatch(pluginId, 'event', 'app.started', {}).catch(
						() => undefined,
					);
			}
			if (fromActivate) {
				const previous = lastWorkspaceOf.get(storageId);
				if (previous !== undefined && previous !== workspace.id) {
					if (registered.event.has('workspace.switched'))
						void dispatch(pluginId, 'event', 'workspace.switched', {
							from: previous,
							to: workspace.id,
						}).catch(() => undefined);
				}
				lastWorkspaceOf.set(storageId, workspace.id);
			}
			tick();
		} catch (error) {
			if (running.get(pluginId)?.process !== process) return;
			stop(
				pluginId,
				'failed',
				error instanceof Error ? error.message : String(error),
			);
		}
	};

	const dispatch = async (
		pluginId: string,
		kind: RegistrationKind,
		target: string,
		args: unknown,
	) => {
		const plugin = running.get(pluginId);
		if (!plugin || state.plugins[pluginId]?.status !== 'running')
			return undefined;
		try {
			return await plugin.process.dispatch(kind, target, args);
		} catch (error: any) {
			const message = `${kind} ${target}: ${error?.message ?? error}`;
			if (error?.code === 'TIMEOUT' || error?.code === 'CRASHED')
				fault(pluginId, message);
			else log(pluginId, 'error', message);
			// Work the timed-out call left behind could still write later; a fresh run drops it.
			if (error?.code === 'TIMEOUT' && running.get(pluginId) === plugin)
				void relaunch(pluginId);
			throw error;
		}
	};

	/** The single scheduler for every plugin's alarms; call from a Rust tick, focus/visibility, or after a start. */
	const tick = () => {
		const workspace = state.workspace;
		if (!workspace || state.safeMode) return;
		const time = now();
		for (const pluginId of running.keys()) {
			if (state.plugins[pluginId]?.status !== 'running') continue;
			const pkg = packages.get(pluginId);
			if (!pkg?.manifest.permissions.includes('alarms')) continue;
			const key = `${storageIdOf(pkg)}@${workspace.id}`;
			const current = alarmsDep.get(key) ?? {};
			let corrupted = false;
			for (const [name, def] of Object.entries(current)) {
				if (!isValidAlarmDef(def)) {
					delete current[name];
					corrupted = true;
				}
			}
			const due = Object.values(current).filter((def) => def.scheduledAtMs <= time);
			if (!due.length) {
				if (corrupted) alarmsDep.set(key, current);
				continue;
			}
			// Persist the reschedule/removal before dispatching: an overlapping tick must not double-fire.
			for (const def of due) {
				if (def.periodMinutes) {
					current[def.name] = { ...def, scheduledAtMs: time + def.periodMinutes * 60_000 };
				} else {
					delete current[def.name];
				}
			}
			alarmsDep.set(key, current);
			for (const def of due) {
				void dispatch(pluginId, 'event', 'alarm', {
					name: def.name,
					scheduledAt: new Date(def.scheduledAtMs).toISOString(),
				}).catch(() => undefined);
			}
		}
	};

	/** The same plugin run, still in the same workspace: the check every click and deep link re-runs right before acting. */
	const isCurrentRun = (
		pluginId: string,
		expected: { generation: string | null; workspaceId: number | null },
	): boolean =>
		running.get(pluginId)?.generation === expected.generation &&
		deps.currentWorkspaceId() === expected.workspaceId;

	/** The declared+registered+running+same-run+same-workspace+admit checks shared by notification and tray clicks. */
	const resolveClick = async (
		pluginId: string,
		workspaceId: number,
		generation: string,
		taskId: number | null,
		command: string | null,
		args: unknown,
	): Promise<NotificationClickResult | null> => {
		const plugin = running.get(pluginId);
		if (!plugin || plugin.generation !== generation) return null;
		if (state.plugins[pluginId]?.status !== 'running') return null;
		if (deps.currentWorkspaceId() !== workspaceId) return null;
		if (command) {
			const declared = packages
				.get(pluginId)!
				.manifest.contributes.commands.some((c) => c.id === command);
			if (!declared || !plugin.registered.command.has(command)) return null;
			try {
				plugin.broker.admit(true);
			} catch {
				return null;
			}
			void dispatch(pluginId, 'command', command, args).catch(() => undefined);
			return { type: 'command' };
		}
		if (taskId != null) return { type: 'task', taskId, workspaceId, pluginId, generation };
		return null;
	};

	const resolveNotificationClick = async (
		token: string,
	): Promise<NotificationClickResult | null> => {
		const entry = pendingClicks.get(token);
		if (!entry) return null;
		pendingClicks.delete(token);
		if (now() > entry.expiresAt) return null;
		return resolveClick(
			entry.pluginId,
			entry.workspaceId,
			entry.generation,
			entry.taskId,
			entry.command,
			entry.args,
		);
	};

	const resolveTrayClick = async (
		token: string,
	): Promise<NotificationClickResult | null> => {
		const entry = trayClickTargets.get(token);
		if (!entry) return null;
		return resolveClick(
			entry.pluginId,
			entry.workspaceId,
			entry.generation,
			entry.taskId,
			entry.command,
			entry.args,
		);
	};

	const relaunch = async (pluginId: string) => {
		const pkg = packages.get(pluginId);
		const workspace = state.workspace;
		stop(pluginId);
		if (
			pkg &&
			workspace &&
			!state.safeMode &&
			isEnabled(pluginId, workspace)
		) {
			await start(pkg, workspace);
		}
	};

	const mapPageEvent = createPageEventMapper();

	const unsubscribe = deps.subscribe((event) => {
		const workspace = state.workspace;
		if (!workspace || event.workspaceId !== workspace.id) return;
		if (event.type.startsWith('routine.') && workspace.kind !== 'local') return;
		if (event.type.startsWith('page.')) {
			const payload =
				event.type === 'page.deleted'
					? { type: event.type, workspaceId: event.workspaceId, pageId: event.pageId }
					: mapPageEvent(event as Parameters<typeof mapPageEvent>[0]);
			for (const [pluginId, plugin] of running) {
				if (
					!plugin.registered.event.has(event.type) ||
					event.actor === `plugin:${pluginId}` ||
					!packages.get(pluginId)?.manifest.permissions.includes('pages:read')
				)
					continue;
				void dispatch(pluginId, 'event', event.type, payload).catch(() => undefined);
			}
			return;
		}
		const { actor: _actor, ...basePayload } = event as DomainEvent & {
			task?: unknown;
			reactions?: unknown;
			routine?: unknown;
			instance?: unknown;
		};
		// Reacted/users are actor-relative; a broadcast has no single actor, so no plugin gets them.
		if (
			event.type === 'comment.reactionChanged' &&
			Array.isArray(basePayload.reactions)
		) {
			basePayload.reactions = basePayload.reactions.map((r: any) => ({
				emoji: r?.emoji,
				count: r?.count,
			}));
		}
		const task =
			basePayload.task && typeof basePayload.task === 'object'
				? { ...basePayload.task, key: taskKey(basePayload.task) }
				: basePayload.task;
		const routine =
			basePayload.routine && typeof basePayload.routine === 'object'
				? toRoutine(basePayload.routine)
				: undefined;
		const instance =
			basePayload.instance && typeof basePayload.instance === 'object'
				? toRoutineInstance(basePayload.instance)
				: undefined;
		for (const [pluginId, plugin] of running) {
			if (
				!plugin.registered.event.has(event.type) ||
				event.actor === `plugin:${pluginId}`
			)
				continue;
			if (!PLUGIN_EVENTS[event.type]) continue;
			const permissions = packages.get(pluginId)?.manifest.permissions ?? [];
			let payloadTask = task;
			// A task snapshot rides along with timer and status events; it is only for tasks:read.
			if (!permissions.includes('tasks:read')) {
				payloadTask = undefined;
			} else if (payloadTask && typeof payloadTask === 'object' && !permissions.includes('relations:read')) {
				const { relationTypeWithTask, ...rest } = payloadTask as Record<string, unknown>;
				payloadTask = rest;
			}
			const payload: Record<string, unknown> = { ...basePayload, task: payloadTask };
			if (payloadTask === undefined) delete payload.task;
			if (routine !== undefined) payload.routine = routine;
			else delete payload.routine;
			if (instance !== undefined) payload.instance = instance;
			else delete payload.instance;
			if (event.type === 'routine.deleted' && !permissions.includes('tasks:read')) delete payload.taskId;
			void dispatch(pluginId, 'event', event.type, payload).catch(
				() => undefined,
			);
		}
	});

	const stopAll = () =>
		[...running.keys()].forEach((pluginId) => stop(pluginId));

	const host = {
		state,
		async load() {
			for (const pkg of await deps.packages()) {
				packages.set(pkg.manifest.id, pkg);
				const entry = state.plugins[pkg.manifest.id];
				if (entry) {
					entry.manifest = pkg.manifest;
					entry.source = pkg.source;
					entry.origin = pkg.origin;
				} else {
					state.plugins[pkg.manifest.id] = {
						manifest: pkg.manifest,
						source: pkg.source,
						origin: pkg.origin,
						status: 'stopped',
						error: null,
						log: [],
					};
				}
			}
		},
		/** Plugins never run in safe mode; which ones run in a workspace is up to `enabled`. */
		async activate(workspace: PluginWorkspace | null) {
			stopAll();
			faults.clear();
			state.workspace = workspace;
			if (!workspace || state.safeMode) return;
			await Promise.all(
				[...packages.values()]
					.filter((pkg) => isEnabled(pkg.manifest.id, workspace))
					.map((pkg) => start(pkg, workspace, true)),
			);
		},
		isEnabled(pluginId: string) {
			return state.workspace ? isEnabled(pluginId, state.workspace) : false;
		},
		async setEnabled(pluginId: string, value: boolean) {
			const workspace = state.workspace;
			const pkg = packages.get(pluginId);
			if (!workspace || !pkg) return;
			deps.enabled.set(pluginId, workspace.id, value);
			stop(pluginId);
			if (!value) void deps.revokePluginTokens?.(pluginId);
			if (value && !state.safeMode && isEnabled(pluginId, workspace))
				await start(pkg, workspace);
		},
		async restart(pluginId: string) {
			const pkg = packages.get(pluginId);
			stop(pluginId);
			faults.delete(pluginId);
			if (
				pkg &&
				state.workspace &&
				!state.safeMode &&
				isEnabled(pluginId, state.workspace)
			) {
				await start(pkg, state.workspace);
			}
		},
		/** Stops what the blocklist now names, and starts again what it no longer names. */
		applyBlocklist() {
			const workspace = state.workspace;
			for (const [pluginId, entry] of Object.entries(state.plugins)) {
				const reason = blockedReason(pluginId);
				if (reason && entry.status !== 'blocked') {
					const wasRunning = running.has(pluginId);
					stop(pluginId, 'blocked', reason);
					if (wasRunning)
						deps.notify(
							`Plugin ${entry.manifest.name} was turned off`,
							`TMGR blocked it: ${reason.slice(0, 300)}`,
						);
				} else if (!reason && entry.status === 'blocked') {
					stop(pluginId);
					const pkg = packages.get(pluginId);
					if (
						pkg &&
						workspace &&
						!state.safeMode &&
						isEnabled(pluginId, workspace)
					)
						void start(pkg, workspace);
				}
			}
		},
		settings: settingsOf,
		async saveSettings(pluginId: string, values: Record<string, unknown>) {
			deps.settings.set(pluginId, values);
			if (running.has(pluginId)) await host.restart(pluginId);
		},
		commands() {
			return [...running.entries()].flatMap(([pluginId, plugin]) =>
				packages
					.get(pluginId)!
					.manifest.contributes.commands.filter((c) =>
						plugin.registered.command.has(c.id),
					)
					.map((c) => ({ pluginId, ...c })),
			);
		},
		taskMenuItems() {
			return resolveTaskMenuItems(
				[...packages.values()].map(({ manifest }) => ({
					manifest,
					running:
						running.has(manifest.id) &&
						state.plugins[manifest.id]?.status === 'running',
					registered: running.get(manifest.id)?.registered.command ?? new Set(),
				})),
			);
		},
		/** A click on a plugin's task menu item: runs that item's command with `{ taskId, workspaceId }`. */
		async runTaskMenuCommand(pluginId: string, commandId: string, taskId: number) {
			const pkg = packages.get(pluginId);
			const offered =
				!!pkg &&
				pkg.manifest.permissions.includes('menus:task') &&
				pkg.manifest.contributes.menus[TASK_MENU_LOCATION].some(
					(item) => item.command === commandId,
				);
			if (!offered || !Number.isSafeInteger(taskId) || taskId <= 0) {
				throw new PluginError(
					'NOT_DECLARED',
					`${commandId} is not a task menu item of ${pluginId}`,
				);
			}
			return host.runCommand(pluginId, commandId, {
				taskId,
				workspaceId: state.workspace?.id ?? null,
			});
		},
		async runCommand(
			pluginId: string,
			commandId: string,
			args: unknown = null,
		) {
			const plugin = running.get(pluginId);
			// A plugin that is not running (or already gone) is not a security question, just a no-op.
			if (!plugin || state.plugins[pluginId]?.status !== 'running') {
				return dispatch(pluginId, 'command', commandId, args);
			}
			const declared = packages
				.get(pluginId)!
				.manifest.contributes.commands.some((c) => c.id === commandId);
			if (!declared || !plugin.registered.command.has(commandId)) {
				throw new PluginError(
					'NOT_DECLARED',
					`${commandId} is not a command of ${pluginId}`,
				);
			}
			plugin.broker.admit(true);
			return dispatch(pluginId, 'command', commandId, args);
		},
		pages() {
			return [...running.keys()].flatMap((pluginId) =>
				packages
					.get(pluginId)!
					.manifest.contributes.views.map((view) => ({ pluginId, ...view })),
			);
		},
		sections() {
			return [...running.entries()].flatMap(([pluginId, plugin]) =>
				packages
					.get(pluginId)!
					.manifest.contributes.taskPanelSections.filter((s) =>
						plugin.registered.section.has(s.id),
					)
					.map((section) => ({ pluginId, ...section })),
			);
		},
		/** A view with its own HTML page opens in a separate window: a busy page cannot freeze the app. */
		/** Stops a removed plugin and drops it from the list. */
		forget(pluginId: string) {
			const pkg = packages.get(pluginId);
			const storageId = pkg ? storageIdOf(pkg) : pluginId;
			stop(pluginId);
			packages.delete(pluginId);
			delete state.plugins[pluginId];
			delete state.revisions[pluginId];
			startedThisSession.delete(pluginId);
			lastWorkspaceOf.delete(storageId);
			notifyTimestamps.delete(pluginId);
		},
		tick,
		resolveNotificationClick,
		resolveTrayClick,
		/** Clears the menu bar text when the user's choice in Settings no longer names who set it. */
		refreshTrayTitleOwner() {
			if (trayTitleSetBy && deps.trayTitleOwner?.() !== trayTitleSetBy) {
				state.trayTitle = null;
				trayTitleSetBy = null;
			}
		},
		/** The declared view, only while the plugin runs here with deeplinks and machine access. */
		deepLinkView(pluginId: string, viewId: string) {
			const plugin = running.get(pluginId);
			const pkg = packages.get(pluginId);
			if (!plugin || !pkg || state.plugins[pluginId]?.status !== 'running') return null;
			if (!pkg.manifest.permissions.includes('deeplinks') || !plugin.machine) return null;
			return pkg.manifest.contributes.views.find((v) => v.id === viewId) ?? null;
		},
		/** The declared, deep-linkable, registered command, only while the plugin runs here with deeplinks and machine access. */
		deepLinkCommand(pluginId: string, commandId: string) {
			const plugin = running.get(pluginId);
			const pkg = packages.get(pluginId);
			if (!plugin || !pkg || state.plugins[pluginId]?.status !== 'running') return null;
			if (!pkg.manifest.permissions.includes('deeplinks') || !plugin.machine) return null;
			const command = pkg.manifest.contributes.commands.find((c) => c.id === commandId);
			if (!command?.deepLink || !plugin.registered.command.has(commandId)) return null;
			return command;
		},
		/** Runs a deep-linked command after the same checks; `expected` must still match, or it is refused. */
		async runDeepLinkCommand(
			pluginId: string,
			commandId: string,
			args: Record<string, string>,
			expected?: { generation: string | null; workspaceId: number | null },
		) {
			if (!host.deepLinkCommand(pluginId, commandId)) return false;
			if (expected && !isCurrentRun(pluginId, expected)) return false;
			running.get(pluginId)!.broker.admit(true);
			await dispatch(pluginId, 'command', commandId, args);
			return true;
		},
		/** Re-validates the view right before opening it, instead of trusting a check made before an earlier await. */
		async openDeepLinkView(
			pluginId: string,
			viewId: string,
			params: Record<string, string> | undefined,
			expected: { generation: string | null; workspaceId: number | null },
		) {
			const view = host.deepLinkView(pluginId, viewId);
			if (!view || !isCurrentRun(pluginId, expected)) return null;
			if (!view.ui) return { view };
			try {
				return (await host.openView(pluginId, viewId, params)) ? { view } : null;
			} catch {
				return null;
			}
		},
		async openView(
			pluginId: string,
			viewId: string,
			props?: Record<string, string>,
		) {
			const key = `${pluginId}/${viewId}`;
			const plugin = running.get(pluginId);
			const pkg = packages.get(pluginId);
			const view = pkg?.manifest.contributes.views.find((v) => v.id === viewId);
			const html = view?.ui ? pkg?.pages?.[view.ui] : undefined;
			if (!plugin || !view || !html || !deps.windows) {
				windowProps.delete(key);
				return false;
			}
			if (props) windowProps.set(key, props);
			else windowProps.delete(key);
			try {
				await deps.windows.open(key, html, view.title, plugin.generation);
			} catch (error) {
				windowProps.delete(key);
				throw error;
			}
			// The run may have ended while the window was opening; its window must not outlive it.
			if (running.get(pluginId)?.generation !== plugin.generation) {
				windowProps.delete(key);
				await deps.windows.close(pluginId).catch(() => undefined);
				return false;
			}
			return true;
		},
		generationOf(pluginId: string) {
			return running.get(pluginId)?.generation ?? null;
		},
		/** Callers must capture this, not a raw store getter: `isCurrentRun` compares against this normalization. */
		currentWorkspaceId() {
			return deps.currentWorkspaceId();
		},
		isCurrentRun,
		/** Calls from a plugin's window use that plugin's broker: the same permissions as its logic. */
		async windowCall(
			pluginId: string,
			generation: string,
			viewId: string,
			method: string,
			params: unknown,
		) {
			const plugin = running.get(pluginId);
			if (
				!plugin ||
				plugin.generation !== generation ||
				state.plugins[pluginId]?.status !== 'running'
			) {
				throw new PluginError('NOT_RUNNING', `${pluginId} is not running`);
			}
			if (method === 'register' || method === 'log') {
				throw new PluginError(
					'UNKNOWN_METHOD',
					`${method} is not available to plugin windows`,
				);
			}
			if (method === 'deepLink.params') {
				const key = `${pluginId}/${viewId}`;
				const props = windowProps.get(key) ?? null;
				windowProps.delete(key);
				return props;
			}
			if (method === 'commands.run') {
				const p = (params ?? {}) as { id?: unknown; args?: unknown };
				const declared = packages
					.get(pluginId)!
					.manifest.contributes.commands.some((c) => c.id === p.id);
				if (!declared || !plugin.registered.command.has(String(p.id))) {
					throw new PluginError(
						'NOT_DECLARED',
						`${String(p.id)} is not a command of ${pluginId}`,
					);
				}
				plugin.broker.admit(true);
				return dispatch(pluginId, 'command', String(p.id), p.args ?? null);
			}
			return callBroker(pluginId, plugin.broker, method, params);
		},
		async renderPage(
			pluginId: string,
			viewId: string,
			props: unknown = null,
		): Promise<UiNode | null> {
			return sanitizeTree(
				await dispatch(pluginId, 'page', viewId, props),
				linkContext(pluginId),
				packages.get(pluginId)?.manifest.apiMinor,
			);
		},
		async renderSection(
			pluginId: string,
			sectionId: string,
			task: unknown,
		): Promise<UiNode | null> {
			return sanitizeTree(
				await dispatch(pluginId, 'section', sectionId, task),
				linkContext(pluginId),
				packages.get(pluginId)?.manifest.apiMinor,
			);
		},
		/** One call per provider for the whole batch of visible cards, merged in a fixed order and then by priority. */
		async badges(
			tasks: { id: number }[],
		): Promise<Record<number, CardBadge[]>> {
			const ids = tasks.map((t) => t.id);
			const providers = [...running.entries()].flatMap(([pluginId, plugin]) =>
				[...plugin.registered.badges].map((badgeId) => ({ pluginId, badgeId })),
			);
			// Promise.all keeps this in provider order regardless of which dispatch settles first.
			const answers = await Promise.all(
				providers.map(async ({ pluginId, badgeId }) => {
					try {
						return {
							pluginId,
							badgeId,
							answer: await dispatch(pluginId, 'badges', badgeId, tasks),
						};
					} catch {
						return { pluginId, badgeId, answer: null };
					}
				}),
			);
			const result: Record<number, CardBadge[]> = {};
			// Keyed by plugin then task: the cap is per plugin in total, shared by all of its badge providers.
			const usedByPlugin: Record<string, Record<number, number>> = {};
			for (const { pluginId, badgeId, answer } of answers) {
				if (!answer || typeof answer !== 'object') continue;
				const used = (usedByPlugin[pluginId] ??= {});
				for (const taskId of ids) {
					if (!Object.prototype.hasOwnProperty.call(answer, taskId)) continue;
					const room = MAX_BADGES_PER_TASK - (used[taskId] ?? 0);
					if (room <= 0) continue;
					const raw = (answer as Record<number, any>)[taskId];
					const list = Array.isArray(raw) ? raw : raw ? [raw] : [];
					const parsed = list
						.filter((b) => b && typeof b.text === 'string')
						.slice(0, room)
						.map((b) => normalizeBadge(pluginId, badgeId, b));
					if (parsed.length) {
						(result[taskId] ??= []).push(...parsed);
						used[taskId] = (used[taskId] ?? 0) + parsed.length;
					}
				}
			}
			for (const list of Object.values(result)) {
				list.sort((a, b) => b.priority - a.priority);
			}
			return result;
		},
		/** Re-checks permission, domain and https at click time; never trusts a tree rendered earlier. */
		openLink(pluginId: string, url: string): boolean {
			const plugin = running.get(pluginId);
			if (!plugin || state.plugins[pluginId]?.status !== 'running') return false;
			if (!isLinkAllowed(url, linkContext(pluginId))) return false;
			try {
				plugin.broker.admit(false);
			} catch {
				return false;
			}
			deps.openExternal?.(url);
			return true;
		},
		dispose() {
			unsubscribe();
			refreshTimers.forEach((timer) => clearTimeout(timer));
			refreshTimers.clear();
			stopAll();
			pendingClicks.clear();
		},
	};
	return host;
};

export type PluginHost = ReturnType<typeof createPluginHost>;
