import { addDays, fmtDate, startOfMonth, startOfWeek } from './dateHelpers';

export interface Period {
	from: string;
	to: string;
}

export function periodBounds(
	view: string,
	cursor: Date,
	todayIso: string,
): Period {
	if (view === 'day') {
		const iso = fmtDate(cursor);
		return { from: iso, to: iso };
	}
	if (view === 'week') {
		const start = startOfWeek(cursor);
		return { from: fmtDate(start), to: fmtDate(addDays(start, 6)) };
	}
	if (view === 'month') {
		const start = startOfMonth(cursor);
		const end = new Date(start.getFullYear(), start.getMonth() + 1, 0);
		return { from: fmtDate(start), to: fmtDate(end) };
	}
	return { from: todayIso, to: todayIso };
}

export function countPeriod(
	entries: { date: string; completed?: boolean }[],
	period: Period,
): { routines: number; completed: number } {
	const inside = entries.filter(
		(e) => e.date >= period.from && e.date <= period.to,
	);
	return {
		routines: inside.length,
		completed: inside.filter((e) => e.completed).length,
	};
}
