import { readFileSync } from 'fs';
import { join } from 'path';
import {
	allowedLandings,
	resolveLanding,
	showSurface,
	SURFACES,
	visibleActiveTasks,
	type SurfaceId,
} from '../moduleSurfaces';
import { isEntryVisible, type ModuleEntry } from '../modules';

const entries = (hiddenKeys: string[], off: string[] = []) => {
	const map: Record<string, ModuleEntry> = {};
	for (const key of Object.keys(SURFACES).flatMap(
		(id) => SURFACES[id as SurfaceId],
	)) {
		map[key] = {
			key,
			core: true,
			enabled: true,
			hidden: hiddenKeys.includes(key),
		};
	}
	for (const key of off) map[key] = { key, enabled: false };
	return (key: string) => isEntryVisible(map[key]);
};

const SURFACE_FILES: Record<SurfaceId, string[]> = {
	'categories.task-picker': ['pages/NewForm.vue'],
	'categories.badge': [
		'components/tasks/TaskBoardCard.vue',
		'components/tasks/TasksListComponent.vue',
		'components/dashboard/RecentTaskItem.vue',
		'components/member/MemberTasksPanel.vue',
	],
	'categories.list-filter': ['pages/TasksListPage.vue'],
	'categories.nav': ['components/general/NavbarMenu.vue'],
	'board.nav': ['components/general/NavbarMenu.vue'],
	'dashboard.nav': ['components/general/NavbarMenu.vue'],
	'routines.nav': ['components/general/NavbarMenu.vue'],
	'timer.board-card': ['components/tasks/TaskBoardCard.vue'],
	'timer.stats': ['components/dashboard/StatisticsGrid.vue'],
	'timer.totals': ['pages/TasksListPage.vue', 'pages/Board.vue'],
	'routines.stats': ['components/dashboard/StatisticsGrid.vue'],
	'checkpoints.board-card': ['components/tasks/TaskBoardCard.vue'],
	'files.nav': ['components/general/CustomSidebar.vue'],
	'timer.tray-recent': ['components/desktop/DesktopTray.vue'],
	'timer.hotkey': ['components/desktop/DesktopHotkeys.vue'],
	'timer.member': [
		'components/member/MemberHeader.vue',
		'components/member/MemberTasksPanel.vue',
		'components/dashboard/TeamMemberItem.vue',
	],
	'comments.task-modal': ['pages/NewForm.vue'],
	'comments.composer': ['pages/NewForm.vue'],
	'comments.rail': ['pages/NewForm.vue'],
	'comments.count': [
		'components/member/MemberHeader.vue',
		'components/member/MemberTasksPanel.vue',
		'components/dashboard/TeamMemberItem.vue',
	],
	'assignees.task-row': ['pages/NewForm.vue'],
	'assignees.board-filter': ['components/general/FiltersBoard.vue'],
};

const read = (file: string) =>
	readFileSync(join(__dirname, '../../', file), 'utf8');

describe('surface visibility rule', () => {
	it.each(Object.keys(SURFACES) as SurfaceId[])(
		'%s is hidden when the user hid its module and shown otherwise',
		(id) => {
			const keys = SURFACES[id];
			expect(showSurface(id, entries([]))).toBe(true);
			expect(showSurface(id, entries([...keys]))).toBe(false);
		},
	);

	it('hides a non-core module that the owner switched off', () => {
		expect(showSurface('dashboard.nav', entries([], ['dashboard']))).toBe(
			false,
		);
	});
});

describe('surface wiring', () => {
	const cases = (Object.keys(SURFACE_FILES) as SurfaceId[]).flatMap((id) =>
		SURFACE_FILES[id].map((file) => [id, file] as const),
	);
	it.each(cases)('%s is used in %s', (id, file) => {
		expect(read(file)).toMatch(
			new RegExp(`showSurface\\(\\s*['"]${id.replace('.', '\\.')}['"]`),
		);
	});

	it('gates the files route', () => {
		expect(read('router/routes.js')).toMatch(/gatedPage\(\s*'task\.files'/);
	});

	it('gates the category form routes', () => {
		expect(read('router/routes.js')).toMatch(
			/ProjectCategoryForm = gatedPage\(\s*'categories'/,
		);
	});
});

describe('timer surfaces', () => {
	const tasks = [{ id: 1 }, { id: 2 }];
	it('empties the running-timer list for every consumer when the timer is hidden', () => {
		expect(visibleActiveTasks(entries(['task.countdown']), tasks)).toEqual([]);
		expect(visibleActiveTasks(entries([]), tasks)).toEqual(tasks);
	});
	it('wires the active list in the app shell', () => {
		expect(read('App.vue')).toContain('visibleActiveTasks(');
	});
});

describe('board and landing page', () => {
	it('falls back to the list when the board is hidden', () => {
		expect(resolveLanding('board', entries(['board']))).toBe('list');
		expect(resolveLanding('board', entries([]))).toBe('board');
	});
	it('only offers modules the user can see', () => {
		expect(allowedLandings(entries(['board'], ['dashboard']))).toEqual([
			'list',
			'daily_routines',
		]);
	});
	it('is used by the router and the store', () => {
		expect(read('router/index.js')).toContain('resolveLanding(');
		expect(read('store/modules/featureToggles.ts')).toContain(
			'allowedLandings(',
		);
	});
});
