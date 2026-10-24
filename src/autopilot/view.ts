import type {
	AutopilotInput,
	AutopilotView,
	ButtonsView,
	CostView,
	CounterItem,
	CounterView,
	EventView,
	HeaderView,
	LimitView,
	RunView,
	ViewOptions,
} from './types';

type Loose = Record<string, unknown>;

interface PrLink {
	task: string;
	url: string | null;
}

const EMPTY_TEXT = 'Autopilot is not running.';
const EMPTY_COMMAND = 'tmgr queue autopilot';
const KNOWN_STATES = ['running', 'paused', 'stopping', 'stopped', 'suspended'];
const EVENT_LIMIT = 50;
const STALE_MS = 30000;
const MAX_URL = 2048;
const CONTROL = /[\u0000-\u001f\u007f-\u009f]/g;
const BIDI = /[‪-‮⁦-⁩]/g;
const LINE_BREAKS = /[\t\n\r]/g;

const rec = (v: unknown): Loose =>
	typeof v === 'object' && v !== null && !Array.isArray(v) ? (v as Loose) : {};

const list = (v: unknown): Loose[] => (Array.isArray(v) ? v.map(rec) : []);

const num = (v: unknown): number | null =>
	typeof v === 'number' && Number.isFinite(v) ? v : null;

const int = (v: unknown): number => num(v) ?? 0;

const timeMs = (v: unknown): number | null => {
	if (typeof v !== 'string') return null;
	const t = Date.parse(v);
	return Number.isNaN(t) ? null : t;
};

const pad = (n: number): string => String(n).padStart(2, '0');

export const clock = (iso: string, tzMinutes?: number): string => {
	const t = Date.parse(iso);
	if (Number.isNaN(t)) return '--:--';
	const offset = tzMinutes ?? -new Date(t).getTimezoneOffset();
	const shifted = new Date(t + offset * 60000);
	return `${pad(shifted.getUTCHours())}:${pad(shifted.getUTCMinutes())}`;
};

export const span = (secs: number): string => {
	if (!Number.isFinite(secs)) return '—';
	const s = Math.max(0, Math.floor(secs));
	const h = Math.floor(s / 3600);
	const m = Math.floor((s % 3600) / 60);
	if (h > 0) return `${h}h ${m}m`;
	if (m > 0) return `${m}m`;
	return `${s}s`;
};

export const money = (v: number | null | undefined): string => {
	if (v === null || v === undefined || !Number.isFinite(v)) return 'n/a';
	if (Math.abs(v - Math.round(v)) < 0.005) return `$${Math.round(v)}`;
	return `$${v.toFixed(2)}`;
};

export const toolName = (tool: string): string =>
	tool.charAt(0).toUpperCase() + tool.slice(1);

export const windowName = (w: string): string => {
	if (w === 'five_hour') return '5h';
	if (w === 'seven_day') return 'week';
	return w;
};

export const cleanText = (s: string, max: number): string => {
	const flat = s
		.replace(LINE_BREAKS, ' ')
		.replace(CONTROL, '')
		.replace(BIDI, '');
	const chars = Array.from(flat);
	if (chars.length <= max) return flat;
	return `${chars.slice(0, Math.max(0, max - 1)).join('')}…`;
};

export const safeHttpsUrl = (url: unknown): string | null => {
	if (typeof url !== 'string' || url.length > MAX_URL) return null;
	let parsed: URL;
	try {
		parsed = new URL(url);
	} catch {
		return null;
	}
	if (
		parsed.protocol !== 'https:' ||
		parsed.hostname === '' ||
		parsed.username !== '' ||
		parsed.password !== ''
	)
		return null;
	const normalized = parsed.toString();
	return normalized.length <= MAX_URL ? normalized : null;
};

const text = (v: unknown, max: number): string =>
	typeof v === 'string' ? cleanText(v, max) : '';

const optText = (v: unknown, max: number): string | null => {
	const t = text(v, max);
	return t === '' ? null : t;
};

const ageSpan = (nowMs: number, v: unknown): string | null => {
	const t = timeMs(v);
	return t === null ? null : span((nowMs - t) / 1000);
};

const slots = (v: unknown): string => {
	const s = rec(v);
	return `${int(s.running)}/${int(s.max)}`;
};

const buttonsFor = (running: boolean, state: string): ButtonsView => {
	const live = running && state !== 'stopping';
	return {
		pause: live && state === 'running',
		resume: live && state === 'paused',
		stop: live && (state === 'running' || state === 'paused'),
	};
};

