export type IsOn = (key: string) => boolean;

export const SURFACES = {
	'categories.task-picker': ['categories'],
	'categories.badge': ['categories'],
	'categories.list-filter': ['categories'],
	'categories.nav': ['categories'],
	'board.nav': ['board'],
	'dashboard.nav': ['dashboard'],
	'routines.nav': ['daily_routines'],
	'timer.board-card': ['task.countdown'],
	'timer.tray-recent': ['task.countdown'],
	'timer.hotkey': ['task.countdown'],
	'timer.member': ['task.countdown'],
	'comments.task-modal': ['task.comments'],
	'comments.composer': ['task.comments'],
	'comments.rail': ['task.comments'],
	'comments.count': ['task.comments'],
	'assignees.task-row': ['task.assignees'],
	'assignees.board-filter': ['task.assignees'],
} as const;

export type SurfaceId = keyof typeof SURFACES;

export const showSurface = (id: SurfaceId, isOn: IsOn): boolean =>
	SURFACES[id].every((key) => isOn(key));

export const visibleActiveTasks = <T>(isOn: IsOn, tasks: T[]): T[] =>
	isOn('task.countdown') ? tasks : [];

export const allowedLandings = (isOn: IsOn): string[] => {
	const allowed = ['list'];
	if (isOn('board')) allowed.push('board');
	if (isOn('dashboard')) allowed.push('dashboard');
	if (isOn('daily_routines')) allowed.push('daily_routines');
	return allowed;
};

export const resolveLanding = (current: unknown, isOn: IsOn): string => {
	const allowed = allowedLandings(isOn);
	return typeof current === 'string' && allowed.includes(current)
		? current
		: allowed[0];
};
