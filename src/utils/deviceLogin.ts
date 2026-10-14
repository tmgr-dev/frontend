export const SCOPE_DESCRIPTIONS: Record<string, string> = {
	'workspaces:read': 'See your workspaces, statuses and projects',
	'tasks:read': 'Read tasks and their comments',
	'tasks:write': 'Change task status and fields',
	'tasks:create': 'Create tasks',
	'comments:create': 'Add comments',
	'time:read': 'Read time reports',
	'time:write': 'Start and stop your timer',
	'agent-work:write': 'Record AI agent work on tasks',
	'agent-status:write':
		'Share your local AI agent status (working, waiting, done) with your workspace',
	'pages:read': 'Read Pages',
	'pages:write': 'Edit Pages',
	'pages:append': 'Append to Pages',
	'notify:send': 'Send notifications and phone alarms to your devices',
};

export const describeScope = (scope: string): string =>
	SCOPE_DESCRIPTIONS[scope] ?? scope;

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