const startingHeader = (): HeaderView => ({
	title: 'Autopilot',
	word: 'starting',
	note: null,
	needsUpdate: false,
	startedAt: null,
	runningFor: null,
	rule: null,
	mode: null,
	stopReason: null,
	stale: false,
	agents: null,
	teams: null,
});

const statusHeader = (
	d: Loose,
	running: boolean,
	nowMs: number,
	tzMinutes: number | undefined,
): HeaderView => {
	const state = typeof d.state === 'string' ? d.state : '';
	const needsUpdate = d.version !== 1 || !KNOWN_STATES.includes(state);
	const stopped = state === 'stopped';
	const updatedMs = timeMs(d.updated_at);
	const startedMs = timeMs(d.started_at);

	let word = state ? cleanText(state, 32) : '—';
	if (!needsUpdate) {
		if (running) word = state;
		else word = stopped ? 'stopped' : 'not running';
	}

	const lastState =
		!needsUpdate && !running && !stopped
			? `last state ${state}${
					updatedMs === null
						? ''
						: `, updated ${span((nowMs - updatedMs) / 1000)} ago`
				}`
			: null;

	return {
		title: d.kind === 'queue' ? 'Queue loop' : 'Autopilot',
		word,
		note: lastState,
		needsUpdate,
		startedAt:
			startedMs === null ? null : clock(d.started_at as string, tzMinutes),
		runningFor: startedMs === null ? null : span((nowMs - startedMs) / 1000),
		rule: optText(d.rule, 120),
		mode: optText(d.mode, 120),
		stopReason: stopped ? optText(d.stop_reason, 200) : null,
		stale: running && updatedMs !== null && nowMs - updatedMs > STALE_MS,
		agents: slots(d.agents),
		teams: slots(d.teams),
	};
};

const runAgents = (v: unknown): string | undefined => {
	if (typeof v === 'number' && Number.isFinite(v)) return String(v);
	if (typeof v === 'string') return cleanText(v, 200);
	if (Array.isArray(v)) {
		const names = v.filter((x): x is string => typeof x === 'string');
		return names.map((x) => cleanText(x, 200)).join(', ');
	}
	return undefined;
};

const reasonItems = (v: unknown): CounterItem[] | null =>
	Array.isArray(v)
		? list(v).map((r) => ({
				task: text(r.task, 64),
				reason: text(r.reason, 300),
			}))
		: null;

const limitView = (
	raw: Loose,
	nowMs: number,
	tzMinutes?: number,
): LimitView => {
	const label = `${toolName(text(raw.tool, 32))} ${windowName(
		text(raw.window, 32),
	)}`;
	const source = text(raw.source, 32);
	const age = ageSpan(nowMs, raw.observed_at);
	const ageSuffix = age === null ? '' : ` ${age} ago`;
	const pct = num(raw.used_pct);
	const soft = num(raw.stop_new_at);
	const hard = num(raw.pause_at);
	const marks = soft !== null && hard !== null ? { soft, hard } : null;
	const resetsMs = timeMs(raw.resets_at);
	const resets =
		resetsMs === null ? null : clock(raw.resets_at as string, tzMinutes);

	let body = `${label}: unknown${ageSuffix}`;
	if (pct !== null) {
		body = `${label}: ${Math.round(pct)}% ${source}${ageSuffix}`;
		if (marks) body += ` (stop new at ${marks.soft}%, pause at ${marks.hard}%)`;
		if (resets !== null) body += ` · resets ${resets}`;
	}

	return {
		label,
		text: body,
		usedPct: pct === null ? null : Math.round(pct),
		source,
		soft: marks?.soft ?? null,
		hard: marks?.hard ?? null,
		resets,
		age,
	};
};

const costView = (d: Loose): CostView => {
	const c = rec(d.cost);
	const subscriptionUsd = num(c.subscription_usd) ?? 0;
	const apiUsd = num(c.api_usd);
	const cap = num(c.api_cap_usd);
	let api = 'n/a';
	if (apiUsd !== null && cap !== null)
		api = `${money(apiUsd)} of ${money(cap)} cap`;
	else if (apiUsd !== null) api = money(apiUsd);
	else if (cap !== null) api = `none yet, ${money(cap)} cap`;
	return {
		subscription: money(subscriptionUsd),
		basis: subscriptionUsd > 0 ? optText(c.basis, 200) : null,
		api,
		tokens: int(c.subscription_tokens),
	};
};

