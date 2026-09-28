'use strict';
// Runs a plugin's main.js in a real QuickJS sandbox with the app's own PRELUDE, broker and manifest
// validation (see sync-prelude.mjs: broker.generated.js, manifest.generated.js, sandbox.generated.js and
// uiTree.generated.js are transpiled straight from src/pluginSystem, so permission checks, param
// validation and UI sanitising behave exactly like the app). Backed by an in-memory mock of the app's
// data instead of the real API.
const fs = require('node:fs');
const { createBroker } = require('./broker.generated.js');
const { parseManifest } = require('./manifest.generated.js');
const { createSandbox } = require('./sandbox.generated.js');
const { sanitizeTree, COLORS } = require('./uiTree.generated.js');

const MAX_BADGES_PER_TASK = 5;
const BADGE_KEY = /^[a-z0-9_-]{1,40}$/;

const normalizeBadge = (badge) => ({
	text: String(badge.text).slice(0, 16),
	color: COLORS.includes(badge.color) ? badge.color : 'gray',
	tooltip: typeof badge.tooltip === 'string' ? badge.tooltip.slice(0, 200) : null,
	priority:
		typeof badge.priority === 'number' && Number.isFinite(badge.priority) ? badge.priority : 0,
	key: typeof badge.key === 'string' && BADGE_KEY.test(badge.key) ? badge.key : undefined,
});

const taskCategory = (state, task) =>
	task.project_category_id != null
		? state.categories.find((c) => c.id === task.project_category_id) ?? null
		: null;

const toWireTask = (state, task) => {
	const category = taskCategory(state, task);
	return {
		...task,
		category: category ? { code: category.code, title: category.title } : null,
		key:
			category?.code && task.category_tasks_sequence_id != null
				? `${category.code}-${task.category_tasks_sequence_id}`
				: null,
	};
};

const findTask = (state, id) => {
	const task = state.tasks.find((t) => t.id === id);
	if (!task) throw new Error(`task ${id} not found`);
	return task;
};

const findAgentWork = (state, runId) => {
	for (const list of Object.values(state.agentWork)) {
		const run = list.find((r) => r.id === runId);
		if (run) return run;
	}
	throw new Error(`agent work run ${runId} not found`);
};

