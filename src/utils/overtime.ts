/** Overtime = tracked time beyond the estimate, summed over tasks (TM-130). */

export interface EstimatedTask {
	common_time?: number | null;
	approximately_time?: number | string | null;
	settings?: Array<{ key?: string; value?: unknown; pivot?: { value?: unknown } }> | null;
}

export interface OvertimePagination {
	total_overtime_seconds?: string | number | null;
}

const toSeconds = (value: unknown): number => {
	if (typeof value === 'number') return Number.isFinite(value) ? value : 0;
	if (typeof value === 'string' && value.trim() !== '') {
		const n = parseInt(value, 10);
		return Number.isFinite(n) ? n : 0;
	}
	return 0;
};

/** The task's own estimate, else the `approximately_time` setting inherited from its category. */
export const taskEstimateSeconds = (task: EstimatedTask): number => {
	const own = toSeconds(task.approximately_time);
	if (own > 0) return own;
	const setting = task.settings?.find((s) => s?.key === 'approximately_time');
	if (!setting) return 0;
	return toSeconds(setting.value ?? setting.pivot?.value);
};

export const taskOvertimeSeconds = (task: EstimatedTask): number => {
	const estimate = taskEstimateSeconds(task);
	if (estimate <= 0) return 0;
	return Math.max(0, (task.common_time || 0) - estimate);
};

/** Sum over the tasks at hand (one page). */
export const pageOvertimeSeconds = (tasks: EstimatedTask[]): number =>
	tasks.reduce((sum, task) => sum + taskOvertimeSeconds(task), 0);

/**
 * Overtime for the whole result set: the server's `total_overtime_seconds` (computed over every
 * matching row) when present, else the page sum as a fallback for older backends.
 */
export const totalOvertimeSeconds = (
	pagination: OvertimePagination | null | undefined,
	tasks: EstimatedTask[],
): number => {
	const server = pagination?.total_overtime_seconds;
	if (server !== undefined && server !== null && server !== '') return toSeconds(server);
	return pageOvertimeSeconds(tasks);
};
