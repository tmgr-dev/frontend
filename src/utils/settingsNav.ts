export interface SettingsNavFlags {
	isLocalOnly: boolean;
	isDesktop: boolean;
	hasLocalWorkspaces: boolean;
	runnerSupported: boolean;
	isFeatureEnabled: (key: string) => boolean;
}

export interface SettingsNavItem {
	key: string;
	label: string;
	path: string;
	tab?: string;
	icon: string;
}

export interface SettingsNavGroup {
	key: string;
	label: string;
	items: SettingsNavItem[];
}

const tab = (key: string, label: string, icon: string): SettingsNavItem => ({
	key,
	label,
	path: '/settings',
	tab: key,
	icon,
});

const page = (
	key: string,
	label: string,
	path: string,
	icon: string,
): SettingsNavItem => ({ key, label, path, icon });

const GROUPS: SettingsNavGroup[] = [
	{
		key: 'account',
		label: 'Account',
		items: [
			tab('profile', 'Profile', 'profile'),
			tab('notification', 'Notifications', 'notifications'),
			tab('device', 'Smart Devices', 'devices'),
			tab('privacy', 'Privacy', 'privacy'),
		],
	},
	{
		key: 'workspace',
		label: 'Workspace',
		items: [
			page('workspaces', 'General', '/settings/workspaces', 'workspace'),
			page('modules', 'Modules', '/settings/modules', 'modules'),
			page('personas', 'Personas', '/settings/personas', 'personas'),
		],
	},
	{
		key: 'agents',
		label: 'Agents',
		items: [
			tab('agent-work', 'Agent work', 'agent-work'),
			page(
				'agent-notifications',
				'Agent alerts',
				'/settings/agent-notifications',
				'alerts',
			),
			page('plugins', 'Plugins', '/settings/plugins', 'plugins'),
		],
	},
	{
		key: 'app',
		label: 'App',
		items: [
			tab('theme', 'Theme', 'theme'),
			tab('desktop', 'Shortcuts', 'shortcuts'),
			tab('data', 'Local data', 'local-data'),
			tab('experimental', 'Experimental', 'experimental'),
			tab('about', 'About', 'about'),
		],
	},
];

const VISIBLE: Record<string, (f: SettingsNavFlags) => boolean> = {
	notification: (f) => !f.isLocalOnly,
	device: (f) => !f.isLocalOnly,
	personas: (f) => f.isFeatureEnabled('personas'),
	'agent-work': (f) => !f.isLocalOnly,
	'agent-notifications': (f) => !f.isLocalOnly && f.isFeatureEnabled('alerts'),
	plugins: (f) => f.isDesktop,
	desktop: (f) => f.isDesktop,
	data: (f) => f.hasLocalWorkspaces,
	experimental: (f) => f.runnerSupported,
};

export const SETTINGS_NAV_COLUMN_QUERY = '(min-width: 1280px)';

export function settingsNavGroups(flags: SettingsNavFlags): SettingsNavGroup[] {
	return GROUPS.map((group) => ({
		...group,
		items: group.items.filter((item) => VISIBLE[item.key]?.(flags) ?? true),
	})).filter((group) => group.items.length > 0);
}

export function settingsNavLocation(item: SettingsNavItem) {
	return item.tab
		? { path: item.path, query: { tab: item.tab } }
		: { path: item.path };
}

export function isSettingsNavItemActive(
	item: SettingsNavItem,
	route: { path: string; query: Record<string, unknown> },
): boolean {
	const path = route.path.replace(/\/+$/, '') || '/';
	if (!item.tab) return path === item.path || path.startsWith(`${item.path}/`);
	const current = route.query.tab;
	return (
		path === item.path &&
		typeof current === 'string' &&
		current.toLowerCase() === item.tab
	);
}