/** Everything a plugin's main.js can reach through `tmgr`, backed by plain in-memory state. */
const createMockApi = (state, clock) => ({
	async listTasks(query) {
		let items = state.tasks;
		if (query.statusId != null) items = items.filter((t) => t.status_id === query.statusId);
		if (query.categoryId != null)
			items = items.filter((t) => t.project_category_id === query.categoryId);
		if (query.search) {
			const needle = query.search.toLowerCase();
			items = items.filter((t) => t.title.toLowerCase().includes(needle));
		}
		if (query.priority) items = items.filter((t) => t.priority === query.priority);
		if (query.statusType) {
			const ids = new Set(
				state.statuses.filter((s) => s.type === query.statusType).map((s) => s.id),
			);
			items = items.filter((t) => ids.has(t.status_id));
		}
		if (query.dueBefore) items = items.filter((t) => t.expired_at && t.expired_at < query.dueBefore);
		if (query.dueAfter) items = items.filter((t) => t.expired_at && t.expired_at > query.dueAfter);
		const perPage = query.perPage ?? 50;
		const page = query.page ?? 1;
		const start = (page - 1) * perPage;
		return {
			items: items.slice(start, start + perPage).map((t) => toWireTask(state, t)),
			total: items.length,
		};
	},
	async getTask(id) {
		return toWireTask(state, findTask(state, id));
	},
	async createTask(fields) {
		const category = fields.project_category_id != null
			? state.categories.find((c) => c.id === fields.project_category_id)
			: null;
		const task = {
			id: state.nextTaskId++,
			title: fields.title,
			description: fields.description ?? null,
			status_id: fields.status_id ?? null,
			project_category_id: fields.project_category_id ?? null,
			priority: fields.priority ?? null,
			approximately_time: fields.approximately_time ?? 0,
			common_time: 0,
			start_time: 0,
			expired_at: fields.expired_at ?? null,
			category_tasks_sequence_id: category ? ++category.nextSequence : null,
		};
		state.tasks.push(task);
		return toWireTask(state, task);
	},
	async updateTask(id, patch) {
		Object.assign(findTask(state, id), patch);
		return toWireTask(state, findTask(state, id));
	},
	async listStatuses() {
		return state.statuses.map((s) => ({ ...s }));
	},
	async listCategories() {
		return state.categories.map(({ nextSequence, ...c }) => c);
	},
	async createStatus(fields) {
		const status = {
			id: state.nextStatusId++,
			name: fields.name,
			type: fields.type,
			color: fields.color ?? null,
		};
		state.statuses.push(status);
		return { ...status };
	},
	async updateStatus(id, patch) {
		const status = state.statuses.find((s) => s.id === id);
		if (!status) throw new Error(`status ${id} not found`);
		Object.assign(status, patch);
		return { ...status };
	},
	async reorderStatuses(ids) {
		return { ids };
	},
	async createCategory(fields) {
		const category = {
			id: state.nextCategoryId++,
			title: fields.title,
			code: fields.code ?? null,
			nextSequence: 0,
		};
		state.categories.push(category);
		const { nextSequence, ...wire } = category;
		return wire;
	},
	async updateCategory(id, patch) {
		const category = state.categories.find((c) => c.id === id);
		if (!category) throw new Error(`category ${id} not found`);
		Object.assign(category, patch);
		const { nextSequence, ...wire } = category;
		return wire;
	},
	async startTimer(taskId) {
		findTask(state, taskId).start_time = Math.floor(clock() / 1000);
		return { ok: true };
	},
	async stopTimer(taskId) {
		const task = findTask(state, taskId);
		if (task.start_time) {
			task.common_time += Math.floor(clock() / 1000) - task.start_time;
			task.start_time = 0;
		}
		return { ok: true };
	},
	async listComments(taskId) {
		return (state.comments[taskId] ?? []).map((c) => ({ ...c }));
	},
	async addComment(taskId, text) {
		findTask(state, taskId);
		const comment = { id: state.nextCommentId++, task_id: taskId, text, is_plugin: true, reactions: {} };
		(state.comments[taskId] ??= []).push(comment);
		return { ...comment };
	},
	async reactToComment(commentId, emoji) {
		for (const list of Object.values(state.comments)) {
			const comment = list.find((c) => c.id === commentId);
			if (comment) {
				comment.reactions[emoji] = (comment.reactions[emoji] ?? 0) + 1;
				return { ...comment };
			}
		}
		throw new Error(`comment ${commentId} not found`);
	},
	async listRelations(taskId) {
		return state.relations[taskId] ?? [];
	},
	async relateTask(taskId, otherId, type) {
		(state.relations[taskId] ??= []).push({ taskId: otherId, type });
		return { ok: true };
	},
	async unrelateTask(taskId, otherId, type) {
		state.relations[taskId] = (state.relations[taskId] ?? []).filter(
			(r) => !(r.taskId === otherId && r.type === type),
		);
		return { ok: true };
	},
	async storageGet(key) {
		return state.storage[key] ?? null;
	},
	async storageSet(key, json) {
		state.storage[key] = json;
		return { ok: true };
	},
	async storageDelete(key) {
		delete state.storage[key];
		return { ok: true };
	},
	async storageKeys() {
		return Object.keys(state.storage);
	},
	async listAttachments(taskId) {
		return state.attachments[taskId] ?? [];
	},
	async readAttachment(fileId) {
		throw new Error(`file ${fileId} not found`);
	},
	async taskDataGet(taskId, key) {
		return state.taskData[`${taskId}:${key}`] ?? null;
	},
	async taskDataSet(taskId, key, json) {
		state.taskData[`${taskId}:${key}`] = json;
		return { ok: true };
	},
	async taskDataDelete(taskId, key) {
		delete state.taskData[`${taskId}:${key}`];
		return { ok: true };
	},
	async taskDataGetMany(taskIds, key) {
		const result = {};
		for (const id of taskIds) result[id] = state.taskData[`${id}:${key}`] ?? null;
		return result;
	},
	async listAgentWork(taskId) {
		return state.agentWork[taskId] ?? [];
	},
	async startAgentWork(taskId, fields) {
		findTask(state, taskId);
		const run = { id: state.nextAgentWorkId++, task_id: taskId, status: 'running', ...fields };
		(state.agentWork[taskId] ??= []).push(run);
		return { ...run };
	},
	async updateAgentWork(runId, patch) {
		Object.assign(findAgentWork(state, runId), patch);
		return { ...findAgentWork(state, runId) };
	},
	async finishAgentWork(runId, patch) {
		Object.assign(findAgentWork(state, runId), patch);
		return { ...findAgentWork(state, runId) };
	},
});

