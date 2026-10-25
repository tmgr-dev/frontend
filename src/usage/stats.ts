import type { UsageDay, UsageProjectRow, UsageRow } from './api';
import { toolName } from './format';

export const CHART_DAYS = 30;
export const OTHER_KEY = '__other__';

const pad = (n: number) => String(n).padStart(2, '0');

export const dayKey = (date: Date): string =>
	`${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;

export const lastDayKeys = (today: Date, count: number): string[] =>
	Array.from({ length: count }, (_, i) =>
		dayKey(
			new Date(
				today.getFullYear(),
				today.getMonth(),
				today.getDate() - (count - 1 - i),
			),
		),
	);

const hasUsage = (d: UsageDay) =>
	d.cost_usd > 0 || d.tokens_total > 0 || d.events > 0;

export const niceTicks = (max: number): number[] => {
	if (!(max > 0)) return [0, 1];
	const raw = Math.max(max / 4, 0.01);
	const pow = 10 ** Math.floor(Math.log10(raw));
	const f = raw / pow;
	const step = (f <= 1 ? 1 : f <= 2 ? 2 : f <= 5 ? 5 : 10) * pow;
	const n = Math.ceil(max / step - 1e-9);
	return Array.from({ length: n + 1 }, (_, i) => Number((i * step).toFixed(6)));
};

export type ChartMode = 'tool' | 'model';

export interface ChartSeries {
	key: string;
	label: string;
	other: boolean;
}

export interface ChartPart {
	key: string;
	cost: number;
}

export interface ChartDay {
	day: string;
	cost: number;
	tokens: number;
	events: number;
	parts: ChartPart[];
}

export interface DailyChart {
	days: ChartDay[];
	series: ChartSeries[];
	ticks: number[];
	axisMax: number;
	empty: boolean;
}

const seriesLabel = (mode: ChartMode, key: string) =>
	mode === 'tool' ? toolName(key) : key || 'Unknown model';

export const buildDailyChart = (
	days: UsageDay[],
	mode: ChartMode,
	today: Date,
	topN = 5,
): DailyChart => {
	const keys = lastDayKeys(today, CHART_DAYS);
	const inWindow = new Set(keys);
	const entries = days.filter((d) => inWindow.has(d.day));
	const keyOf = (d: UsageDay) => (mode === 'tool' ? d.tool : d.model);

	const totals = new Map<string, { cost: number; tokens: number }>();
	for (const d of entries) {
		const t = totals.get(keyOf(d)) ?? { cost: 0, tokens: 0 };
		t.cost += d.cost_usd;
		t.tokens += d.tokens_total;
		totals.set(keyOf(d), t);
	}
	const ranked = [...totals.entries()]
		.sort(
			([ak, a], [bk, b]) =>
				b.cost - a.cost || b.tokens - a.tokens || ak.localeCompare(bk),
		)
		.map(([key]) => key);
	const top = ranked.slice(0, topN);
	const topSet = new Set(top);
	const series: ChartSeries[] = top.map((key) => ({
		key,
		label: seriesLabel(mode, key),
		other: false,
	}));
	const hasOther = ranked.length > top.length;
	if (hasOther) series.push({ key: OTHER_KEY, label: 'Other', other: true });

	const byDay = new Map<string, ChartDay & { costs: Map<string, number> }>();
	for (const day of keys)
		byDay.set(day, {
			day,
			cost: 0,
			tokens: 0,
			events: 0,
			parts: [],
			costs: new Map(),
		});
	for (const d of entries) {
		const entry = byDay.get(d.day)!;
		entry.cost += d.cost_usd;
		entry.tokens += d.tokens_total;
		entry.events += d.events;
		const key = topSet.has(keyOf(d)) ? keyOf(d) : OTHER_KEY;
		entry.costs.set(key, (entry.costs.get(key) ?? 0) + d.cost_usd);
	}
	const chartDays: ChartDay[] = keys.map((day) => {
		const { costs, ...rest } = byDay.get(day)!;
		const parts = series
			.map((s) => ({ key: s.key, cost: costs.get(s.key) ?? 0 }))
			.filter((p) => p.cost > 0);
		return { ...rest, parts };
	});

	const max = Math.max(...chartDays.map((d) => d.cost), 0);
	const ticks = niceTicks(max);
	return {
		days: chartDays,
		series,
		ticks,
		axisMax: ticks[ticks.length - 1],
		empty: !entries.some(hasUsage),
	};
};

export interface Delta {
	current: number;
	previous: number;
	pct: number | null;
}

export interface WeekComparison {
	cost: Delta;
	tokens: Delta;
	events: Delta;
}

const delta = (current: number, previous: number): Delta => ({
	current,
	previous,
	pct: previous > 0 ? ((current - previous) / previous) * 100 : null,
});

export const weekComparison = (
	days: UsageDay[],
	today: Date,
): WeekComparison => {
	const keys = lastDayKeys(today, 14);
	const sum = (from: number, to: number) => {
		const set = new Set(keys.slice(from, to));
		const out = { cost: 0, tokens: 0, events: 0 };
		for (const d of days) {
			if (!set.has(d.day)) continue;
			out.cost += d.cost_usd;
			out.tokens += d.tokens_total;
			out.events += d.events;
		}
		return out;
	};
	const previous = sum(0, 7);
	const current = sum(7, 14);
	return {
		cost: delta(current.cost, previous.cost),
		tokens: delta(current.tokens, previous.tokens),
		events: delta(current.events, previous.events),
	};
};

export const averageCostPerDay = (
	days: UsageDay[],
	today: Date,
): { perDay: number; spanDays: number } => {
	const keys = lastDayKeys(today, CHART_DAYS);
	const inWindow = new Set(keys);
	const entries = days.filter((d) => inWindow.has(d.day));
	if (!entries.length) return { perDay: 0, spanDays: CHART_DAYS };
	const first = entries.reduce(
		(min, d) => (d.day < min ? d.day : min),
		entries[0].day,
	);
	const spanDays = Math.max(1, CHART_DAYS - keys.indexOf(first));
	const cost = entries.reduce((sum, d) => sum + d.cost_usd, 0);
	return { perDay: cost / spanDays, spanDays };
};

export interface ProjectLine {
	key: string;
	label: string;
	cost_usd: number;
	tokens_total: number;
	events: number;
	unpriced_events: number;
}

export const projectLines = (
	projects: UsageProjectRow[],
	limit = 10,
): { lines: ProjectLine[]; more: number } => {
	const sorted = [...projects].sort(
		(a, b) => b.cost_usd - a.cost_usd || b.tokens_total - a.tokens_total,
	);
	const base = sorted.map((p) => p.name.trim() || 'No project');
	const seen = new Map<string, number>();
	for (const label of base) seen.set(label, (seen.get(label) ?? 0) + 1);
	const lines = sorted.map((p, i) => ({
		key: p.key,
		label:
			(seen.get(base[i]) ?? 0) > 1 && p.key
				? `${base[i]} · ${p.key.slice(0, 6)}`
				: base[i],
		cost_usd: p.cost_usd,
		tokens_total: p.tokens_total,
		events: p.events,
		unpriced_events: p.unpriced_events,
	}));
	return {
		lines: lines.slice(0, limit),
		more: Math.max(0, lines.length - limit),
	};
};

export interface TokenTypeRow {
	key: 'input' | 'output' | 'cache_read' | 'cache_write';
	label: string;
	tokens: number;
	share: number;
	cost: number;
}

const TOKEN_TYPES: [TokenTypeRow['key'], string][] = [
	['input', 'Input'],
	['output', 'Output'],
	['cache_read', 'Cache read'],
	['cache_write', 'Cache write'],
];

export const tokenTypeRows = (
	totals: UsageRow,
): { rows: TokenTypeRow[]; noSplit: number | null } => {
	const all = TOKEN_TYPES.reduce((sum, [key]) => sum + (totals[key] ?? 0), 0);
	const rows = TOKEN_TYPES.map(([key, label]) => ({
		key,
		label,
		tokens: totals[key] ?? 0,
		share: all > 0 ? ((totals[key] ?? 0) / all) * 100 : 0,
		cost: totals.cost_by_type?.[key] ?? 0,
	}));
	const rest = totals.cost_usd - rows.reduce((sum, r) => sum + r.cost, 0);
	return { rows, noSplit: rest > 0.005 ? rest : null };
};
