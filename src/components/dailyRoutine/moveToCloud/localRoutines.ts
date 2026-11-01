import { activeLocalWorkspace, localContext } from '@/local/runtime';
import type { LocalPattern, LocalRoutine } from './payload';

const parseDays = (value: unknown): string[] => {
	try {
		const days = typeof value === 'string' ? JSON.parse(value) : value;
		return Array.isArray(days) ? days.map(String) : [];
	} catch {
		return [];
	}
};

/** Every active routine of the open local workspace, with its pattern and its next pending slot. */
export const readLocalRoutines = async (): Promise<LocalRoutine[]> => {
	const workspace = activeLocalWorkspace();
	if (!workspace) return [];
	const { db } = await localContext(workspace, { id: 0, name: '', email: '' });
	const [routines, patterns, instances] = await Promise.all([
		db.select<any>(
			`SELECT id, title, description, routine_category, approximately_time FROM routines
			 WHERE deleted_at IS NULL AND archived_at IS NULL ORDER BY id DESC`,
		),
		db.select<any>(`SELECT * FROM routine_patterns`),
		db.select<any>(
			`SELECT routine_id, scheduled_for, status FROM routine_instances ORDER BY scheduled_for ASC`,
		),
	]);
	const patternOf = new Map<number, LocalPattern>();
	for (const row of patterns) {
		if (!row.frequency) continue;
		patternOf.set(Number(row.routine_id), {
			frequency: String(row.frequency),
			interval: row.interval ?? null,
			days_of_week: parseDays(row.days_of_week),
			day_of_frequency: row.day_of_frequency ?? null,
			month: row.month ?? null,
			start_at: row.start_at ?? null,
			end_at: row.end_at ?? null,
			occurrences: row.occurrences ?? null,
			scheduled_time: row.scheduled_time ?? null,
			duration_min: row.duration_min ?? null,
			reminder_min: row.reminder_min ?? null,
		});
	}
	const nextPending = new Map<number, string>();
	const hasHistory = new Set<number>();
	for (const row of instances) {
		const id = Number(row.routine_id);
		if (row.status === 'PENDING') {
			if (!nextPending.has(id)) nextPending.set(id, String(row.scheduled_for));
		} else {
			hasHistory.add(id);
		}
	}
	return routines.map((row: any) => {
		const id = Number(row.id);
		const [date, time] = (nextPending.get(id) ?? '').split('T');
		return {
			id,
			title: String(row.title),
			description: row.description ?? null,
			category: row.routine_category ?? null,
			approximatelyTime: row.approximately_time ?? null,
			pattern: patternOf.get(id) ?? null,
			scheduledDate: date || null,
			scheduledTime: time || null,
			hasHistory: hasHistory.has(id),
		};
	});
};