const computeBadges = async (sandbox, badgeIds, tasks) => {
	const ids = tasks.map((t) => t.id);
	const result = {};
	for (const badgeId of badgeIds) {
		let answer;
		try {
			answer = await sandbox.dispatch('badges', badgeId, tasks);
		} catch {
			continue;
		}
		if (!answer || typeof answer !== 'object') continue;
		for (const taskId of ids) {
			if (!Object.prototype.hasOwnProperty.call(answer, taskId)) continue;
			const raw = answer[taskId];
			const list = Array.isArray(raw) ? raw : raw ? [raw] : [];
			const parsed = list
				.filter((b) => b && typeof b.text === 'string')
				.slice(0, MAX_BADGES_PER_TASK)
				.map(normalizeBadge);
			if (parsed.length) (result[taskId] ??= []).push(...parsed);
		}
	}
	for (const list of Object.values(result)) list.sort((a, b) => b.priority - a.priority);
	return result;
};

/** Real Node has no trouble loading the wasm variant directly; only Jest's CommonJS transform does. */
const loadDefaultQuickJS = () => {
	const variant = require('@jitl/quickjs-wasmfile-release-sync');
	const { newQuickJSWASMModuleFromVariant } = require('quickjs-emscripten-core');
	return newQuickJSWASMModuleFromVariant(variant);
};

