const GROUP_LABELS: Record<string, string> = {
	pages: 'Pages',
	task: 'Tasks',
	notifications: 'Notifications',
};

const DESCRIPTIONS: Record<string, string> = {
	categories: 'Group tasks into categories to organize your workspace.',
	board: 'Switch to a kanban board view for tracking task status.',
	pages: 'Keep notes, context and documentation in a tree of pages that people and agents can edit.',
	dashboard: 'See an overview of activity and progress across your workspace.',
	daily_routines: 'Track recurring routines and notes separate from your tasks.',
	'task.comments': 'Allow comments and discussion on tasks.',
	'task.countdown': 'Track time spent on tasks with a start/stop timer.',
	'task.files': 'Allow files to be attached to tasks.',
	'task.relations': 'Allow tasks to be linked to related tasks.',
	'task.checkpoints': 'Break tasks down into smaller checkpoints.',
	'task.assignees': 'Assign tasks to specific workspace members.',
	exports: 'Export workspace data.',
	default_landing_page: 'Choose which page opens first when you log in.',
};

export const humanizeKey = (key: string): string => {
	const spaced = key.replace(/[._]/g, ' ');
	return spaced.charAt(0).toUpperCase() + spaced.slice(1);
};

export const humanizeGroupName = (group: string): string =>
	GROUP_LABELS[group] ?? humanizeKey(group);

export const featureDescription = (
	key: string,
	feature?: { description?: string },
): string => feature?.description || DESCRIPTIONS[key] || '';
