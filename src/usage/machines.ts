import type {
	DailyQuery,
	DailyRow,
	Machine,
	MachineLimit,
} from '@/actions/tmgr/aiUsageMachines';
import { t, te } from '@/i18n';

export type Period = 'today' | '7d' | '30d' | '90d';
export type LimitLevel = 'ok' | 'warn' | 'critical';
export type StackBy = 'machine' | 'agent';
export type Metric = 'cost' | 'tokens';

export const PERIODS: Period[] = ['today', '7d', '30d', '90d'];
export const UNASSIGNED = 'none';

const PERIOD_DAYS: Record<Period, number> = {
	today: 1,
	'7d': 7,
	'30d': 30,
	'90d': 90,
};

const pad = (n: number) => String(n).padStart(2, '0');

export const localDate = (d: Date): string =>
	`${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;

export const periodRange = (
	period: Period,
	now: Date,
): { from: string; to: string } => {
	const start = new Date(
		now.getFullYear(),
		now.getMonth(),
		now.getDate() - (PERIOD_DAYS[period] - 1),
	);
	return { from: localDate(start), to: localDate(now) };
};

export interface Filters {
	machineId: string;
	agent: string;
	model: string;
	taskId: string;
}

export const emptyFilters = (): Filters => ({
	machineId: '',
	agent: '',
	model: '',
	taskId: '',
});

export const dailyQuery = (
	range: { from: string; to: string },
	filters: Filters,
): DailyQuery => {
	const query: DailyQuery = { from: range.from, to: range.to };
	if (filters.machineId) query.machineId = filters.machineId;
	if (filters.agent) query.agent = filters.agent;
	if (filters.model) query.model = filters.model;
	const taskId = Number(filters.taskId);
	if (filters.taskId === UNASSIGNED) query.taskId = 0;
	else if (taskId > 0) query.taskId = taskId;
	return query;
};

const num = (value: unknown): number => {
	const n = Number(value);
	return Number.isFinite(n) ? n : 0;
};

export const rowTokens = (r: DailyRow): number =>
	num(r.input_tokens) +
	num(r.output_tokens) +
	num(r.cache_read_tokens) +
	num(r.cache_write_tokens) +
	num(r.reasoning_tokens);

export const isUnassigned = (r: DailyRow): boolean => !(num(r.task_id) > 0);

export const taskLabel = (r: DailyRow): string => r.task_key || `#${r.task_id}`;

export interface FilterOptions {
	machines: { id: string; label: string }[];
	agents: string[];
	models: string[];
	tasks: { id: string; label: string }[];
	hasUnassigned: boolean;
}

export const filterOptions = (
	rows: DailyRow[],
	machines: Machine[],
): FilterOptions => {
	const byId = new Map(machines.map((m) => [m.machine_id, m.label]));
	const machineIds = new Set<string>(byId.keys());
	const agents = new Set<string>();
	const models = new Set<string>();
	const tasks = new Map<string, string>();
	let hasUnassigned = false;
	for (const r of rows) {
		machineIds.add(r.machine_id);
		agents.add(r.agent);
		models.add(r.model);
		if (isUnassigned(r)) hasUnassigned = true;
		else tasks.set(String(r.task_id), taskLabel(r));
	}
	const sort = (a: string, b: string) => a.localeCompare(b);
	return {
		machines: [...machineIds]
			.map((id) => ({ id, label: byId.get(id) ?? id }))
			.sort((a, b) => sort(a.label, b.label)),
		agents: [...agents].sort(sort),
		models: [...models].sort(sort),
		tasks: [...tasks]
			.map(([id, label]) => ({ id, label }))
			.sort((a, b) => Number(a.id) - Number(b.id)),
		hasUnassigned,
	};
};

export interface Totals {
	tokens: number;
	cost: number;
	sessions: number;
	events: number;
}

export const aggregateTotals = (rows: DailyRow[]): Totals =>
	rows.reduce<Totals>(
		(acc, r) => ({
			tokens: acc.tokens + rowTokens(r),
			cost: acc.cost + num(r.cost_usd),
			sessions: acc.sessions + num(r.sessions),
			events: acc.events + num(r.events),
		}),
		{ tokens: 0, cost: 0, sessions: 0, events: 0 },
	);

const AGENT_NAMES: Record<string, string> = {
	'claude-code': 'Claude Code',
	codex: 'Codex',
};

export const agentName = (agent: string): string => AGENT_NAMES[agent] ?? agent;

export const windowLabel = (window: string): string => {
	if (window === 'session') return t('agents.usage.machines.windows.session');
	if (window === 'weekly') return t('agents.usage.machines.windows.weekly');
	if (window.startsWith('model:'))
		return t('agents.usage.machines.windows.model', {
			model: window.slice('model:'.length),
		});
	return window;
};

export interface SeriesEntry {
	key: string;
	label: string;
	total: number;
}

export interface SeriesDay {
	day: string;
	total: number;
	parts: Record<string, number>;
}

export interface DailySeries {
	days: SeriesDay[];
	series: SeriesEntry[];
	max: number;
	empty: boolean;
}

const addDays = (day: string, n: number): string => {
	const [y, m, d] = day.split('-').map(Number);
	const date = new Date(Date.UTC(y, m - 1, d + n));
	return `${date.getUTCFullYear()}-${pad(date.getUTCMonth() + 1)}-${pad(date.getUTCDate())}`;
};

