import type { TaskTimeEntry } from '@/actions/tmgr/timeEntries';
import { formatWorkDuration } from '@/utils/agentWork';
import { format } from 'date-fns';

export interface TimeEntryRow {
	id: number;
	date: string;
	range: string;
	counted: string;
	source: string;
	user: string | null;
	partial: boolean;
}

const SOURCE_LABELS: Record<string, string> = {
	attach: 'Terminal session',
};

export const timeEntrySourceLabel = (source: string): string =>
	SOURCE_LABELS[source] ?? source;

export const shouldLoadTimeEntries = (
	taskId: number | null | undefined,
	workspaceId: number | null | undefined,
): boolean =>
	Number.isFinite(taskId) &&
	Number(taskId) > 0 &&
	!(workspaceId != null && Number(workspaceId) < 0);

export const timeEntryRows = (
	entries: TaskTimeEntry[],
	currentUserId: number | null | undefined,
): TimeEntryRow[] =>
	entries.map((entry) => {
		const start = new Date(entry.started_at);
		const end = new Date(entry.ended_at);
		return {
			id: entry.id,
			date: format(start, 'd MMM yyyy'),
			range: `${format(start, 'HH:mm')}–${format(end, 'HH:mm')}`,
			counted: formatWorkDuration(entry.counted_seconds),
			source: timeEntrySourceLabel(entry.source),
			user:
				entry.user_id !== currentUserId && entry.user_name
					? entry.user_name
					: null,
			partial: entry.counted_seconds < entry.seconds,
		};
	});
