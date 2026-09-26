import type { DomainEvent } from '@/utils/domainEvents';
import {
	createBroker,
	PLUGIN_EVENTS,
	type DataApi,
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

export type PluginSource = 'builtin' | 'folder';
export type PluginStatus =
	| 'stopped'
	| 'starting'
	| 'running'
	| 'failed'
	| 'crashed';

export interface PluginPackage {
	manifest: PluginManifest;
	code: string;
	source: PluginSource;
}

export interface PluginLogLine {
	at: number;
	level: 'info' | 'warn' | 'error';
	message: string;
}

export interface PluginEntry {
	manifest: PluginManifest;
	source: PluginSource;
	status: PluginStatus;
	error: string | null;
	log: PluginLogLine[];
}

export interface StatusBarEntry {
	pluginId: string;
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
	/** Bumped whenever badges, pages or sections should be asked again. */
	revision: number;
}

export interface PluginHostDeps {
	state: PluginHostState;
	packages: () => Promise<PluginPackage[]>;
	createEndpoint: () => WorkerEndpoint;
	api: (pluginId: string) => DataApi;
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
	now?: () => number;
	cpuMs?: number;
	wallMs?: number;
}

const FAULT_LIMIT = 3;
const FAULT_WINDOW_MS = 5 * 60_000;
const LOG_LIMIT = 200;
const COLORS: Color[] = ['gray', 'green', 'yellow', 'red', 'blue'];

interface Running {
	process: PluginProcess;
	registered: Record<RegistrationKind, Set<string>>;
}

export const createPluginHost = (deps: PluginHostDeps) => {
	const { state } = deps;
	const now = deps.now ?? Date.now;
	const packages = new Map<string, PluginPackage>();
	const running = new Map<string, Running>();
	const faults = new Map<string, number[]>();

	const log = (
		pluginId: string,
		level: PluginLogLine['level'],
		message: string,
	) => {
		const entry = state.plugins[pluginId];
		if (!entry) return;
		entry.log.push({ at: now(), level, message });
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
		const entry = state.plugins[pluginId];
		if (entry) {
			entry.status = status;
			entry.error = error;
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
				`${name} was turned off`,
				`It failed ${FAULT_LIMIT} times in 5 minutes: ${reason}`,
			);
		}
	};

	const isEnabled = (pluginId: string, workspaceId: number) =>
		deps.enabled.get(pluginId, workspaceId) ??
		packages.get(pluginId)?.source === 'builtin';

	const start = async (pkg: PluginPackage, workspace: PluginWorkspace) => {
		const { manifest } = pkg;
		const pluginId = manifest.id;
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
			api: deps.api(pluginId),
			settings: () => settingsOf(pluginId),
			notify: (message) => deps.notify(manifest.name, message),
			setStatusBarItem: (itemId, item) => {
				const key = `${pluginId}:${itemId}`;
				if (item) state.statusBar[key] = { pluginId, itemId, ...item };
				else delete state.statusBar[key];
			},
			refresh: () => void state.revision++,
			register: (kind, id) => {
				registered[kind].add(id);
				if (kind !== 'event' && kind !== 'command') state.revision++;
			},
			log: (level, message) => log(pluginId, level, message),
			now,
		});
		const process = startPluginProcess(pkg.code, {
			endpoint: deps.createEndpoint(),
			call: (method, params) => broker.call(method, params),
			onCrash: (reason) => fault(pluginId, reason),
			cpuMs: deps.cpuMs,
			wallMs: deps.wallMs,
		});
		running.set(pluginId, { process, registered });
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
			throw error;
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
			const { actor: _actor, ...payload } = event;
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
				state.plugins[pkg.manifest.id] ??= {
					manifest: pkg.manifest,
					source: pkg.source,
					status: 'stopped',
					error: null,
					log: [],
				};
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
						for (const [key, badge] of Object.entries<any>(answer)) {
							const taskId = Number(key);
							if (!ids.has(taskId) || !badge || typeof badge.text !== 'string')
								continue;
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
			stopAll();
		},
	};
	return host;
};

export type PluginHost = ReturnType<typeof createPluginHost>;
