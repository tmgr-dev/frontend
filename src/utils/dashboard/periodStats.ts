import type {
	CategoryTotal,
	DailyActivityPoint,
	DashboardPeriod,
	TimeDailyDay,
} from '@/types/dashboard';

const DAY_MS = 86_400_000;

export const periodLength = (period: DashboardPeriod): number =>
	period === '30d' ? 30 : 7;

/** UTC day keys, oldest first, for `length` days ending `back` days before `today`'s UTC date. */
export function utcDayKeys(today: Date, length: number, back = 0): string[] {
	const end = Date.UTC(
		today.getUTCFullYear(),
		today.getUTCMonth(),
		today.getUTCDate(),
	);
	return Array.from({ length }, (_, i) =>
		new Date(end - (back + length - 1 - i) * DAY_MS).toISOString().slice(0, 10),
	);
}

export function countsForDays(
	points: DailyActivityPoint[] | null | undefined,
	keys: string[],
): number[] {
	const byDate = new Map<string, number>();
	for (const p of points ?? [])
		byDate.set(p.date, (byDate.get(p.date) ?? 0) + (Number(p.count) || 0));
	return keys.map((key) => byDate.get(key) ?? 0);
}

export const sumValues = (values: number[]): number =>
	values.reduce((acc, v) => acc + v, 0);

/** Whole percent change; null when there is nothing to compare against (previous 0). */
export function deltaPercent(current: number, previous: number): number | null {
	if (!previous) return null;
	return Math.round(((current - previous) / previous) * 100);
}

type MemberDays = { daily_activity?: DailyActivityPoint[] | null };

export function activeMembersPerDay(
	members: MemberDays[],
	keys: string[],
): number[] {
	const perMember = members.map((m) => countsForDays(m.daily_activity, keys));
	return keys.map((_, i) => perMember.filter((counts) => counts[i] > 0).length);
}

export const activeMembersCount = (members: MemberDays[], keys: string[]) =>
	members.filter((m) =>
		countsForDays(m.daily_activity, keys).some((c) => c > 0),
	).length;

export function trackedSecondsForDays(
	days: TimeDailyDay[],
	keys: string[],
): number[] {
	return countsForDays(
		days.map((d) => ({ date: d.date, count: d.total_s })),
		keys,
	);
}

export const secondsToHours = (seconds: number): number =>
	Math.round((seconds / 3600) * 10) / 10;

export function categoryTotals(
	days: TimeDailyDay[],
	keys: string[],
): CategoryTotal[] {
	const inPeriod = new Set(keys);
	const totals = new Map<string, CategoryTotal>();
	for (const day of days) {
		if (!inPeriod.has(day.date)) continue;
		for (const item of day.items ?? []) {
			const seconds = Number(item.seconds) || 0;
			if (seconds <= 0) continue;
			const c = item.category;
			const key = c ? `c${c.id}` : 'none';
			const name = c ? c.name || c.code || `Category ${c.id}` : 'No category';
			const total = totals.get(key) ?? { key, name, seconds: 0 };
			total.seconds += seconds;
			totals.set(key, total);
		}
	}
	return [...totals.values()].sort(
		(a, b) => b.seconds - a.seconds || a.name.localeCompare(b.name),
	);
}
