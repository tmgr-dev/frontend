import { t } from '@/i18n';

const GROUP_KEYS = ['pages', 'task', 'notifications'];

const DESCRIPTION_KEYS = [
	'categories',
	'board',
	'workflow',
	'pages',
	'graph',
	'agent_work',
	'dashboard',
	'daily_routines',
	'task.comments',
	'task.countdown',
	'task.files',
	'task.relations',
	'task.checkpoints',
	'task.assignees',
	'exports',
	'default_landing_page',
];

const camelKey = (key: string): string =>
	key.replace(/[._]([a-z])/g, (_, char: string) => char.toUpperCase());

export const humanizeKey = (key: string): string => {
	const spaced = key.replace(/[._]/g, ' ');
	return spaced.charAt(0).toUpperCase() + spaced.slice(1);
};

export const humanizeGroupName = (group: string): string =>
	GROUP_KEYS.includes(group)
		? t(`settingsPages.helpers.featureToggle.groups.${group}`)
		: humanizeKey(group);

export const featureDescription = (
	key: string,
	feature?: { description?: string },
): string =>
	feature?.description ||
	(DESCRIPTION_KEYS.includes(key)
		? t(`settingsPages.helpers.featureToggle.descriptions.${camelKey(key)}`)
		: '');
