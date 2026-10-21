import {
	isSettingsNavItemActive,
	settingsNavGroups,
	settingsNavLocation,
	type SettingsNavFlags,
	type SettingsNavItem,
} from '@/utils/settingsNav';

const ALL_ON: SettingsNavFlags = {
	isLocalOnly: false,
	isDesktop: true,
	hasLocalWorkspaces: true,
	runnerSupported: true,
	isFeatureEnabled: () => true,
};

const ALL_OFF: SettingsNavFlags = {
	isLocalOnly: false,
	isDesktop: false,
	hasLocalWorkspaces: false,
	runnerSupported: false,
	isFeatureEnabled: () => false,
};

const shape = (flags: SettingsNavFlags) =>
	Object.fromEntries(
		settingsNavGroups(flags).map((g) => [g.label, g.items.map((i) => i.label)]),
	);

const keys = (flags: SettingsNavFlags) =>
	settingsNavGroups(flags).flatMap((g) => g.items.map((i) => i.key));

const item = (key: string): SettingsNavItem =>
	settingsNavGroups(ALL_ON)
		.flatMap((g) => g.items)
		.find((i) => i.key === key)!;

describe('settingsNavGroups', () => {
	it('shows every group and item when all flags are on', () => {
		expect(shape(ALL_ON)).toEqual({
			Account: ['Profile', 'Notifications', 'Smart Devices', 'Privacy'],
			Workspace: ['General', 'Modules', 'Personas'],
			Agents: ['Agent work', 'Agent alerts', 'Plugins'],
			App: ['Theme', 'Shortcuts', 'Local data', 'Experimental', 'About'],
		});
	});

	it('cloud web user with every optional flag off keeps the cloud items', () => {
		expect(shape(ALL_OFF)).toEqual({
			Account: ['Profile', 'Notifications', 'Smart Devices', 'Privacy'],
			Workspace: ['General', 'Modules'],
			Agents: ['Agent work'],
			App: ['Theme', 'About'],
		});
	});

	it('local-only with all optional flags off: Profile, Privacy, General, Modules, Theme, About', () => {
		expect(shape({ ...ALL_OFF, isLocalOnly: true })).toEqual({
			Account: ['Profile', 'Privacy'],
			Workspace: ['General', 'Modules'],
			App: ['Theme', 'About'],
		});
	});

	it('local-only hides Notifications, Smart Devices, Agent work and Agent alerts', () => {
		const visible = keys({ ...ALL_ON, isLocalOnly: true });
		for (const key of [
			'notification',
			'device',
			'agent-work',
			'agent-notifications',
		])
			expect(visible).not.toContain(key);
		expect(visible).toContain('plugins');
	});

	it('Personas follows the personas module', () => {
		const only = (k: string) => (key: string) => key === k;
		expect(keys({ ...ALL_OFF, isFeatureEnabled: only('personas') })).toContain(
			'personas',
		);
		expect(
			keys({ ...ALL_ON, isFeatureEnabled: (k) => k !== 'personas' }),
		).not.toContain('personas');
	});

	it('Agent alerts needs the alerts module and a cloud account', () => {
		expect(
			keys({ ...ALL_ON, isFeatureEnabled: (k) => k !== 'alerts' }),
		).not.toContain('agent-notifications');
		expect(
			keys({ ...ALL_OFF, isFeatureEnabled: (k) => k === 'alerts' }),
		).toContain('agent-notifications');
	});

	it('Plugins and Shortcuts are desktop only', () => {
		expect(keys({ ...ALL_ON, isDesktop: false })).not.toEqual(
			expect.arrayContaining(['plugins']),
		);
		expect(keys({ ...ALL_ON, isDesktop: false })).not.toContain('desktop');
		expect(keys({ ...ALL_OFF, isDesktop: true })).toEqual(
			expect.arrayContaining(['plugins', 'desktop']),
		);
	});

	it('Local data follows hasLocalWorkspaces', () => {
		expect(keys({ ...ALL_ON, hasLocalWorkspaces: false })).not.toContain(
			'data',
		);
		expect(keys({ ...ALL_OFF, hasLocalWorkspaces: true })).toContain('data');
	});

	it('Experimental follows runner support', () => {
		expect(keys({ ...ALL_ON, runnerSupported: false })).not.toContain(
			'experimental',
		);
		expect(keys({ ...ALL_OFF, runnerSupported: true })).toContain(
			'experimental',
		);
	});

	it('drops the Agents group only when none of its items is visible', () => {
		expect(shape({ ...ALL_OFF, isLocalOnly: true }).Agents).toBeUndefined();
		expect(
			shape({ ...ALL_OFF, isLocalOnly: true, isDesktop: true }).Agents,
		).toEqual(['Plugins']);
		expect(
			settingsNavGroups({ ...ALL_OFF, isFeatureEnabled: (k) => k === 'alerts' })
				.find((g) => g.key === 'agents')
				?.items.map((i) => i.key),
		).toEqual(['agent-work', 'agent-notifications']);
	});
});

describe('settingsNavLocation', () => {
	it('tab items open /settings with the tab query; pages open their path', () => {
		expect(settingsNavLocation(item('privacy'))).toEqual({
			path: '/settings',
			query: { tab: 'privacy' },
		});
		expect(settingsNavLocation(item('workspaces'))).toEqual({
			path: '/settings/workspaces',
		});
		expect(settingsNavLocation(item('agent-notifications'))).toEqual({
			path: '/settings/agent-notifications',
		});
	});
});

describe('isSettingsNavItemActive', () => {
	const at = (path: string, query: Record<string, unknown> = {}) => ({
		path,
		query,
	});

	it('matches a tab item by path and tab only', () => {
		expect(
			isSettingsNavItemActive(item('theme'), at('/settings', { tab: 'theme' })),
		).toBe(true);
		expect(
			isSettingsNavItemActive(item('theme'), at('/settings', { tab: 'about' })),
		).toBe(false);
		expect(isSettingsNavItemActive(item('theme'), at('/settings'))).toBe(false);
		expect(
			isSettingsNavItemActive(
				item('theme'),
				at('/settings/workspaces', { tab: 'theme' }),
			),
		).toBe(false);
	});

	it('compares the tab case-insensitively like Settings.vue', () => {
		expect(
			isSettingsNavItemActive(
				item('privacy'),
				at('/settings', { tab: 'Privacy' }),
			),
		).toBe(true);
	});

	it('highlights exactly one item per settings route', () => {
		const all = settingsNavGroups(ALL_ON).flatMap((g) => g.items);
		const active = (route: ReturnType<typeof at>) =>
			all.filter((i) => isSettingsNavItemActive(i, route)).map((i) => i.key);
		expect(active(at('/settings', { tab: 'profile' }))).toEqual(['profile']);
		expect(active(at('/settings/workspaces'))).toEqual(['workspaces']);
		expect(active(at('/settings/modules'))).toEqual(['modules']);
		expect(active(at('/settings/personas'))).toEqual(['personas']);
		expect(active(at('/settings/agent-notifications'))).toEqual([
			'agent-notifications',
		]);
		expect(active(at('/settings/plugins'))).toEqual(['plugins']);
		expect(active(at('/settings/plugins/help'))).toEqual(['plugins']);
		expect(active(at('/settings/plugins/'))).toEqual(['plugins']);
		expect(active(at('/settings/sections'))).toEqual([]);
		expect(active(at('/settings', { tab: 'nope' }))).toEqual([]);
	});

	it('does not treat a longer sibling path as a child', () => {
		expect(
			isSettingsNavItemActive(item('plugins'), at('/settings/pluginsx')),
		).toBe(false);
	});
});
