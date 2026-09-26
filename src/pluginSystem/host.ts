import type { DomainEvent } from '@/utils/domainEvents';
import {
	createBroker,
	PLUGIN_EVENTS,
	PluginError,
	type BrokerDeps,
	type DataApi,
	type FetchRequest,
	type FetchResponse,
	type PluginWorkspace,
	type RegistrationKind,
} from './broker';
import type { PluginManifest } from './manifest';
import {
	startPluginProcess,
	type PluginProcess,
	type WorkerEndpoint,
} from './process';
import { sanitizeTree, type Color, type UiNode } from './uiTree';

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
	origin?: { repo: string; tag: string; sha256: string; verified?: boolean };
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
	text: string;
	color: Color;
	tooltip: string | null;
}

export interface PluginHostState {
	workspace: PluginWorkspace | null;
	safeMode: boolean;
	plugins: Record<string, PluginEntry>;
	statusBar: Record<string, StatusBarEntry>;
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
	/** Why a plugin must not run (the signed blocklist); it wins over every other setting. */
	blocked?: (pluginId: string) => string | null;
	now?: () => number;
	cpuMs?: number;
	wallMs?: number;
}

const FAULT_LIMIT = 3;
const FAULT_WINDOW_MS = 5 * 60_000;
const LOG_LIMIT = 200;
const MESSAGE_LIMIT = 1000;
const REFRESH_THROTTLE_MS = 500;
const COLORS: Color[] = ['gray', 'green', 'yellow', 'red', 'blue'];

interface Running {
	process: PluginProcess;
	broker: ReturnType<typeof createBroker>;
	registered: Record<RegistrationKind, Set<string>>;
	/** Changes on every start: windows and calls of an earlier run are refused. */
	generation: string;
}

