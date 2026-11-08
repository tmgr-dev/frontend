import { t } from '@/i18n';

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

type NavItemDef = Omit<SettingsNavItem, 'label'>;

const tab = (key: string, icon: string): NavItemDef => ({
	key,
	path: '/settings',
	tab: key,
	icon,
});

const page = (key: string, path: string, icon: string): NavItemDef => ({
	key,
	path,
	icon,
});

const GROUPS: { key: string; items: NavItemDef[] }[] = [
	{
		key: 'account',
		items: [
			tab('profile', 'profile'),
			tab('notification', 'notifications'),
			tab('device', 'devices'),
			tab('privacy', 'privacy'),
		],
	},
	{
		key: 'workspace',
		items: [
			page('workspaces', '/settings/workspaces', 'workspace'),
			page('modules', '/settings/modules', 'modules'),
			page('personas', '/settings/personas', 'personas'),
		],
	},
	{
		key: 'agents',
		items: [
			tab('agent-work', 'agent-work'),
			page('agent-notifications', '/settings/agent-notifications', 'alerts'),
			page('plugins', '/settings/plugins', 'plugins'),
		],
	},
	{
		key: 'app',
		items: [
			tab('theme', 'theme'),
			tab('desktop', 'shortcuts'),
			tab('data', 'local-data'),
			tab('experimental', 'experimental'),
			tab('about', 'about'),
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
		key: group.key,
		label: t(`settings.nav.groups.${group.key}`),
		items: group.items
			.filter((item) => VISIBLE[item.key]?.(flags) ?? true)
			.map((item) => ({ ...item, label: t(`settings.nav.items.${item.key}`) })),
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
