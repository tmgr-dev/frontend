import { readFileSync } from 'fs';
import { join } from 'path';

const ANCHORS: Record<string, string[]> = {
	'pages/PagesIndex.vue': ['pages.new', 'pages.import', 'pages.export'],
	'components/pagesNav/SidebarPagesSection.vue': ['pages.tree', 'pages.type-picker'],
	'pages/PageView.vue': ['page.editor', 'page.versions'],
	'components/pages/PageSidePanel.vue': ['page.backlinks'],
	'pages/WorkspaceMap.vue': ['map.canvas', 'map.filters', 'map.search'],
	'pages/DailyRoutine.vue': ['routines.add', 'routines.quick-add'],
	'components/dailyRoutine/RoutineRow.vue': ['routines.instance'],
	'components/dailyRoutine/ViewSwitcher.vue': ['routines.view-switcher'],
	'components/dailyRoutine/EditRoutineModal.vue': ['routine.reminder', 'routine.to-task'],
};

const read = (file: string) => readFileSync(join(__dirname, '../..', file), 'utf8');

describe('S4b pages, map and routines data-tour anchors', () => {
	it.each(Object.entries(ANCHORS).flatMap(([file, list]) => list.map((a) => [file, a])))(
		'%s has %s',
		(file, anchor) => {
			expect(read(file)).toMatch(new RegExp(`data-tour="[^"]*${anchor.replace(/\./g, '\\.')}`));
		},
	);

	it('PageEditor emits through the page event helper', () => {
		expect(read('components/pages/PageEditor.vue')).toContain('emitPageLinkAdded(item.kind)');
	});

	it('EditRoutineModal reports the editor open on mount', () => {
		expect(read('components/dailyRoutine/EditRoutineModal.vue')).toContain(
			'emitRoutineEditorOpened(isNew.value)',
		);
	});
});