export const createPluginHost = (deps: PluginHostDeps) => {
	const { state } = deps;
	const now = deps.now ?? Date.now;
	const packages = new Map<string, PluginPackage>();
	const running = new Map<string, Running>();
	const faults = new Map<string, number[]>();
	let nextGeneration = 0;
	const refreshTimers = new Map<string, ReturnType<typeof setTimeout>>();

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
	) => {
		const entry = state.plugins[pluginId];
		if (!entry) return;
		entry.log.push({
			at: now(),
			level,
			message: message.slice(0, MESSAGE_LIMIT),
		});
		if (entry.log.length > LOG_LIMIT)
			entry.log.splice(0, entry.log.length - LOG_LIMIT);
	};

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

	const clearStatusBar = (pluginId: string) => {
		for (const key of Object.keys(state.statusBar)) {
			if (state.statusBar[key].pluginId === pluginId)
				delete state.statusBar[key];
		}
	};

	const stop = (
		pluginId: string,
		status: PluginStatus = 'stopped',
		error: string | null = null,
	) => {
		running.get(pluginId)?.process.stop();
		running.delete(pluginId);
		clearStatusBar(pluginId);
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

	const isEnabled = (pluginId: string, workspaceId: number) =>
		deps.enabled.get(pluginId, workspaceId) ??
		packages.get(pluginId)?.source === 'builtin';

	const start = async (pkg: PluginPackage, workspace: PluginWorkspace) => {
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
		const broker = createBroker({
			manifest,
			workspace,
			currentWorkspaceId: deps.currentWorkspaceId,
			// An installed plugin's data belongs to its repository: another author reusing the id gets none of it.
			api: deps.api(
				pluginId,
				workspace,
				pkg.origin ? `${pluginId}@github.com/${pkg.origin.repo}` : pluginId,
			),
			settings: () => settingsOf(pluginId),
			notify: (message) => deps.notify(`Plugin ${manifest.name}`, message),
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
			refresh: () => bump(pluginId),
			register: (kind, id) => {
				if (registered[kind].has(id)) return;
				registered[kind].add(id);
				if (kind !== 'event' && kind !== 'command') bump(pluginId);
			},
			log: (level, message) => log(pluginId, level, message),
			fetch: deps.fetch,
			files: deps.files?.(pluginId, workspace, manifest.name),
			now,
		});
		const process = startPluginProcess(pkg.code, {
			endpoint: deps.createEndpoint(),
			call: (method, params) => broker.call(method, params),
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
			generation: String(++nextGeneration),
		});
		state.plugins[pluginId].status = 'starting';
		state.plugins[pluginId].error = null;
		try {
			await process.ready;
			if (running.get(pluginId)?.process !== process) return;
			state.plugins[pluginId].status = 'running';
			log(pluginId, 'info', `started in ${workspace.name}`);
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

	const relaunch = async (pluginId: string) => {
		const pkg = packages.get(pluginId);
		const workspace = state.workspace;
		stop(pluginId);
		if (
			pkg &&
			workspace?.kind === 'local' &&
			!state.safeMode &&
			isEnabled(pluginId, workspace.id)
		) {
			await start(pkg, workspace);
		}
	};

	const unsubscribe = deps.subscribe((event) => {
		const workspace = state.workspace;
		if (!workspace || event.workspaceId !== workspace.id) return;
		for (const [pluginId, plugin] of running) {
			if (
				!plugin.registered.event.has(event.type) ||
				event.actor === `plugin:${pluginId}`
			)
				continue;
			if (!PLUGIN_EVENTS[event.type]) continue;
			const { actor: _actor, ...payload } = event as DomainEvent & {
				task?: unknown;
			};
			// A task snapshot rides along with timer and status events; it is only for tasks:read.
			if (
				!packages.get(pluginId)?.manifest.permissions.includes('tasks:read')
			) {
				delete (payload as { task?: unknown }).task;
			}
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
		/** Plugins run only inside a local workspace, and never in safe mode. */
		async activate(workspace: PluginWorkspace | null) {
			stopAll();
			faults.clear();
			state.workspace = workspace;
			if (!workspace || workspace.kind !== 'local' || state.safeMode) return;
			await Promise.all(
				[...packages.values()]
					.filter((pkg) => isEnabled(pkg.manifest.id, workspace.id))
					.map((pkg) => start(pkg, workspace)),
			);
		},
		isEnabled(pluginId: string) {
			return state.workspace ? isEnabled(pluginId, state.workspace.id) : false;
		},
		async setEnabled(pluginId: string, value: boolean) {
			const workspace = state.workspace;
			const pkg = packages.get(pluginId);
			if (!workspace || !pkg) return;
			deps.enabled.set(pluginId, workspace.id, value);
			stop(pluginId);
			if (value && workspace.kind === 'local' && !state.safeMode)
				await start(pkg, workspace);
		},
		async restart(pluginId: string) {
			const pkg = packages.get(pluginId);
			stop(pluginId);
			faults.delete(pluginId);
			if (
				pkg &&
				state.workspace?.kind === 'local' &&
				!state.safeMode &&
				isEnabled(pluginId, state.workspace.id)
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
						workspace?.kind === 'local' &&
						!state.safeMode &&
						isEnabled(pluginId, workspace.id)
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
		async runCommand(
			pluginId: string,
			commandId: string,
			args: unknown = null,
		) {
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
			stop(pluginId);
			packages.delete(pluginId);
			delete state.plugins[pluginId];
			delete state.revisions[pluginId];
		},
		async openView(pluginId: string, viewId: string) {
			const plugin = running.get(pluginId);
			const pkg = packages.get(pluginId);
			const view = pkg?.manifest.contributes.views.find((v) => v.id === viewId);
			const html = view?.ui ? pkg?.pages?.[view.ui] : undefined;
			if (!plugin || !view || !html || !deps.windows) return false;
			await deps.windows.open(
				`${pluginId}/${viewId}`,
				html,
				view.title,
				plugin.generation,
			);
			// The run may have ended while the window was opening; its window must not outlive it.
			if (running.get(pluginId)?.generation !== plugin.generation) {
				await deps.windows.close(pluginId).catch(() => undefined);
				return false;
			}
			return true;
		},
		generationOf(pluginId: string) {
			return running.get(pluginId)?.generation ?? null;
		},
		/** Calls from a plugin's window use that plugin's broker: the same permissions as its logic. */
		async windowCall(
			pluginId: string,
			generation: string,
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
			return plugin.broker.call(method, params);
		},
		async renderPage(
			pluginId: string,
			viewId: string,
			props: unknown = null,
		): Promise<UiNode | null> {
			return sanitizeTree(await dispatch(pluginId, 'page', viewId, props));
		},
		async renderSection(
			pluginId: string,
			sectionId: string,
			task: unknown,
		): Promise<UiNode | null> {
			return sanitizeTree(await dispatch(pluginId, 'section', sectionId, task));
		},
		/** One call per provider for the whole batch of visible cards. */
		async badges(
			tasks: { id: number }[],
		): Promise<Record<number, CardBadge[]>> {
			const ids = new Set(tasks.map((t) => t.id));
			const result: Record<number, CardBadge[]> = {};
			await Promise.all(
				[...running.entries()].flatMap(([pluginId, plugin]) =>
					[...plugin.registered.badges].map(async (badgeId) => {
						let answer: any;
						try {
							answer = await dispatch(pluginId, 'badges', badgeId, tasks);
						} catch {
							return;
						}
						if (!answer || typeof answer !== 'object') return;
						for (const taskId of ids) {
							const badge = Object.prototype.hasOwnProperty.call(answer, taskId)
								? (answer as Record<number, any>)[taskId]
								: null;
							if (!badge || typeof badge.text !== 'string') continue;
							(result[taskId] ??= []).push({
								pluginId,
								text: badge.text.slice(0, 16),
								color: COLORS.includes(badge.color) ? badge.color : 'gray',
								tooltip:
									typeof badge.tooltip === 'string'
										? badge.tooltip.slice(0, 200)
										: null,
							});
						}
					}),
				),
			);
			return result;
		},
		dispose() {
			unsubscribe();
			refreshTimers.forEach((timer) => clearTimeout(timer));
			refreshTimers.clear();
			stopAll();
		},
	};
	return host;
};

export type PluginHost = ReturnType<typeof createPluginHost>;
