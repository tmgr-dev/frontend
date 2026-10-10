import { isEntryVisible, type ModuleEntry } from '@/utils/modules';

const RULES: Array<[RegExp, string]> = [
	[/^daily-routines(\/|$)/, 'daily_routines'],
	[/^workspaces\/[^/]+\/dashboard(\/|$)/, 'dashboard'],
	[/^workspaces\/[^/]+\/files$/, 'task.files'],
	[/^tasks\/[^/]+\/files(\/|$)/, 'task.files'],
	[/^tasks\/[^/]+\/pomodoro(\/|$)/, 'pomodoro'],
	[/^task-relation-types$/, 'task.relations'],
	[/^tasks\/[^/]+\/related-to(\/|$)/, 'task.relations'],
	[/^github(\/|$)/, 'github'],
	[/^categories\/[^/]+\/github(\/|$)/, 'github'],
	[/^tasks\/[^/]+\/github(\/|$)/, 'github'],
	[/^cursor-agents(\/|$)/, 'cursor'],
	[/^categories\/[^/]+\/cursor-/, 'cursor'],
	[/^tasks\/[^/]+\/cursor-agent(\/|$)/, 'cursor'],
	[/^personas(\/|$)/, 'personas'],
	[/^persona-tokens(\/|$)/, 'personas'],
	[
		/^workspaces\/[^/]+\/(personas|assignable-personas|persona-policy)(\/|$)/,
		'personas',
	],
	[/^telegram(\/|$)/, 'telegram'],
	[/^agent(\/|$)/, 'ai.assistant'],
	[/^tasks\/[^/]+\/agent-work$/, 'agent_work'],
	[/^user\/alarm-phone(\/|$)/, 'alerts'],
	[/^notify-tokens(\/|$)/, 'alerts'],
	[/^pages(\/|$)/, 'pages'],
	[/^tasks\/[^/]+\/pages$/, 'pages'],
	[/^graph(\/|$)/, 'graph'],
];

export const moduleKeyForRequest = (url: string | undefined): string | null => {
	if (!url) return null;
	const path = url.split('?')[0].replace(/^\/+/, '');
	return RULES.find(([pattern]) => pattern.test(path))?.[1] ?? null;
};

type EntryLookup = (
	key: string,
) => Pick<ModuleEntry, 'enabled' | 'hidden'> | undefined;

export const shouldBlockRequest = (
	method: string | undefined,
	url: string | undefined,
	entryFor: EntryLookup,
	modulesKnown = true,
): boolean => {
	const key = moduleKeyForRequest(url);
	if (!key) return false;
	const read = ['get', 'head'].includes((method || 'get').toLowerCase());
	if (read && !modulesKnown) return true;
	const entry = entryFor(key);
	if (!entry) return false;
	return read ? !isEntryVisible({ key, ...entry }) : entry.enabled === false;
};

export const workspaceIdInUrl = (url: string | undefined): string | null =>
	url?.match(/^\/*workspaces\/([^/?]+)/)?.[1] ?? null;

export const moduleOffError = (config: { url?: string }, key: string) => {
	const data = {
		error: 'feature_disabled',
		feature: key,
		message: `Module \`${key}\` is off`,
	};
	return Object.assign(new Error(data.message), {
		config,
		isAxiosError: true,
		clientModuleGate: true,
		response: {
			status: 403,
			statusText: 'Forbidden',
			data,
			headers: {},
			config,
		},
	});
};
