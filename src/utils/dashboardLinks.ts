/**
 * Where a dashboard click should actually go (#8988). The page pushed `/team`, `/tasks/{id}` and
 * `/profile/{id}`, none of which are routes: they matched `/:workspace_code/:category_code` and
 * silently rendered a category that does not exist. A task opens in the modal the rest of the app
 * uses, and a destination with no page yet resolves to nothing rather than to the wrong page.
 */

export interface RouteTarget {
	path: string;
	query?: Record<string, string>;
}

export interface TaskTarget {
	taskId: number;
}

export function dashboardStatisticTarget(filter: Record<string, any>): RouteTarget | null {
	if (filter.view === 'time_tracking') return { path: '/stats' };
	if (filter.view === 'daily_routine') return { path: '/daily-routines' };
	if (filter.view) return null;

	const query: Record<string, string> = {};
	if (filter.status && filter.status !== 'all') query.status = filter.status;
	if (filter.period) query.period = filter.period;
	return { path: '/list', query };
}

export function taskTarget(taskId: number | null | undefined): TaskTarget | null {
	return taskId ? { taskId } : null;
}