export const buildDailySeries = (
	rows: DailyRow[],
	range: { from: string; to: string },
	options: { by: StackBy; metric: Metric; machines: Machine[] },
): DailySeries => {
	const days = new Map<string, SeriesDay>();
	for (let d = range.from; d <= range.to; d = addDays(d, 1))
		days.set(d, { day: d, total: 0, parts: {} });
	const labels = new Map(options.machines.map((m) => [m.machine_id, m.label]));
	const totals = new Map<string, SeriesEntry>();
	for (const r of rows) {
		const day = days.get(r.day);
		if (!day) continue;
		const value = options.metric === 'cost' ? num(r.cost_usd) : rowTokens(r);
		const key = options.by === 'machine' ? r.machine_id : r.agent;
		day.parts[key] = (day.parts[key] ?? 0) + value;
		day.total += value;
		const entry = totals.get(key) ?? {
			key,
			label:
				options.by === 'machine' ? (labels.get(key) ?? key) : agentName(key),
			total: 0,
		};
		entry.total += value;
		totals.set(key, entry);
	}
	const list = [...days.values()];
	const max = list.reduce((m, d) => Math.max(m, d.total), 0);
	return {
		days: list,
		series: [...totals.values()].sort(
			(a, b) => b.total - a.total || a.label.localeCompare(b.label),
		),
		max,
		empty: max === 0,
	};
};

export interface LimitState {
	pct: number;
	level: LimitLevel;
	reset: boolean;
}

const time = (iso: string | null): number | null => {
	if (!iso) return null;
	const ms = Date.parse(iso);
	return Number.isFinite(ms) ? ms : null;
};

export const limitLevel = (pct: number): LimitLevel =>
	pct >= 95 ? 'critical' : pct >= 80 ? 'warn' : 'ok';

export const limitState = (limit: MachineLimit, now: Date): LimitState => {
	const resetsAt = time(limit.resets_at);
	if (resetsAt !== null && resetsAt < now.getTime())
		return { pct: 0, level: 'ok', reset: true };
	const pct = Math.min(100, Math.max(0, num(limit.used_pct)));
	return { pct, level: limitLevel(pct), reset: false };
};

export const countdownText = (resetsAt: string | null, now: Date): string => {
	const at = time(resetsAt);
	if (at === null) return '';
	const ms = at - now.getTime();
	if (ms < 60_000) return t('agents.usage.machines.countdown.soon');
	const minutes = Math.floor(ms / 60_000);
	const d = Math.floor(minutes / 1440);
	const h = Math.floor((minutes % 1440) / 60);
	const m = minutes % 60;
	if (d > 0) return t('agents.usage.machines.countdown.dh', { d, h });
	if (h > 0) return t('agents.usage.machines.countdown.hm', { h, m });
	return t('agents.usage.machines.countdown.m', { m });
};

export const ageText = (iso: string | null, now: Date): string => {
	const at = time(iso);
	if (at === null) return '';
	const seconds = Math.max(0, Math.floor((now.getTime() - at) / 1000));
	if (seconds < 60) return t('agents.usage.machines.age.justNow');
	if (seconds < 3600)
		return t('agents.usage.machines.age.m', { n: Math.floor(seconds / 60) });
	if (seconds < 86400)
		return t('agents.usage.machines.age.h', { n: Math.floor(seconds / 3600) });
	return t('agents.usage.machines.age.d', { n: Math.floor(seconds / 86400) });
};

export interface LimitRowView {
	key: string;
	label: string;
	account: string;
	pct: number;
	level: LimitLevel;
	reset: boolean;
	countdown: string;
	asOf: string;
}

const windowRank = (window: string): number =>
	window === 'session' ? 0 : window === 'weekly' ? 1 : 2;

export const limitRows = (limits: MachineLimit[], now: Date): LimitRowView[] =>
	[...limits]
		.sort(
			(a, b) =>
				a.agent.localeCompare(b.agent) ||
				a.account.localeCompare(b.account) ||
				windowRank(a.window) - windowRank(b.window) ||
				a.window.localeCompare(b.window),
		)
		.map((l) => {
			const state = limitState(l, now);
			const sourceKey = `agents.usage.machines.sources.${l.source}`;
			return {
				key: `${l.agent}/${l.account}/${l.window}`,
				label: `${agentName(l.agent)} · ${windowLabel(l.window)}`,
				account: l.account,
				...state,
				countdown: state.reset ? '' : countdownText(l.resets_at, now),
				asOf: t('agents.usage.machines.limits.asOf', {
					age: ageText(l.observed_at, now),
					source: te(sourceKey) ? t(sourceKey) : l.source,
				}),
			};
		});

export interface ModelTaskRow extends Totals {
	key: string;
	agent: string;
	model: string;
	task: string | null;
}

export const groupByModelTask = (rows: DailyRow[]): ModelTaskRow[] => {
	const groups = new Map<string, ModelTaskRow>();
	for (const r of rows) {
		const task = isUnassigned(r) ? null : taskLabel(r);
		const key = `${r.agent}|${r.model}|${task ?? ''}`;
		const g = groups.get(key) ?? {
			key,
			agent: r.agent,
			model: r.model,
			task,
			tokens: 0,
			cost: 0,
			sessions: 0,
			events: 0,
		};
		g.tokens += rowTokens(r);
		g.cost += num(r.cost_usd);
		g.sessions += num(r.sessions);
		g.events += num(r.events);
		groups.set(key, g);
	}
	return [...groups.values()].sort(
		(a, b) => b.cost - a.cost || b.tokens - a.tokens,
	);
};
