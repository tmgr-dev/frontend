import { readFileSync } from 'fs';
import { join } from 'path';

const ANCHORS: Record<string, string[]> = {
	'components/desktop/DesktopShortcutsSettings.vue': ['settings.desktop-shortcuts'],
	'pages/NewForm.vue': ['task-form.open-window'],
	'pages/PageView.vue': ['task-form.open-window'],
	'components/desktop/LocalOnlyBanner.vue': ['local.banner'],
	'components/general/CustomSidebar.vue': ['local.badge', 'nav.create-account'],
	'components/desktop/LocalDataSettings.vue': [
		'settings.data-backups',
		'settings.backup-now',
		'settings.data-move',
	],
	'components/local/LocalPersonasPanel.vue': [
		'local-personas.panel',
		'local-personas.access',
		'local-personas.connect',
	],
	'pages/Settings/Plugins.vue': ['plugins.install', 'plugins.help'],
	'components/desktop/DesktopWhatsNew.vue': ['whats-new.item-tour'],
};

const read = (file: string) => readFileSync(join(__dirname, '../..', file), 'utf8');

describe('S8 desktop data-tour anchors', () => {
	it.each(Object.entries(ANCHORS).flatMap(([file, list]) => list.map((a) => [file, a])))(
		'%s has %s',
		(file, anchor) => {
			expect(read(file)).toMatch(new RegExp(`data-tour="[^"]*${anchor.replace(/\./g, '\\.')}`));
		},
	);
});
