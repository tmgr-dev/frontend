export interface BoardCounts {
	total: number;
	inProgress: number;
	done: number;
	hidden: number;
	percent: number;
}

interface BoardColumnLike {
	status?: { type?: string } | null;
	taskCount?: number;
	tasks?: unknown[];
}

/**
 * How many tasks the board holds, by status type. A column knows its server-side
 * `taskCount`; while it is still loading, the tasks already rendered are counted
 * instead. The archive is skipped: it is not part of the board people look at.
 */
export const boardTaskCounts = (columns: (BoardColumnLike | null | undefined)[]): BoardCounts => {
	const counts = { total: 0, inProgress: 0, done: 0, hidden: 0 };
	for (const column of columns || []) {
		const type = column?.status?.type;
		if (type === 'archived') {
			continue;
		}
		const n = column?.taskCount ?? column?.tasks?.length ?? 0;
		counts.total += n;
		if (type === 'active') counts.inProgress += n;
		else if (type === 'completed') counts.done += n;
		else if (type === 'hidden') counts.hidden += n;
	}
	const percent = counts.total ? Math.round((counts.done / counts.total) * 100) : 0;
	return { ...counts, percent };
};