const eventView = (e: Loose, tzMinutes?: number): EventView => {
	const url = safeHttpsUrl(e.url);
	const view: EventView = {
		at: clock(typeof e.at === 'string' ? e.at : '', tzMinutes),
		kind: text(e.kind, 64),
		task: optText(e.task, 64),
		text: text(e.text, 500),
	};
	if (url) view.url = url;
	return view;
};

export const buildView = (
	input: AutopilotInput,
	nowMs: number,
	opts: ViewOptions = {},
): AutopilotView => {
	const tz = opts.tzMinutes;
	const hasStatus = input.status !== null && input.status !== undefined;
	const d = rec(input.status);
	const state = typeof d.state === 'string' ? d.state : '';
	const tasks = rec(d.tasks);
	const resume = rec(d.resume);
	const summary = rec(d.summary);

	const prs: PrLink[] = list(d.prs).map((p) => ({
		task: text(p.task, 64),
		url: safeHttpsUrl(p.url),
	}));
	const prByTask = new Map<string, string>();
	for (const p of prs)
		if (p.url && !prByTask.has(p.task)) prByTask.set(p.task, p.url);

	const runs: RunView[] = list(d.runs).map((r) => {
		const task = text(r.task, 64);
		const startedMs = timeMs(r.started_at);
		const view: RunView = {
			task,
			taskId: num(r.task_id),
			title: text(r.title, 200),
			mode: text(r.mode, 64),
			duration: startedMs === null ? '-' : span((nowMs - startedMs) / 1000),
			repos: Array.isArray(r.repos)
				? r.repos
						.filter((x): x is string => typeof x === 'string')
						.map((x) => cleanText(x, 200))
				: [],
			step: text(r.step, 200),
		};
		const agents = runAgents(r.agents);
		if (agents !== undefined) view.agents = agents;
		const workspaceCode = optText(r.workspace_code, 64);
		if (workspaceCode && /^[A-Za-z0-9_-]+$/.test(workspaceCode))
			view.workspaceCode = workspaceCode;
		const sessionId = optText(r.session_id, 128);
		if (sessionId) view.sessionId = sessionId;
		const prUrl = prByTask.get(task);
		if (prUrl) view.prUrl = prUrl;
		return view;
	});

	const counters: CounterView[] = hasStatus
		? [
				{
					key: 'done',
					label: 'done (PR)',
					count: int(tasks.done_pr),
					items: prs.map((p) =>
						p.url ? { task: p.task, url: p.url } : { task: p.task },
					),
				},
				{
					key: 'inWork',
					label: 'in work',
					count: int(tasks.in_work),
					items: runs.map((r) => ({ task: r.task, title: r.title })),
				},
				{
					key: 'needsFeedback',
					label: 'needs feedback',
					count: int(tasks.needs_feedback),
					items: null,
				},
				{
					key: 'blocked',
					label: 'blocked',
					count: int(tasks.blocked),
					items: null,
				},
				{
					key: 'held',
					label: 'held',
					count: int(tasks.held),
					items: reasonItems(resume.held ?? summary.held),
				},
				{
					key: 'failed',
					label: 'failed',
					count: int(tasks.failed),
					items: reasonItems(resume.failed ?? summary.failed),
				},
				{
					key: 'queued',
					label: 'queued',
					count: int(tasks.queued),
					items: null,
				},
			]
		: [];

	const limits = list(d.limits).map((l) => limitView(l, nowMs, tz));
	const settings = rec(d.settings);
	const nextRaw = rec(d.next_stop);
	const pctLeft = num(nextRaw.pct_left);

	return {
		empty: !input.running && !hasStatus,
		emptyText: EMPTY_TEXT,
		emptyCommand: EMPTY_COMMAND,
		header: !hasStatus
			? input.running
				? startingHeader()
				: null
			: statusHeader(d, input.running, nowMs, tz),
		buttons: buttonsFor(input.running, state),
		counters,
		runs,
		limits,
		limitsNote:
			limits.length === 0 && settings.usage_guard === false
				? 'no usage limit (the guard is off)'
				: null,
		nextStop:
			pctLeft === null
				? null
				: `≈ ${Math.round(pctLeft)}% of the ${windowName(
						text(nextRaw.window, 32),
					)} window left before "${text(nextRaw.kind, 200)}"`,
		cost: hasStatus ? costView(d) : null,
		unconfirmedNew: int(d.unconfirmed_new),
		events: list(d.events)
			.slice(-EVENT_LIMIT)
			.reverse()
			.map((e) => eventView(e, tz)),
	};
};
