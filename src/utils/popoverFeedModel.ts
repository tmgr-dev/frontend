import type { Machine } from '@/actions/tmgr/aiUsageMachines';
import type { PopoverExtras } from '@/popover/types';
import type { RunnerSession } from '@/runner/api';
import type { RoutineEntry } from '@/types/dailyRoutine';
import type { TimeDailyDay } from '@/types/dashboard';
import type { UsageLimit, UsageOverview } from '@/usage/api';

type Usage = NonNullable<PopoverExtras['usage']>;
type Today = NonNullable<PopoverExtras['today']>;
type Routine = Today['routines'][number];
type Machines = NonNullable<PopoverExtras['machines']>;
type Workspace = NonNullable<PopoverExtras['workspace']>;

const FIVE_HOURS_S = 18_000;
const WEEK_S = 604_800;
const ONLINE_WINDOW_MS = 10 * 60_000;
export const LOCAL_MACHINE_ID = 'local';

const finite = (value: unknown): value is number =>
	typeof value === 'number' && Number.isFinite(value);

const epochSeconds = (ms: number): number => Math.floor(ms / 1000);

const parseMs = (iso: string | null | undefined): number =>
	iso ? Date.parse(iso) : NaN;

const clampPct = (value: number): number => Math.min(100, Math.max(0, value));

const newestClaudeLimit = (
	limits: UsageLimit[],
	windowS: number,
): UsageLimit | undefined =>
	limits
		.filter(
			(limit) =>
				limit.tool === 'claude' &&
				limit.window_s === windowS &&
				finite(limit.used_pct),
		)
		.sort(
			(a, b) => (parseMs(b.observed_at) || 0) - (parseMs(a.observed_at) || 0),
		)[0];

const resetsAt = (limit: UsageLimit, now: number): number | null => {
	const absolute = parseMs(limit.resets_at);
	if (Number.isFinite(absolute)) return epochSeconds(absolute);
	if (finite(limit.resets_in_s)) {
		return epochSeconds(now) + Math.round(limit.resets_in_s);
	}
	return null;
};

const usageBar = (limit: UsageLimit, now: number) => ({
	pct: clampPct(limit.used_pct),
	resetsAt: resetsAt(limit, now),
});

export const buildUsageExtras = (
	overview: UsageOverview | null | undefined,
	runningCost?: number | null,
	now = Date.now(),
): Usage | undefined => {
	const usage: Usage = {};
	const fiveHour = newestClaudeLimit(overview?.limits ?? [], FIVE_HOURS_S);
	if (fiveHour) usage.fiveHour = usageBar(fiveHour, now);
	const week = newestClaudeLimit(overview?.limits ?? [], WEEK_S);
	if (week) usage.week = usageBar(week, now);
	const today = overview?.windows?.find((window) => window.key === 'today');
	if (finite(today?.totals?.cost_usd)) usage.todayCost = today.totals.cost_usd;
	if (finite(today?.totals?.tokens_total)) {
		usage.todayTokens = today.totals.tokens_total;
	}
	if (finite(runningCost)) usage.runningCost = runningCost;
	return Object.keys(usage).length ? usage : undefined;
};

const buildRoutines = (entries: RoutineEntry[], today: string): Routine[] => {
	const seen = new Set<number>();
	const routines: Routine[] = [];
	for (const entry of entries) {
		if (entry.date !== today || seen.has(entry.task_id)) continue;
		seen.add(entry.task_id);
		routines.push({
			id: entry.task_id,
			title: entry.title,
			done: entry.completed === true,
		});
	}
	return routines;
};

export const buildTodayExtras = (input: {
	trackedSeconds: number | null | undefined;
	routines: RoutineEntry[] | null | undefined;
	today: string;
}): Today | undefined => {
	if (!finite(input.trackedSeconds)) return undefined;
	return {
		trackedSeconds: input.trackedSeconds,
		routines: buildRoutines(input.routines ?? [], input.today),
		deadlines: [],
	};
};

const isRecent = (lastSeenAt: string, now: number): boolean => {
	const seen = parseMs(lastSeenAt);
	return Number.isFinite(seen) && now - seen <= ONLINE_WINDOW_MS;
};

export const buildMachines = (
	local: RunnerSession[] | null | undefined,
	cloud: Machine[] | null | undefined,
	now: number,
	localLabel: string,
): Machines | undefined => {
	if (!local && !cloud) return undefined;
	const machines: Machines = [];
	if (local) {
		const live = local.filter((session) => session.state.kind !== 'exited');
		machines.push({
			id: LOCAL_MACHINE_ID,
			label: localLabel,
			state: live.some((session) => session.state.kind === 'working')
				? 'busy'
				: 'online',
			sessions: live.length,
			local: true,
		});
	}
	const seen = new Set<string>();
	for (const machine of cloud ?? []) {
		if (!machine.machine_id || seen.has(machine.machine_id)) continue;
		seen.add(machine.machine_id);
		machines.push({
			id: machine.machine_id,
			label: machine.label,
			state: isRecent(machine.last_seen_at, now) ? 'online' : 'offline',
			sessions: null,
			local: false,
		});
	}
	return machines;
};

export const buildTodaySecondsByTask = (
	workspaceId: number,
	taskIds: number[],
	days: TimeDailyDay[],
	today: string,
): Record<string, number> | undefined => {
	const wanted = new Set(taskIds);
	const totals: Record<string, number> = {};
	for (const day of days) {
		if (day.date !== today) continue;
		for (const item of day.items ?? []) {
			const taskId = item.task?.id;
			if (taskId === undefined || !wanted.has(taskId)) continue;
			const key = `${workspaceId}:${taskId}`;
			totals[key] = (totals[key] ?? 0) + (Number(item.seconds) || 0);
		}
	}
	return Object.keys(totals).length ? totals : undefined;
};

export const buildWorkspace = (
	workspace: { name?: unknown; code?: unknown } | null | undefined,
): Workspace | undefined =>
	typeof workspace?.name === 'string' && typeof workspace?.code === 'string'
		? { name: workspace.name, code: workspace.code }
		: undefined;
