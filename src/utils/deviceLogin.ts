import { t } from '@/i18n';

export const SCOPE_KEYS: Record<string, string> = {
	'workspaces:read': 'settingsPages.helpers.deviceScope.workspacesRead',
	'tasks:read': 'settingsPages.helpers.deviceScope.tasksRead',
	'tasks:write': 'settingsPages.helpers.deviceScope.tasksWrite',
	'tasks:create': 'settingsPages.helpers.deviceScope.tasksCreate',
	'comments:create': 'settingsPages.helpers.deviceScope.commentsCreate',
	'time:read': 'settingsPages.helpers.deviceScope.timeRead',
	'time:write': 'settingsPages.helpers.deviceScope.timeWrite',
	'agent-work:write': 'settingsPages.helpers.deviceScope.agentWorkWrite',
	'agent-status:write': 'settingsPages.helpers.deviceScope.agentStatusWrite',
	'pages:read': 'settingsPages.helpers.deviceScope.pagesRead',
	'pages:write': 'settingsPages.helpers.deviceScope.pagesWrite',
	'pages:append': 'settingsPages.helpers.deviceScope.pagesAppend',
	'notify:send': 'settingsPages.helpers.deviceScope.notifySend',
	'agent-approvals:write':
		'settingsPages.helpers.deviceScope.agentApprovalsWrite',
	'skills:read': 'settingsPages.helpers.deviceScope.skillsRead',
	'skills:write': 'settingsPages.helpers.deviceScope.skillsWrite',
};

export const describeScope = (scope: string): string =>
	scope in SCOPE_KEYS ? t(SCOPE_KEYS[scope]) : scope;

const CODE_LENGTH = 8;

export const normalizeUserCode = (raw: string | null | undefined): string => {
	const compact = String(raw ?? '')
		.toUpperCase()
		.replace(/[^A-Z0-9]/g, '')
		.slice(0, CODE_LENGTH);
	return compact.length > 4
		? `${compact.slice(0, 4)}-${compact.slice(4)}`
		: compact;
};

export const isCompleteUserCode = (code: string): boolean =>
	/^[A-Z0-9]{4}-[A-Z0-9]{4}$/.test(code);

export const toggleScope = (selected: string[], scope: string): string[] =>
	selected.includes(scope)
		? selected.filter((item) => item !== scope)
		: [...selected, scope];

export const sameScopes = (a: string[], b: string[]): boolean =>
	a.length === b.length && a.every((scope) => b.includes(scope));

export const visibleScopes = (scopes: string[] | undefined): string[] =>
	(scopes ?? []).filter((scope) => scope !== 'cli');