const createTestHost = async (options = {}) => {
	const manifest = parseManifest(options.manifest);
	const code = options.code ?? fs.readFileSync(options.mainPath, 'utf8');
	const quickjs = await (options.quickjs ?? loadDefaultQuickJS());

	let currentTime = typeof options.now === 'number' ? options.now : Date.now();
	const clock = () => currentTime;

	const state = {
		nextTaskId: 1,
		nextStatusId: 1,
		nextCategoryId: 1,
		nextCommentId: 1,
		nextAgentWorkId: 1,
		tasks: [],
		statuses: [],
		categories: [],
		comments: {},
		relations: {},
		taskData: {},
		storage: {},
		agentWork: {},
		attachments: {},
		files: {},
		alarms: {},
		notifications: [],
		statusBar: {},
		trayItems: {},
		trayTitle: null,
		log: [],
	};
	for (const s of options.statuses ?? []) {
		state.statuses.push({ id: state.nextStatusId++, name: s.name, type: s.type ?? 'default', color: s.color ?? null });
	}
	for (const c of options.categories ?? []) {
		state.categories.push({ id: state.nextCategoryId++, title: c.title, code: c.code ?? null, nextSequence: 0 });
	}
	for (const t of options.tasks ?? []) {
		state.tasks.push({
			id: state.nextTaskId++,
			title: t.title,
			description: t.description ?? null,
			status_id: t.status_id ?? null,
			project_category_id: t.project_category_id ?? null,
			priority: t.priority ?? null,
			approximately_time: t.approximately_time ?? 0,
			common_time: t.common_time ?? 0,
			start_time: t.start_time ?? 0,
			expired_at: t.expired_at ?? null,
			category_tasks_sequence_id: t.category_tasks_sequence_id ?? null,
		});
	}

	const registered = { event: new Set(), command: new Set(), badges: new Set(), page: new Set(), section: new Set() };
	const workspace = {
		id: -1,
		code: options.workspaceCode ?? 'test',
		name: options.workspaceName ?? 'Test Workspace',
		kind: 'local',
	};
	const trayTitleOwner = options.trayTitleOwner ?? true;
	const linkContext = {
		allowedDomains: manifest.links.allowedDomains,
		linksOpen: manifest.permissions.includes('links:open'),
	};

	const broker = createBroker({
		manifest,
		workspace,
		currentWorkspaceId: () => workspace.id,
		api: createMockApi(state, clock),
		settings: () => ({ ...(options.settings ?? {}) }),
		notify: (payload) => state.notifications.push({ ...payload }),
		setStatusBarItem: (id, item) => {
			if (item) state.statusBar[id] = item;
			else delete state.statusBar[id];
		},
		refresh: () => undefined,
		register: (kind, id) => registered[kind]?.add(id),
		log: (level, message) => state.log.push({ at: clock(), level, message }),
		now: clock,
		files: {
			export: async (path, content) => {
				state.files[path] = content;
				return { path };
			},
			reveal: async () => undefined,
			pick: async () => options.filesPick ?? null,
		},
		alarms: {
			create: (name, spec) => {
				state.alarms[name] = { name, scheduledAtMs: spec.scheduledAtMs, periodMinutes: spec.periodMinutes };
				return { name, scheduledAt: new Date(spec.scheduledAtMs).toISOString() };
			},
			clear: (name) => delete state.alarms[name],
			list: () =>
				Object.values(state.alarms).map((a) => ({
					name: a.name,
					scheduledAt: new Date(a.scheduledAtMs).toISOString(),
				})),
		},
		dnd: () => (typeof options.dnd === 'function' ? options.dnd() : options.dnd ?? { active: false, until: null }),
		tray: {
			setItem: (itemId, item) => {
				if (item) state.trayItems[itemId] = item;
				else delete state.trayItems[itemId];
			},
			setTitle: (text) => (state.trayTitle = text),
			isTitleOwner: () => trayTitleOwner,
		},
		fetch: options.fetch,
		localAccess: {
			requestConnection: async () => options.localAccessConnect ?? { status: 'cancelled' },
		},
	});

	const calls = [];
	const sandbox = createSandbox({
		quickjs,
		code,
		cpuMs: options.cpuMs,
		wallMs: options.wallMs,
		call: async (method, params) => {
			calls.push([method, params]);
			return broker.call(method, params);
		},
	});
	await sandbox.start();

	return {
		manifest,
		tmgr: state,
		registered,
		calls,
		emit: (event) => {
			const { type, ...payload } = event;
			return sandbox.dispatch('event', type, payload);
		},
		runCommand: (id, args) => sandbox.dispatch('command', id, args ?? null),
		renderPage: async (id, props) =>
			sanitizeTree(await sandbox.dispatch('page', id, props ?? null), linkContext),
		renderSection: async (id, task) =>
			sanitizeTree(await sandbox.dispatch('section', id, task ?? null), linkContext),
		badges: (tasks) => computeBadges(sandbox, registered.badges, tasks),
		fireAlarms: (atTime) => {
			if (typeof atTime === 'number') currentTime = atTime;
			const due = Object.values(state.alarms).filter((a) => a.scheduledAtMs <= currentTime);
			for (const def of due) {
				if (def.periodMinutes) {
					state.alarms[def.name] = { ...def, scheduledAtMs: currentTime + def.periodMinutes * 60_000 };
				} else {
					delete state.alarms[def.name];
				}
			}
			return Promise.all(
				due.map((def) =>
					sandbox
						.dispatch('event', 'alarm', { name: def.name, scheduledAt: new Date(def.scheduledAtMs).toISOString() })
						.catch(() => undefined),
				),
			);
		},
		dispose: () => sandbox.dispose(),
	};
};

module.exports = { createTestHost };
