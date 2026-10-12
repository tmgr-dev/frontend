// Snapshot for local mode only; tours and UI read runtime featureToggles/isFeatureEnabled.
export interface CatalogPack {
	key: string;
	name: string;
}

export interface CatalogModule {
	key: string;
	name: string;
	description: string;
	pack: string;
	clients: string[];
	depends_on: string[];
	scope: 'workspace' | 'user';
	core: boolean;
	enforced: boolean;
	default: boolean;
	/** Works without a TMGR account; the rest is cloud-only and stays off. */
	local: boolean;
}

export interface CatalogPreset {
	key: string;
	name: string;
	description: string;
	modules: string[];
}

export const PACKS: CatalogPack[] = [
	{ key: 'core', name: 'Core' },
	{ key: 'time_focus', name: 'Time & focus' },
	{ key: 'planning', name: 'Planning' },
	{ key: 'knowledge', name: 'Knowledge' },
	{ key: 'ai', name: 'AI agents' },
	{ key: 'integrations', name: 'Integrations' },
	{ key: 'notifications', name: 'Notifications' },
];

const WEB_MOBILE = ['web', 'mobile'];

const entry = (
	key: string,
	name: string,
	description: string,
	pack: string,
	options: Partial<CatalogModule> & { local: boolean },
): CatalogModule => ({
	key,
	name,
	description,
	pack,
	clients: WEB_MOBILE,
	depends_on: [],
	scope: 'workspace',
	core: false,
	enforced: true,
	default: false,
	...options,
});

const core = (key: string, name: string, description: string, clients: string[]) =>
	entry(key, name, description, 'core', { clients, core: true, enforced: true, default: true, local: true });

/** Snapshot of the backend `ModuleCatalog`, except `task.assignees`: core there, but local keeps it switchable and off as before. */
export const MODULES: CatalogModule[] = [
	core('categories', 'Projects', 'Projects with ticket numbers', WEB_MOBILE),
	{ ...core('board', 'Board', 'Kanban board', WEB_MOBILE), depends_on: ['workflow'] },
	core('task.countdown', 'Timer', 'Start and stop a timer on any task', ['web', 'mobile', 'desktop']),
	core('task.comments', 'Comments', 'Comments and reactions', WEB_MOBILE),
	{
		...core('task.assignees', 'Assignees', 'Assign tasks to members', WEB_MOBILE),
		core: false,
		default: false,
	},
	entry('dashboard', 'Dashboard', 'Overview of today and the week', 'time_focus', {
		default: true,
		local: false,
	}),
	entry('pomodoro', 'Pomodoro', '25/5 focus cycles on any task', 'time_focus', { local: false }),
	entry('exports', 'Reports & export', 'Time reports and CSV', 'time_focus', {
		clients: ['web'],
		local: false,
	}),
	entry('workflow', 'Workflow', 'Custom statuses, the status picker and board columns', 'planning', {
		clients: ['web', 'mobile', 'desktop'],
		enforced: false,
		default: true,
		local: true,
	}),
	entry('task.checkpoints', 'Checkpoints', 'Checklist inside a task', 'planning', {
		enforced: false,
		default: true,
		local: true,
	}),
	entry('task.relations', 'Relations', 'Links between tasks (blocks, relates to)', 'planning', {
		default: false,
		local: true,
	}),
	entry('daily_routines', 'Routines', 'Daily and recurring routines', 'planning', {
		clients: ['web', 'mobile', 'desktop'],
		default: false,
		local: true,
	}),
	entry('task.files', 'Attachments & files', 'Attachments on tasks and the Files screen', 'planning', {
		default: true,
		local: true,
	}),
	entry('pages', 'Pages', 'Markdown wiki linked to tasks', 'knowledge', {
		clients: ['web', 'desktop'],
		default: false,
		local: true,
	}),
	entry('graph', 'Graph', 'Map of tasks, pages and agents', 'knowledge', {
		clients: ['web'],
		depends_on: ['pages'],
		local: true,
	}),
	entry('ai.assistant', 'AI assistant', 'Chat and AI replies to comments (uses AI credits)', 'ai', {
		local: false,
	}),
	entry('agent_work', 'Agent work', 'See what agents did and how long', 'ai', { default: false, local: true }),
	entry('personas', 'Personas', 'AI teammates with their own tokens and skills', 'ai', {
		clients: ['web', 'desktop'],
		default: false,
		local: true,
	}),
	entry('cursor', 'Cursor agents', 'Launch Cursor cloud agents from a task', 'ai', { local: false }),
	entry('github', 'GitHub', 'GitHub App: link repos, PRs and issues', 'integrations', { local: false }),
	entry('telegram', 'Telegram bot', 'Create and get tasks in Telegram', 'integrations', { local: false }),
	entry('import', 'Import', 'Import tasks from other tools', 'integrations', { clients: ['web'], local: false }),
	entry('notifications.push', 'Push notifications', 'Push notifications to your devices', 'notifications', {
		clients: ['mobile'],
		enforced: false,
		default: true,
		local: false,
	}),
	entry('notifications.email', 'Email notifications', 'Email notifications', 'notifications', {
		clients: ['web'],
		enforced: false,
		default: true,
		local: false,
	}),
	entry('mcp', 'MCP & API tokens', 'Let Claude Code, Codex and other agents use TMGR', 'core', {
		scope: 'user',
		clients: ['web', 'desktop'],
		enforced: false,
		default: true,
		local: true,
	}),
	entry('alerts', 'Agent alerts & alarms', 'Agents can push and ring you (tmgr-notify, alarms)', 'core', {
		scope: 'user',
		default: true,
		local: false,
	}),
];

/**
 * What a local workspace without a stored choice shows. These are the values the local toggles had
 * before modules existed, so workspaces that were never configured look the same as before.
 */
export const LOCAL_DEFAULTS: Record<string, boolean> = {
	'task.checkpoints': true,
	'task.relations': true,
	'task.files': true,
	daily_routines: true,
	pages: true,
	graph: false,
	agent_work: true,
	personas: true,
	'task.assignees': false,
};

const nonCoreWorkspaceKeys = (): string[] =>
	MODULES.filter((m) => m.scope === 'workspace' && !m.core && m.local).map((m) => m.key);

const onlyLocal = (keys: string[]): string[] => {
	const local = new Set(nonCoreWorkspaceKeys());
	return keys.filter((key) => local.has(key));
};

export const PRESETS: CatalogPreset[] = [
	{
		key: 'minimal',
		name: 'Minimal',
		description: 'Just the essentials',
		modules: [],
	},
	{
		key: 'personal',
		name: 'Personal',
		description: 'Just me — tasks and time',
		modules: onlyLocal(['task.checkpoints', 'task.files', 'daily_routines']),
	},
	{
		key: 'developer',
		name: 'Developer with AI agents',
		description: 'Me and my AI agents',
		modules: onlyLocal(['workflow', 'task.checkpoints', 'task.files', 'agent_work', 'personas']),
	},
	{
		key: 'everything',
		name: 'Everything',
		description: 'Everything',
		modules: nonCoreWorkspaceKeys(),
	},
];
