import { readFileSync } from 'fs';
import { join } from 'path';

const ANCHORS: Record<string, string[]> = {
	'pages/Settings.vue': ['settings.device-token', 'settings.telegram', 'settings.theme'],
	'components/privacy/PrivacySettings.vue': ['settings.privacy-telemetry'],
	'pages/Settings/Modules.vue': [
		'modules.presets',
		'modules.list',
		'modules.hide',
		'modules.graph',
		'settings.default-landing',
	],
	'pages/Settings/AgentNotifications.vue': [
		'agent-notify.connect',
		'agent-notify.command',
		'agent-notify.test',
	],
	'pages/Settings/Personas.vue': [
		'personas.create',
		'personas.grants',
		'personas.skills',
		'personas.tokens',
	],
	'pages/Settings/Workspaces.vue': ['workspace.members'],
	'components/dashboard/TeamActivityWidget.vue': ['dashboard.activity', 'dashboard.member'],
	'components/notifications/NotificationBell.vue': ['nav.notification-bell'],
};

describe('S4a settings-area data-tour anchors', () => {
	it.each(Object.entries(ANCHORS).flatMap(([file, list]) => list.map((a) => [file, a])))(
		'%s has %s',
		(file, anchor) => {
			const source = readFileSync(join(__dirname, '../..', file), 'utf8');
			expect(source).toContain(anchor);
			expect(source).toMatch(new RegExp(`data-tour="[^"]*'?${anchor.replace(/\./g, '\\.')}`));
		},
	);
});
